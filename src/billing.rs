use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DirectLine {
    pub price_id: i64,
    pub description: String,
    pub unit: String,
    pub quantity: i64,
    pub unit_price: Option<i64>,
    pub tax_code: Option<i64>,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DirectInvoice {
    pub request_key: String,
    pub customer: i64,
    pub date: String,
    pub currency: String,
    pub title: String,
    pub lines: Vec<DirectLine>,
}
pub fn direct_tax_code(c: &Connection, customer: i64, price: i64) -> Result<i64> {
    Ok(c.query_row("SELECT COALESCE(NULLIF((SELECT tax_code FROM prices WHERE id=?1),0),(SELECT code FROM tax_assignments WHERE entity='customer' AND record=?2),0)",params![price,customer],|r|r.get(0))?)
}
pub fn direct_unit_price(
    c: &Connection,
    line: &DirectLine,
    currency: &str,
    date: &str,
) -> Result<i64> {
    if let Some(price) = line.unit_price {
        ensure!(price >= 0, "单价不可为负");
        return Ok(price);
    }
    if line.price_id == 0 {
        return Ok(0);
    }
    let (price, source): (i64, String) = c
        .query_row(
            "SELECT price,currency FROM prices WHERE id=?",
            [line.price_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .context("价格单业务不存在")?;
    crate::convert(price, crate::fx::dated(c, &source, currency, date)?)
}
pub fn direct_preview(c: &Connection, draft: &DirectInvoice) -> Result<Value> {
    crate::required(&draft.request_key)?;
    ensure!(draft.request_key.len() <= 128, "请求标识过长");
    crate::task_date(&draft.date).map_err(anyhow::Error::msg)?;
    let currency = crate::currency(&draft.currency).map_err(anyhow::Error::msg)?;
    let customer: String = c
        .query_row(
            "SELECT name FROM customers WHERE id=? AND active=1",
            [draft.customer],
            |r| r.get(0),
        )
        .context("请选择有效客户")?;
    ensure!(!draft.lines.is_empty(), "请至少添加一行业务");
    let mut items = Vec::new();
    let mut net = 0i64;
    for (index, line) in draft.lines.iter().enumerate() {
        ensure!(
            line.price_id >= 0 && line.quantity > 0,
            "业务选择或数量无效"
        );
        let price = direct_unit_price(c, line, &currency, &draft.date)?;
        let subtotal = price.checked_mul(line.quantity).context("业务金额过大")?;
        net = net.checked_add(subtotal).context("发票合计过大")?;
        let source: Option<(String, String, String, i64, i64, String)> = if line.price_id == 0 {
            None
        } else {
            Some(c.query_row("SELECT p.name,p.unit,p.currency,p.price,p.vendor,v.name FROM prices p JOIN vendors v ON v.id=p.vendor WHERE p.id=?",[line.price_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?)
        };
        let description = if line.description.trim().is_empty() {
            source.as_ref().map(|s| s.0.as_str()).unwrap_or("")
        } else {
            line.description.trim()
        };
        let unit = if line.unit.trim().is_empty() {
            source.as_ref().map(|s| s.1.as_str()).unwrap_or("")
        } else {
            line.unit.trim()
        };
        crate::required(description)?;
        crate::required(unit)?;
        let source_currency = source.as_ref().map(|s| s.2.as_str()).unwrap_or(&currency);
        let fx = if line.unit_price.is_some() && source_currency != currency {
            None
        } else {
            Some(crate::fx::dated(
                c,
                source_currency,
                &currency,
                &draft.date,
            )?)
        };
        items.push(json!({"id":index+1,"price_id":(line.price_id>0).then_some(line.price_id),"source_task":null,"source_task_number":"","description":description,"unit":unit,"quantity":line.quantity,"unit_price":price,"unit_cost":0,"subtotal":subtotal,"tax_code":line.tax_code.unwrap_or(direct_tax_code(c,draft.customer,line.price_id)?),"source_currency":source_currency,"source_price":source.as_ref().map(|s|s.3).unwrap_or(price),"fx_rate":fx,"price_overridden":line.unit_price.is_some(),"vendor_id":source.as_ref().map(|s|s.4),"vendor_name":source.as_ref().map(|s|s.5.as_str())}));
    }
    let title = if draft.title.trim().is_empty() {
        "Sales Invoice"
    } else {
        draft.title.trim()
    };
    let data = json!({"origin":"direct","task":null,"tasks":[],"task_date":null,"number":null,"title":title,"customer":customer,"customer_id":draft.customer,"currency":currency,"items":items,"expenses":[],"total":net,"cost":0,"profit":net});
    finish_preview(c, data, &draft.date)
}
pub fn direct_confirm(c: &mut Connection, draft: &DirectInvoice, expected: &Value) -> Result<i64> {
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let request = serde_json::to_value(draft)?;
    let prior: Option<(i64, String)> = tx
        .query_row(
            "SELECT invoice,request_json FROM invoice_requests WHERE request_key=?",
            [&draft.request_key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((id, raw)) = prior {
        ensure!(
            serde_json::from_str::<Value>(&raw)? == request,
            "该请求已用于其他发票内容，请重新新建发票"
        );
        return Ok(id);
    }
    let mut data = direct_preview(&tx, draft)?;
    ensure!(
        &data == expected,
        "客户、业务、税率、公司信息或汇率已变化，请重新预览"
    );
    let id = insert_invoice(&tx, None, &mut data)?;
    tx.execute(
        "INSERT INTO invoice_requests(request_key,invoice,request_json) VALUES(?,?,?)",
        params![draft.request_key, id, request.to_string()],
    )?;
    tx.commit()?;
    Ok(id)
}
pub fn init(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS invoice_tasks(invoice INTEGER NOT NULL REFERENCES invoices(id),task INTEGER NOT NULL UNIQUE REFERENCES tasks(id),PRIMARY KEY(invoice,task));
        INSERT OR IGNORE INTO invoice_tasks SELECT id,task FROM invoices WHERE task IS NOT NULL;
        DROP TRIGGER IF EXISTS invoice_anchor_link;
        CREATE TRIGGER invoice_anchor_link AFTER INSERT ON invoices WHEN NEW.task IS NOT NULL BEGIN INSERT INTO invoice_tasks(invoice,task) VALUES(NEW.id,NEW.task); END;
        CREATE TABLE IF NOT EXISTS invoice_requests(request_key TEXT PRIMARY KEY,invoice INTEGER NOT NULL UNIQUE REFERENCES invoices(id),request_json TEXT NOT NULL CHECK(json_valid(request_json)));
        CREATE TABLE IF NOT EXISTS payments(id INTEGER PRIMARY KEY,invoice INTEGER NOT NULL REFERENCES invoices(id),date TEXT,amount INTEGER NOT NULL CHECK(amount>0),method TEXT NOT NULL,reference TEXT NOT NULL DEFAULT '',voided INTEGER NOT NULL DEFAULT 0 CHECK(voided IN (0,1)),void_reason TEXT NOT NULL DEFAULT '',created TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        CREATE INDEX IF NOT EXISTS payments_invoice_idx ON payments(invoice);")?;
    let has_void_date: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('payments') WHERE name='voided_on')",
        [],
        |r| r.get(0),
    )?;
    if !has_void_date {
        c.execute_batch("DROP TRIGGER IF EXISTS payment_void_guard;DROP TRIGGER IF EXISTS payment_amount_guard;ALTER TABLE payments ADD COLUMN voided_on TEXT;")?;
        let has_gl: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='gl_reversals')",
            [],
            |r| r.get(0),
        )?;
        if has_gl {
            c.execute_batch("UPDATE payments SET voided_on=(SELECT e.date FROM gl_sources s JOIN gl_reversals r ON r.entry=s.entry JOIN gl_entries e ON e.id=r.reversal WHERE s.kind='payment' AND s.record=payments.id) WHERE voided=1;")?;
        }
    }
    crate::numbering::init(c)?;
    let legacy = {
        let mut stmt=c.prepare("SELECT id,snapshot FROM invoices WHERE paid=1 AND NOT EXISTS(SELECT 1 FROM payments WHERE invoice=invoices.id)")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for (invoice, raw) in legacy {
        let data: Value = serde_json::from_str(&raw)?;
        let amount = data["total"]
            .as_i64()
            .context("旧发票已支付标记缺少有效金额")?;
        if amount > 0 {
            c.execute("INSERT INTO payments(invoice,amount,method,reference) VALUES(?,?,'Legacy','旧版已支付标记迁移，付款日期未知')",params![invoice,amount])?;
            crate::numbering::assign(c, "payment", c.last_insert_rowid())?;
        }
    }
    Ok(())
}
pub fn for_task(c: &Connection, task: i64) -> Result<Option<i64>> {
    Ok(c.query_row(
        "SELECT invoice FROM invoice_tasks WHERE task=?",
        [task],
        |r| r.get(0),
    )
    .optional()?)
}
pub fn preview(c: &Connection, tasks: &[i64]) -> Result<Value> {
    ensure!(!tasks.is_empty(), "请先勾选 completed Task");
    let unique: std::collections::BTreeSet<_> = tasks.iter().copied().collect();
    ensure!(unique.len() == tasks.len(), "Task 不可重复");
    let mut parts = Vec::new();
    for task in tasks {
        crate::editable(c, *task)?;
        ensure!(
            c.query_row("SELECT progress=100 FROM tasks WHERE id=?", [task], |r| {
                r.get::<_, bool>(0)
            })?,
            "Task #{task} 尚未 completed，不能开票"
        );
        let data = crate::snapshot(c, *task)?;
        ensure!(data["currency"] != "UNSET", "请先指定 Task 币种");
        crate::invoice::validate(&data)?;
        if let Some(first) = parts.first() {
            let first: &Value = first;
            ensure!(
                data["customer_id"] == first["customer_id"],
                "合并开票必须选择同一 Customer 的 Task"
            );
            ensure!(
                data["currency"] == first["currency"],
                "合并开票必须使用相同结算币种，请分别开票"
            );
        }
        parts.push(data);
    }
    let mut data = parts[0].clone();
    let mut items = Vec::new();
    let mut sources = Vec::new();
    let mut expenses = Vec::new();
    let mut total = 0i64;
    let mut cost = 0i64;
    for part in &parts {
        sources.push(json!({"id":part["task"],"number":part["number"],"title":part["title"],"date":part["task_date"]}));
        total = total
            .checked_add(part["total"].as_i64().unwrap())
            .context("发票总额过大")?;
        cost = cost
            .checked_add(part["cost"].as_i64().unwrap())
            .context("发票成本过大")?;
        for expense in part["expenses"].as_array().unwrap() {
            let mut expense = expense.clone();
            expense["source_task"] = part["task"].clone();
            expenses.push(expense);
        }
        for item in part["items"].as_array().unwrap() {
            let mut item = item.clone();
            item["source_task"] = part["task"].clone();
            item["source_task_number"] = part["number"].clone();
            items.push(item);
        }
    }
    data["items"] = json!(items);
    data["tasks"] = json!(sources);
    data["expenses"] = json!(expenses);
    data["total"] = json!(total);
    data["cost"] = json!(cost);
    data["profit"] = json!(total - cost);
    data["number"] = Value::Null;
    data["title"] = json!(parts
        .iter()
        .map(|p| p["number"].as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" / "));
    finish_preview(c, data, &crate::today().to_string())
}
fn finish_preview(c: &Connection, mut data: Value, date: &str) -> Result<Value> {
    let template = crate::invoice_template::Template::load(c)?;
    data["payment_terms"] = json!(template.payment_terms);
    if let Some(id) = data["customer_id"].as_i64() {
        let customer = crate::customer::load(c, id)?;
        if !customer.profile.payment_terms.trim().is_empty() {
            data["payment_terms"] = json!(customer.profile.payment_terms);
        }
        data["customer_address"] = json!(customer.profile.billing_address);
        data["customer_company"] = json!(customer.profile.company);
        data["customer_email"] = json!(customer.profile.email);
        data["customer_phone"] = json!(customer.profile.phone);
    }
    data["payment_instructions"] = json!(template.payment_instructions);
    let total = data["total"].as_i64().context("未税合计无效")?;
    data["number"] = Value::Null;
    data["company"] = crate::Settings::load(c)?.company_snapshot();
    let mut tax_total = 0i64;
    for item in data["items"].as_array_mut().unwrap() {
        let calc = crate::tax::calculate(
            c,
            item["tax_code"].as_i64().unwrap_or(0),
            date,
            item["subtotal"].as_i64().unwrap(),
            false,
            false,
            0,
        )?;
        tax_total = tax_total
            .checked_add(calc["tax"].as_i64().unwrap())
            .context("税额合计过大")?;
        item["tax_snapshot"] = calc;
    }
    data["net_total"] = json!(total);
    data["tax_total"] = json!(tax_total);
    data["total"] = json!(total.checked_add(tax_total).context("含税总额过大")?);
    data["invoice_date"] = json!(date);
    let gl = crate::gl::invoice_plan(c, &data)?;
    data["gl_reporting_fx"] = json!({"rate":gl.rate,"currency":crate::gl::base(c)?});
    crate::invoice::validate(&data)?;
    if data["items"].as_array().unwrap().iter().any(|item| {
        item["tax_snapshot"]["parts"]
            .as_array()
            .is_some_and(|parts| !parts.is_empty())
    }) {
        let (rate, date) = crate::tax::cad_rate(c, data["currency"].as_str().unwrap(), date)?;
        data["tax_reporting_fx"] = json!({"rate":rate,"date":date});
    }
    Ok(data)
}
pub fn merge(c: &mut Connection, tasks: &[i64]) -> Result<i64> {
    merge_inner(c, tasks, None)
}
pub fn merge_confirmed(c: &mut Connection, tasks: &[i64], expected: &Value) -> Result<i64> {
    merge_inner(c, tasks, Some(expected))
}
fn merge_inner(c: &mut Connection, tasks: &[i64], expected: Option<&Value>) -> Result<i64> {
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut data = preview(&tx, tasks)?;
    if let Some(expected) = expected {
        ensure!(
            &data == expected,
            "任务、税务设置、汇率或开票日期已变化，请重新预览后再确认"
        );
    }
    let id = insert_invoice(&tx, Some(tasks[0]), &mut data)?;
    for task in &tasks[1..] {
        tx.execute(
            "INSERT INTO invoice_tasks(invoice,task) VALUES(?,?)",
            params![id, task],
        )?;
    }
    tx.commit()?;
    Ok(id)
}
fn insert_invoice(c: &Connection, task: Option<i64>, data: &mut Value) -> Result<i64> {
    let number = crate::numbering::allocate(c, "invoice")?;
    data["number"] = json!(number);
    let date = data["invoice_date"].as_str().unwrap().to_owned();
    let total = data["total"].as_i64().unwrap();
    c.execute(
        "INSERT INTO invoices(task,snapshot,number,paid) VALUES(?,?,?,?)",
        params![task, data.to_string(), number, total == 0],
    )?;
    let id = c.last_insert_rowid();
    for (index, item) in data["items"].as_array().unwrap().iter().enumerate() {
        crate::tax::post(
            c,
            "invoice",
            id,
            (index + 1) as i64,
            &date,
            data["currency"].as_str().unwrap(),
            &item["tax_snapshot"],
        )?;
    }
    crate::gl::invoice(c, id, data)?;
    Ok(id)
}
#[derive(Clone)]
pub struct Balance {
    pub total: i64,
    pub received: i64,
    pub remaining: i64,
    pub currency: String,
}
impl Balance {
    pub fn status(&self) -> &'static str {
        if self.remaining == 0 {
            "paid"
        } else if self.received > 0 {
            "partially_paid"
        } else {
            "unpaid"
        }
    }
}
pub fn balance(c: &Connection, invoice: i64) -> Result<Balance> {
    let (total, currency): (i64, String) = c
        .query_row("SELECT snapshot FROM invoices WHERE id=?", [invoice], |r| {
            r.get::<_, String>(0)
        })
        .context("发票不存在")
        .and_then(|raw| {
            let data: Value = serde_json::from_str(&raw)?;
            crate::invoice::validate(&data)?;
            Ok(c.query_row(
                "SELECT total,currency FROM invoice_headers WHERE invoice=?",
                [invoice],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })?;
    let mut received = 0i64;
    let mut stmt = c.prepare("SELECT amount FROM payments WHERE invoice=? AND voided=0")?;
    for row in stmt.query_map([invoice], |r| r.get::<_, i64>(0))? {
        received = received.checked_add(row?).context("付款合计过大")?;
    }
    ensure!(received <= total, "付款合计超过发票金额");
    Ok(Balance {
        total,
        received,
        remaining: total - received,
        currency,
    })
}
pub struct InvoiceSummary {
    pub id: i64,
    pub number: String,
    pub customer: String,
    pub date: String,
    pub balance: Balance,
}
/// Overview query only. Opening, posting and restoring retain full snapshot validation.
pub fn list_invoices(
    c: &Connection,
    query: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<InvoiceSummary>> {
    use rusqlite::functions::FunctionFlags;
    c.create_scalar_function(
        "axibiz_lower",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |context| Ok(context.get::<String>(0)?.to_lowercase()),
    )?;
    ensure!(
        (1..=101).contains(&limit) && offset >= 0,
        "Invalid invoice page"
    );
    let mut stmt=c.prepare("SELECT b.invoice,COALESCE(b.number,''),b.customer_name,COALESCE(json_extract(i.snapshot,'$.invoice_date'),substr(i.created,1,10)),b.total,b.currency,b.received,b.remaining FROM invoice_balances b JOIN invoices i ON i.id=b.invoice WHERE instr(axibiz_lower(COALESCE(b.number,'')),axibiz_lower(?1))>0 OR instr(axibiz_lower(b.customer_name),axibiz_lower(?1))>0 OR instr(axibiz_lower(b.title),axibiz_lower(?1))>0 ORDER BY b.invoice DESC LIMIT ?2 OFFSET ?3")?;
    let rows = stmt
        .query_map(params![query.trim(), limit, offset], |r| {
            Ok(InvoiceSummary {
                id: r.get(0)?,
                number: r.get(1)?,
                customer: r.get(2)?,
                date: r.get(3)?,
                balance: Balance {
                    total: r.get(4)?,
                    currency: r.get(5)?,
                    received: r.get(6)?,
                    remaining: r.get(7)?,
                },
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
pub fn add_payment(
    c: &mut Connection,
    invoice: i64,
    date: &str,
    amount: &str,
    method: &str,
    reference: &str,
) -> Result<i64> {
    let request = crate::posting::request_id(c)?;
    let draft = crate::receipts::prepare(c, &request, invoice, date, amount, method, reference)?;
    crate::receipts::record(c, &draft)
}

pub fn void_payment(c: &mut Connection, id: i64, reason: &str) -> Result<()> {
    void_payment_on(c, id, reason, &crate::today().to_string())
}
pub fn void_payment_on(c: &mut Connection, id: i64, reason: &str, date: &str) -> Result<()> {
    crate::required(reason)?;
    crate::task_date(date).map_err(anyhow::Error::msg)?;
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let invoice: i64 = tx
        .query_row(
            "SELECT invoice FROM payments WHERE id=? AND voided=0",
            [id],
            |r| r.get(0),
        )
        .context("付款记录不存在或已撤销")?;
    let original: Option<String> =
        tx.query_row("SELECT date FROM payments WHERE id=?", [id], |r| r.get(0))?;
    let source:Option<String>=tx.query_row("SELECT e.date FROM gl_sources s JOIN gl_entries e ON e.id=s.entry WHERE s.kind='payment' AND s.record=?",[id],|r|r.get(0)).optional()?;
    let earliest = original
        .as_deref()
        .unwrap_or("")
        .max(source.as_deref().unwrap_or(""));
    crate::posting::settlement_date(&tx, date, earliest, &crate::receipts::latest(&tx, invoice)?)?;
    crate::gl::void_source(&tx, "payment", id, date, reason)?;
    tx.execute(
        "UPDATE payments SET voided=1,void_reason=?,voided_on=? WHERE id=?",
        params![reason, date, id],
    )?;
    let due = balance(&tx, invoice)?;
    tx.execute(
        "UPDATE invoices SET paid=? WHERE id=?",
        params![due.remaining == 0, invoice],
    )?;
    tx.commit()?;
    Ok(())
}

pub struct HistoricalBalance {
    pub balance: Balance,
    pub complete: bool,
}
pub fn validate_payment_dates(c: &Connection) -> Result<()> {
    let mut stmt = c.prepare("SELECT date,voided_on FROM payments")?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, Option<String>>(0)?,
            r.get::<_, Option<String>>(1)?,
        ))
    })? {
        let (date, voided_on) = row?;
        for date in [date, voided_on].into_iter().flatten() {
            crate::task_date(&date).map_err(anyhow::Error::msg)?;
        }
    }
    let invalid:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM payments WHERE (voided=0 AND voided_on IS NOT NULL) OR voided_on<date) OR EXISTS(SELECT 1 FROM payments p JOIN gl_sources s ON s.kind='payment' AND s.record=p.id LEFT JOIN gl_reversals r ON r.entry=s.entry LEFT JOIN gl_entries e ON e.id=r.reversal WHERE (r.reversal IS NOT NULL AND (p.voided!=1 OR (p.voided_on IS NOT NULL AND p.voided_on!=e.date))) OR (p.voided=1 AND s.entry IS NOT NULL AND r.reversal IS NULL))",[],|r|r.get(0))?;
    ensure!(!invalid, "收款撤销日期或 GL 冲销关联不一致");
    Ok(())
}
pub fn balance_as_of(
    c: &Connection,
    invoice: i64,
    date: &str,
) -> Result<Option<HistoricalBalance>> {
    crate::task_date(date).map_err(anyhow::Error::msg)?;
    let (raw, created): (String, String) = c.query_row(
        "SELECT snapshot,created FROM invoices WHERE id=?",
        [invoice],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let data: Value = serde_json::from_str(&raw)?;
    crate::invoice::validate(&data)?;
    let issue_date = data["invoice_date"]
        .as_str()
        .unwrap_or_else(|| created.split(' ').next().unwrap_or(&created));
    if issue_date > date {
        return Ok(None);
    }
    let (received,unknown):(i64,bool)=c.query_row("SELECT COALESCE(sum(CASE WHEN date<=?2 AND (voided=0 OR voided_on>?2) THEN amount ELSE 0 END),0),EXISTS(SELECT 1 FROM payments WHERE invoice=?1 AND (date IS NULL OR (date<=?2 AND voided=1 AND voided_on IS NULL))) FROM payments WHERE invoice=?1",params![invoice,date],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let total = data["total"].as_i64().context("发票总额无效")?;
    Ok(Some(HistoricalBalance {
        balance: Balance {
            total,
            currency: data["currency"].as_str().unwrap_or("UNSET").into(),
            received,
            remaining: total.checked_sub(received).context("余额过大")?,
        },
        complete: !unknown,
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cutoff_receipts_remain_in_their_original_period_after_voiding() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Buyer','');")?;
        let draft = DirectInvoice {
            request_key: "cutoff-test".into(),
            customer: 1,
            date: "2026-10-01".into(),
            currency: "CAD".into(),
            title: "Service".into(),
            lines: vec![DirectLine {
                price_id: 0,
                description: "Service".into(),
                unit: "hour".into(),
                quantity: 1,
                unit_price: Some(5000),
                tax_code: Some(0),
            }],
        };
        let preview = direct_preview(&c, &draft)?;
        let invoice = direct_confirm(&mut c, &draft, &preview)?;
        let payment = add_payment(&mut c, invoice, "2026-10-05", "20", "Cash", "")?;
        assert!(balance_as_of(&c, invoice, "2026-09-30")?.is_none());
        assert!(void_payment_on(&mut c, payment, "Mistake", "2026-10-04").is_err());
        c.execute("UPDATE gl_settings SET locked_through='2026-10-15'", [])?;
        assert!(void_payment_on(&mut c, payment, "Mistake", "2026-10-15").is_err());
        assert_eq!(balance(&c, invoice)?.remaining, 3000);
        c.execute("UPDATE gl_settings SET locked_through=''", [])?;
        void_payment_on(&mut c, payment, "Correction", "2026-10-15")?;
        let historical = balance_as_of(&c, invoice, "2026-10-10")?.unwrap();
        assert!(historical.complete);
        assert_eq!(historical.balance.remaining, 3000);
        assert_eq!(
            balance_as_of(&c, invoice, "2026-10-15")?
                .unwrap()
                .balance
                .remaining,
            5000
        );
        assert!(void_payment_on(&mut c, payment, "Again", "2026-10-16").is_err());
        crate::storage::validate_projections(&c)?;
        Ok(())
    }
    #[test]
    fn receipt_date_upgrade_uses_reversal_evidence_without_guessing() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Buyer','');")?;
        let draft = DirectInvoice {
            request_key: "migration-date".into(),
            customer: 1,
            date: "2026-10-01".into(),
            currency: "CAD".into(),
            title: "Service".into(),
            lines: vec![DirectLine {
                price_id: 0,
                description: "Service".into(),
                unit: "item".into(),
                quantity: 1,
                unit_price: Some(5000),
                tax_code: Some(0),
            }],
        };
        let preview = direct_preview(&c, &draft)?;
        let invoice = direct_confirm(&mut c, &draft, &preview)?;
        let payment = add_payment(&mut c, invoice, "2026-10-05", "20", "Cash", "")?;
        void_payment_on(&mut c, payment, "Correction", "2026-10-15")?;
        c.execute_batch("DROP TRIGGER payment_void_guard;DROP TRIGGER payment_amount_guard;DROP TRIGGER audit_payments_INSERT;DROP TRIGGER audit_payments_UPDATE;DROP TRIGGER audit_payments_DELETE;ALTER TABLE payments DROP COLUMN voided_on;PRAGMA user_version=14;")?;
        crate::init(&c)?;
        assert_eq!(
            c.query_row(
                "SELECT voided_on FROM payments WHERE id=?",
                [payment],
                |r| r.get::<_, String>(0)
            )?,
            "2026-10-15"
        );
        assert_eq!(
            balance_as_of(&c, invoice, "2026-10-10")?
                .unwrap()
                .balance
                .remaining,
            3000
        );
        c.execute(
            "INSERT INTO payments(invoice,amount,method) VALUES(?,100,'Legacy')",
            [invoice],
        )?;
        assert!(!balance_as_of(&c, invoice, "2026-10-10")?.unwrap().complete);
        crate::storage::validate_projections(&c)?;
        Ok(())
    }
    #[test]
    fn invoice_overview_pages_and_balances_match_validated_subledger() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer Élodie','');INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','hour',0,5000,'CAD');")?;
        let mut invoices = Vec::new();
        for title in ["One", "Two", "Three"] {
            let task = crate::add_task(&c, title, 1, "CAD")?;
            crate::add_item(&c, task, 1, 1, None, None)?;
            crate::set_task_status(&mut c, task, false)?;
            invoices.push(merge(&mut c, &[task])?);
        }
        let payment = add_payment(
            &mut c,
            invoices[0],
            &crate::today().to_string(),
            "12.50",
            "Cash",
            "",
        )?;
        for row in list_invoices(&c, "Customer", 101, 0)? {
            let full = balance(&c, row.id)?;
            assert_eq!(
                (
                    row.balance.total,
                    row.balance.received,
                    row.balance.remaining
                ),
                (full.total, full.received, full.remaining)
            );
        }
        assert_eq!(list_invoices(&c, "", 2, 0)?.len(), 2);
        assert_eq!(list_invoices(&c, "", 2, 2)?[0].id, invoices[0]);
        assert!(list_invoices(&c, "%", 101, 0)?.is_empty());
        assert_eq!(list_invoices(&c, "customer", 101, 0)?.len(), 3);
        assert_eq!(list_invoices(&c, "ÉLODIE", 101, 0)?.len(), 3);
        void_payment(&mut c, payment, "Correction")?;
        assert_eq!(
            list_invoices(&c, "INV0001", 101, 0)?[0].balance.remaining,
            5000
        );
        Ok(())
    }
    fn direct_draft(c: &Connection) -> Result<DirectInvoice> {
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer','');INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency,tax_code) VALUES(1,1,'Service','item',0,10000,'CAD',3);UPDATE tax_agencies SET registered=1,registration='TEST';")?;
        Ok(DirectInvoice {
            request_key: "direct-test".into(),
            customer: 1,
            date: crate::today().to_string(),
            currency: "CAD".into(),
            title: "Direct sale".into(),
            lines: vec![DirectLine {
                price_id: 1,
                description: "Service".into(),
                unit: "item".into(),
                quantity: 1,
                unit_price: None,
                tax_code: None,
            }],
        })
    }
    #[test]
    fn direct_invoice_posts_without_tasks_and_retries_do_not_duplicate() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        let mut draft = direct_draft(&c)?;
        let task = crate::add_task(&c, "Unrelated task", 1, "CAD")?;
        let writes = c.total_changes();
        let preview = direct_preview(&c, &draft)?;
        assert_eq!(c.total_changes(), writes);
        assert_eq!(preview["total"], 11200);
        let id = direct_confirm(&mut c, &draft, &preview)?;
        assert!(c
            .query_row("SELECT task FROM invoices WHERE id=?", [id], |r| r
                .get::<_, Option<i64>>(0))?
            .is_none());
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM invoice_tasks", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM invoice_lines WHERE source_task IS NOT NULL",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        assert_eq!(crate::snapshot(&c, task)?["status"], "pending");
        assert_eq!(direct_confirm(&mut c, &draft, &preview)?, id);
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM invoices", [], |r| r.get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            c.query_row(
                "SELECT next_value FROM number_sequences WHERE kind='invoice'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            2
        );
        let payment = add_payment(&mut c, id, &draft.date, "56", "Bank", "")?;
        assert_eq!(balance(&c, id)?.remaining, 5600);
        let path = std::env::temp_dir().join(format!(
            "axibiz-direct-{}-{}.db",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let safety = path.with_extension("safety.db");
        crate::backup::save(&c, &path)?;
        crate::backup::verify(&path)?;
        let mut restored = Connection::open_in_memory()?;
        crate::init(&restored)?;
        crate::backup::restore(&mut restored, &path, &safety)?;
        assert_eq!(balance(&restored, id)?.remaining, 5600);
        assert_eq!(direct_confirm(&mut restored, &draft, &preview)?, id);
        crate::storage::validate_projections(&restored)?;
        drop(restored);
        std::fs::remove_file(path)?;
        std::fs::remove_file(safety)?;
        void_payment(&mut c, payment, "Correction")?;
        assert_eq!(balance(&c, id)?.remaining, 11200);
        draft.lines[0].quantity = 2;
        assert!(direct_confirm(&mut c, &draft, &preview).is_err());
        crate::storage::validate_projections(&c)?;
        crate::init(&c)?;
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM invoice_tasks", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        Ok(())
    }
    #[test]
    fn direct_preview_revalidates_prices_and_supports_custom_foreign_override() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        let mut draft = direct_draft(&c)?;
        let preview = direct_preview(&c, &draft)?;
        c.execute("UPDATE prices SET price=12000", [])?;
        assert!(direct_confirm(&mut c, &draft, &preview).is_err());
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM invoices", [], |r| r.get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM gl_entries", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        c.execute("UPDATE prices SET currency='USD',tax_code=0", [])?;
        assert!(direct_preview(&c, &draft).is_err());
        draft.lines[0].unit_price = Some(6000);
        let fresh = direct_preview(&c, &draft)?;
        assert_eq!(fresh["total"], 6000);
        assert!(fresh["items"][0]["fx_rate"].is_null());
        direct_confirm(&mut c, &draft, &fresh)?;
        draft.request_key = "custom-invoice".into();
        draft.lines[0].price_id = 0;
        draft.lines[0].description = "Custom consulting".into();
        draft.lines[0].quantity = 2;
        draft.lines[0].tax_code = Some(3);
        let preview = direct_preview(&c, &draft)?;
        assert_eq!(preview["total"], 13440);
        let id = direct_confirm(&mut c, &draft, &preview)?;
        assert_eq!(balance(&c, id)?.total, 13440);
        crate::storage::validate_projections(&c)?;
        Ok(())
    }
    #[test]
    fn tax_preview_matches_saved_invoice_and_rejects_changed_registration() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute_batch("UPDATE tax_agencies SET registered=1,registration='TEST-ONLY'; INSERT INTO customers(id,name,contact) VALUES(1,'Customer',''); INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor',''); INSERT INTO prices(id,vendor,name,unit,cost,price,currency,tax_code) VALUES(1,1,'Service','item',0,10000,'CAD',3);")?;
        for _ in 0..2 {
            let id = crate::add_task(&c, "Task", 1, "CAD")?;
            crate::add_item(&c, id, 1, 1, None, None)?;
            crate::set_task_status(&mut c, id, false)?;
        }
        let writes = c.total_changes();
        let data = preview(&c, &[1, 2])?;
        assert_eq!(c.total_changes(), writes);
        assert_eq!(data["net_total"], 20000);
        assert_eq!(data["tax_total"], 2400);
        assert_eq!(data["total"], 22400);
        assert!(data["tax_reporting_fx"]["rate"].is_i64());
        c.execute(
            "UPDATE tax_agencies SET registration='CHANGED' WHERE id=1",
            [],
        )?;
        assert!(merge_confirmed(&mut c, &[1, 2], &data).is_err());
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM tax_postings", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        let mut fresh = preview(&c, &[1, 2])?;
        let id = merge_confirmed(&mut c, &[1, 2], &fresh)?;
        let raw: String = c.query_row("SELECT snapshot FROM invoices WHERE id=?", [id], |r| {
            r.get(0)
        })?;
        let saved: Value = serde_json::from_str(&raw)?;
        fresh["number"] = saved["number"].clone();
        assert_eq!(saved, fresh);
        assert_eq!(balance(&c, id)?.total, 22400);
        Ok(())
    }
    #[test]
    fn merged_invoice_locks_every_task_and_tracks_partial_receipts() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer',''),(2,'Other','');INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','item',0,5000,'CAD');")?;
        for customer in [1, 1, 2] {
            let id = crate::add_task(&c, "Task", customer, "CAD")?;
            crate::add_item(&c, id, 1, 1, None, None)?;
            crate::set_task_status(&mut c, id, false)?;
        }
        assert!(merge(&mut c, &[1, 3]).is_err());
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM invoices", [], |r| r.get::<_, i64>(0))?,
            0
        );
        let id = merge(&mut c, &[1, 2])?;
        assert_eq!(for_task(&c, 2)?, Some(id));
        assert!(crate::editable(&c, 2).is_err());
        assert!(crate::set_task_status(&mut c, 2, true).is_err());
        assert_eq!(balance(&c, id)?.remaining, 10000);
        let payment = add_payment(
            &mut c,
            id,
            &crate::today().to_string(),
            "30",
            "Bank transfer",
            "REF-1",
        )?;
        assert_eq!(balance(&c, id)?.status(), "partially_paid");
        assert!(add_payment(&mut c, id, &crate::today().to_string(), "80", "Cash", "").is_err());
        add_payment(&mut c, id, &crate::today().to_string(), "70", "Cash", "")?;
        assert_eq!(balance(&c, id)?.status(), "paid");
        void_payment(&mut c, payment, "Correction")?;
        assert_eq!(balance(&c, id)?.remaining, 3000);
        crate::init(&c)?;
        assert_eq!(balance(&c, id)?.remaining, 3000);
        Ok(())
    }
    #[test]
    fn legacy_invoice_links_and_paid_flags_migrate_once() -> Result<()> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer','');INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','item',0,5000,'CAD');")?;
        let task = crate::add_task(&c, "Task", 1, "CAD")?;
        crate::add_item(&c, task, 1, 1, None, None)?;
        let data = crate::snapshot(&c, task)?.to_string();
        c.execute(
            "INSERT INTO invoices(task,snapshot,paid) VALUES(?,?,1)",
            params![task, data],
        )?;
        c.execute("DROP TRIGGER invoice_anchor_link", [])?;
        c.execute("DROP TABLE invoice_tasks", [])?;
        crate::init(&c)?;
        crate::init(&c)?;
        let id = for_task(&c, task)?.unwrap();
        assert_eq!(balance(&c, id)?.received, 5000);
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM payments", [], |r| r.get::<_, i64>(0))?,
            1
        );
        let date: Option<String> = c.query_row("SELECT date FROM payments", [], |r| r.get(0))?;
        assert!(date.is_none());
        let preserved: String = c.query_row("SELECT snapshot FROM invoices", [], |r| r.get(0))?;
        assert_eq!(preserved, data);
        Ok(())
    }
    #[test]
    fn mismatched_currency_and_duplicate_tasks_do_not_consume_numbers() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer','');INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'CAD service','item',0,5000,'CAD'),(2,1,'USD service','item',0,5000,'USD');")?;
        for (currency, price) in [("CAD", 1), ("USD", 2)] {
            let task = crate::add_task(&c, "Task", 1, currency)?;
            crate::add_item(&c, task, price, 1, None, None)?;
            crate::set_task_status(&mut c, task, false)?;
        }
        assert!(merge(&mut c, &[1, 2]).is_err());
        assert!(merge(&mut c, &[1, 1]).is_err());
        assert_eq!(
            c.query_row(
                "SELECT next_value FROM number_sequences WHERE kind='invoice'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM invoices", [], |r| r.get::<_, i64>(0))?,
            0
        );
        Ok(())
    }
}
