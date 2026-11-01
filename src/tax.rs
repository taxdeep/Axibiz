use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
pub mod admin;
pub const SCALE: i64 = 1_000_000;
#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub code: i64,
    pub inclusive: bool,
    pub recovery: u8,
}
impl Default for Input {
    fn default() -> Self {
        Self {
            code: 0,
            inclusive: false,
            recovery: 100,
        }
    }
}
pub fn percent(s: &str) -> Result<i64> {
    let parts: Vec<_> = s.trim().split('.').collect();
    ensure!(
        parts.len() <= 2
            && !parts[0].is_empty()
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit())),
        "税率须为非负十进制数"
    );
    let whole: i64 = parts[0].parse()?;
    ensure!(whole <= 100, "税率不能超过 100%");
    let fraction = if parts.len() == 2 {
        ensure!(parts[1].len() <= 4, "税率最多四位小数");
        parts[1].parse::<i64>()? * 10i64.pow(4 - parts[1].len() as u32)
    } else {
        0
    };
    let value = whole * 10000 + fraction;
    ensure!(value <= SCALE, "税率不能超过 100%");
    Ok(value)
}
pub fn format_rate(rate: i64) -> String {
    format!("{}.{:04}", rate / 10000, rate % 10000)
}
fn account(c: &Connection, code: &str, name: &str, kind: &str) -> Result<i64> {
    let mut candidate = code.to_owned();
    let mut suffix = 0;
    loop {
        if let Some((id, existing, active, existing_name)) = c
            .query_row(
                "SELECT id,kind,active,name FROM accounts WHERE code=?",
                [&candidate],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, bool>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?
        {
            if existing == kind && active && existing_name == name {
                return Ok(id);
            }
            suffix += 1;
            candidate = format!("{code}-TAX-{suffix}");
        } else {
            c.execute(
                "INSERT INTO accounts(code,name,kind) VALUES(?,?,?)",
                params![candidate, name, kind],
            )?;
            return Ok(c.last_insert_rowid());
        }
    }
}
pub fn init(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS tax_agencies(id INTEGER PRIMARY KEY,name TEXT NOT NULL UNIQUE,registration TEXT NOT NULL DEFAULT '',registered INTEGER NOT NULL DEFAULT 0 CHECK(registered IN(0,1)),payable_account INTEGER NOT NULL REFERENCES accounts(id),recoverable_account INTEGER REFERENCES accounts(id));
        CREATE TABLE IF NOT EXISTS tax_items(id INTEGER PRIMARY KEY,agency INTEGER NOT NULL REFERENCES tax_agencies(id),name TEXT NOT NULL UNIQUE,recoverable INTEGER NOT NULL CHECK(recoverable IN(0,1)));
        CREATE TABLE IF NOT EXISTS tax_rates(id INTEGER PRIMARY KEY,item INTEGER NOT NULL REFERENCES tax_items(id),effective TEXT NOT NULL,rate INTEGER NOT NULL CHECK(typeof(rate)='integer' AND rate BETWEEN 0 AND 1000000),UNIQUE(item,effective));
        CREATE TABLE IF NOT EXISTS tax_codes(id INTEGER PRIMARY KEY,code TEXT NOT NULL UNIQUE,name TEXT NOT NULL,kind TEXT NOT NULL CHECK(kind IN('taxable','zero','exempt','none')),active INTEGER NOT NULL DEFAULT 1 CHECK(active IN(0,1)));
        CREATE TABLE IF NOT EXISTS tax_code_items(code INTEGER NOT NULL REFERENCES tax_codes(id),item INTEGER NOT NULL REFERENCES tax_items(id),PRIMARY KEY(code,item));
        CREATE TABLE IF NOT EXISTS tax_assignments(entity TEXT NOT NULL CHECK(entity IN('customer','vendor')),record INTEGER NOT NULL,code INTEGER NOT NULL REFERENCES tax_codes(id),PRIMARY KEY(entity,record));
        CREATE TABLE IF NOT EXISTS expense_tax(expense INTEGER PRIMARY KEY REFERENCES ledger_expenses(id),input INTEGER NOT NULL CHECK(input>0),code INTEGER NOT NULL REFERENCES tax_codes(id),inclusive INTEGER NOT NULL CHECK(inclusive IN(0,1)),recovery INTEGER NOT NULL CHECK(recovery BETWEEN 0 AND 100),net INTEGER NOT NULL,gross INTEGER NOT NULL,cost INTEGER NOT NULL,snapshot TEXT NOT NULL CHECK(json_valid(snapshot)));
        CREATE TABLE IF NOT EXISTS tax_postings(id INTEGER PRIMARY KEY,source TEXT NOT NULL CHECK(source IN('invoice','expense')),document INTEGER NOT NULL,line INTEGER NOT NULL,agency INTEGER NOT NULL REFERENCES tax_agencies(id),date TEXT NOT NULL,currency TEXT NOT NULL,base INTEGER NOT NULL,amount INTEGER NOT NULL,recoverable INTEGER NOT NULL,cad_amount INTEGER NOT NULL,cad_recoverable INTEGER NOT NULL,fx_rate INTEGER NOT NULL,fx_date TEXT,snapshot TEXT NOT NULL CHECK(json_valid(snapshot)));
        CREATE INDEX IF NOT EXISTS tax_postings_period_idx ON tax_postings(agency,date,source);
        CREATE INDEX IF NOT EXISTS tax_postings_source_idx ON tax_postings(source,document);
        CREATE TABLE IF NOT EXISTS tax_cash(id INTEGER PRIMARY KEY,agency INTEGER NOT NULL REFERENCES tax_agencies(id),date TEXT NOT NULL,kind TEXT NOT NULL CHECK(kind IN('payment','refund','increase','decrease')),amount INTEGER NOT NULL CHECK(typeof(amount)='integer' AND amount>0),reference TEXT NOT NULL,reason TEXT NOT NULL,voided INTEGER NOT NULL DEFAULT 0 CHECK(voided IN(0,1)));")?;
    let has_void_date: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tax_cash') WHERE name='voided_on')",
        [],
        |r| r.get(0),
    )?;
    if !has_void_date {
        c.execute_batch("DROP TRIGGER IF EXISTS tax_cash_fixed; ALTER TABLE tax_cash ADD COLUMN voided_on TEXT;")?;
        let has_gl: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='gl_reversals')",
            [],
            |r| r.get(0),
        )?;
        if has_gl {
            // Only reconstruct dates supported by the original reversal journal.
            c.execute_batch("UPDATE tax_cash SET voided_on=(SELECT e.date FROM gl_sources s JOIN gl_reversals r ON r.entry=s.entry JOIN gl_entries e ON e.id=r.reversal WHERE s.kind='tax' AND s.record=tax_cash.id) WHERE voided=1;")?;
        }
    }
    let seeded: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM tax_codes WHERE id=0)",
        [],
        |r| r.get(0),
    )?;
    if !seeded {
        let payable = account(c, "2200", "GST/HST Payable", "Liability")?;
        let itc = account(c, "1200", "GST/HST Recoverable", "Asset")?;
        let pst = account(c, "2210", "BC PST Payable", "Liability")?;
        c.execute("INSERT INTO tax_agencies(id,name,payable_account,recoverable_account) VALUES(1,'CRA · GST/HST',?,?)",params![payable,itc])?;
        c.execute("INSERT INTO tax_agencies(id,name,payable_account) VALUES(2,'BC Ministry of Finance · PST',?)",[pst])?;
        c.execute_batch("INSERT INTO tax_items VALUES(1,1,'GST',1),(2,2,'BC PST',0),(3,1,'HST Ontario',1),(4,1,'HST Nova Scotia',1),(5,1,'HST NB/NL/PE',1);
            INSERT INTO tax_rates(item,effective,rate) VALUES(1,'2008-01-01',50000),(2,'2013-04-01',70000),(3,'2010-07-01',130000),(4,'2010-07-01',150000),(4,'2025-04-01',140000),(5,'2016-10-01',150000);
            INSERT INTO tax_codes VALUES(0,'NONE','未指定 / 不计税（旧数据）','none',1),(1,'G','GST Only','taxable',1),(2,'P','BC PST Only','taxable',1),(3,'S','GST + BC PST','taxable',1),(4,'H-ON','HST Ontario','taxable',1),(5,'H-NS','HST Nova Scotia','taxable',1),(6,'H-15','HST NB/NL/PE','taxable',1),(7,'Z','Zero-rated','zero',1),(8,'E','Exempt','exempt',1);
            INSERT INTO tax_code_items VALUES(1,1),(2,2),(3,1),(3,2),(4,3),(5,4),(6,5);")?;
    }
    for table in ["prices", "items"] {
        let has: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?) WHERE name='tax_code')",
            [table],
            |r| r.get(0),
        )?;
        if !has {
            c.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN tax_code INTEGER NOT NULL DEFAULT 0;"
            ))?;
        }
    }
    for table in ["prices", "items"] {
        for operation in ["INSERT", "UPDATE OF tax_code"] {
            let suffix = if operation == "INSERT" {
                "insert"
            } else {
                "update"
            };
            c.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS {table}_tax_reference_{suffix} BEFORE {operation} ON {table} WHEN typeof(NEW.tax_code)!='integer' OR NOT EXISTS(SELECT 1 FROM tax_codes WHERE id=NEW.tax_code) BEGIN SELECT RAISE(ABORT,'Tax code does not exist'); END;"))?;
        }
    }
    c.execute_batch("CREATE TRIGGER IF NOT EXISTS tax_code_delete_guard BEFORE DELETE ON tax_codes WHEN OLD.id=0 OR EXISTS(SELECT 1 FROM prices WHERE tax_code=OLD.id) OR EXISTS(SELECT 1 FROM items WHERE tax_code=OLD.id) BEGIN SELECT RAISE(ABORT,'Referenced tax codes cannot be deleted'); END;
        CREATE TRIGGER IF NOT EXISTS tax_code_id_fixed BEFORE UPDATE OF id ON tax_codes BEGIN SELECT RAISE(ABORT,'Tax code identity is immutable'); END;
        CREATE TRIGGER IF NOT EXISTS tax_rates_fixed_update BEFORE UPDATE ON tax_rates BEGIN SELECT RAISE(ABORT,'Add a new effective-date rate instead of editing history'); END;
        CREATE TRIGGER IF NOT EXISTS tax_rates_fixed_delete BEFORE DELETE ON tax_rates BEGIN SELECT RAISE(ABORT,'Tax rate history cannot be deleted'); END;
        CREATE TRIGGER IF NOT EXISTS tax_cash_fixed BEFORE UPDATE ON tax_cash WHEN NEW.agency!=OLD.agency OR NEW.date!=OLD.date OR NEW.kind!=OLD.kind OR NEW.amount!=OLD.amount OR NEW.reference!=OLD.reference OR NEW.reason!=OLD.reason OR OLD.voided=1 OR NEW.voided!=1 OR NEW.voided_on IS NULL OR NEW.voided_on<OLD.date BEGIN SELECT RAISE(ABORT,'Only dated voiding of a tax cash record is allowed'); END;
        CREATE TRIGGER IF NOT EXISTS tax_cash_initial BEFORE INSERT ON tax_cash WHEN NEW.voided!=0 OR NEW.voided_on IS NOT NULL BEGIN SELECT RAISE(ABORT,'New tax cash records cannot be voided'); END;
        CREATE TRIGGER IF NOT EXISTS tax_cash_no_delete BEFORE DELETE ON tax_cash BEGIN SELECT RAISE(ABORT,'Void tax cash records instead of deleting'); END;
        CREATE TRIGGER IF NOT EXISTS tax_invoice_no_update BEFORE UPDATE ON tax_postings WHEN OLD.source='invoice' BEGIN SELECT RAISE(ABORT,'Invoice tax snapshot is immutable'); END;
        CREATE TRIGGER IF NOT EXISTS tax_invoice_no_delete BEFORE DELETE ON tax_postings WHEN OLD.source='invoice' BEGIN SELECT RAISE(ABORT,'Invoice tax snapshot is immutable'); END;")?;
    Ok(())
}
pub fn calculate(
    c: &Connection,
    code: i64,
    date: &str,
    input: i64,
    inclusive: bool,
    purchase: bool,
    recovery: u8,
) -> Result<Value> {
    crate::task_date(date).map_err(anyhow::Error::msg)?;
    ensure!(input >= 0 && recovery <= 100, "计税金额或抵扣比例无效");
    let (label, kind, active): (String, String, bool) = c
        .query_row(
            "SELECT code,kind,active FROM tax_codes WHERE id=?",
            [code],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .context("税码不存在")?;
    ensure!(active, "税码已停用");
    let mut parts = Vec::new();
    let mut rates = 0i64;
    if kind == "taxable" {
        let mut stmt=c.prepare("SELECT i.id,i.agency,i.name,i.recoverable,a.registered,a.name,a.registration,(SELECT rate FROM tax_rates WHERE item=i.id AND effective<=? ORDER BY effective DESC LIMIT 1),(SELECT effective FROM tax_rates WHERE item=i.id AND effective<=? ORDER BY effective DESC LIMIT 1) FROM tax_code_items x JOIN tax_items i ON i.id=x.item JOIN tax_agencies a ON a.id=i.agency WHERE x.code=? ORDER BY i.id")?;
        let rows = stmt.query_map(params![date, date, code], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, bool>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, Option<String>>(8)?,
            ))
        })?;
        for row in rows {
            let (
                item,
                agency,
                name,
                allowed,
                registered,
                agency_name,
                registration,
                rate,
                effective,
            ) = row?;
            ensure!(
                purchase || (registered && !registration.trim().is_empty()),
                "请先在 Sales Tax Setup 设置 {agency_name} 的注册状态和税号"
            );
            let rate = rate.context(format!("{name} 在 {date} 没有生效税率"))?;
            rates = rates.checked_add(rate).context("税率合计过大")?;
            let (payable, recoverable_account): (i64, Option<i64>) = c.query_row(
                "SELECT payable_account,recoverable_account FROM tax_agencies WHERE id=?",
                [agency],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            parts.push(json!({"item":item,"agency":agency,"agency_name":agency_name,"payable_account":payable,"recoverable_account":recoverable_account,"registration":registration,"name":name,"rate":rate,"effective":effective,"eligible":allowed && registered && !registration.is_empty() && recoverable_account.is_some(),"amount":0,"recoverable":0}));
        }
        ensure!(!parts.is_empty(), "应税税码缺少税目");
    }
    let net = if inclusive {
        i64::try_from(
            (i128::from(input) * i128::from(SCALE) + i128::from(SCALE + rates) / 2)
                / i128::from(SCALE + rates),
        )?
    } else {
        input
    };
    let mut tax = 0i64;
    for part in &mut parts {
        let amount = i64::try_from(
            (i128::from(net) * i128::from(part["rate"].as_i64().unwrap()) + i128::from(SCALE / 2))
                / i128::from(SCALE),
        )?;
        part["amount"] = json!(amount);
        tax = tax.checked_add(amount).context("税额过大")?;
    }
    if inclusive {
        let mut delta = input - net - tax;
        for part in parts.iter_mut().rev() {
            let amount = part["amount"].as_i64().unwrap();
            let correction = if delta < 0 { delta.max(-amount) } else { delta };
            part["amount"] = json!(amount + correction);
            delta -= correction;
        }
        ensure!(delta == 0, "含税拆分失败");
        tax = input - net;
    }
    let mut recoverable = 0i64;
    for part in &mut parts {
        let value = if purchase && part["eligible"] == true {
            (i128::from(part["amount"].as_i64().unwrap()) * i128::from(recovery) + 50) / 100
        } else {
            0
        };
        let value = i64::try_from(value)?;
        part["recoverable"] = json!(value);
        recoverable = recoverable.checked_add(value).context("抵扣额过大")?;
    }
    let gross = net.checked_add(tax).context("含税金额过大")?;
    Ok(
        json!({"code_id":code,"code":label,"kind":kind,"net":net,"gross":gross,"tax":tax,"recoverable":recoverable,"cost":gross-recoverable,"parts":parts}),
    )
}
pub fn cad_rate(c: &Connection, currency: &str, date: &str) -> Result<(i64, Option<String>)> {
    if currency == "CAD" {
        return Ok((crate::FX_SCALE, None));
    }
    if let Some(quote)=c.query_row("SELECT rate,rate_date FROM fx_rates WHERE source=? AND target='CAD' AND provider='Manual' AND rate_date IS NULL",[currency],|r|Ok((r.get(0)?,r.get(1)?))).optional()? {return Ok(quote);}
    let day = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")?;
    let earliest = day
        .checked_sub_signed(chrono::Duration::days(7))
        .context("汇率日期超出范围")?
        .to_string();
    if let Some(quote)=c.query_row("SELECT rate,rate_date FROM fx_history WHERE source=? AND target='CAD' AND rate_date BETWEEN ? AND ? ORDER BY rate_date DESC,fetched_at DESC LIMIT 1",params![currency,earliest,date],|r|Ok((r.get(0)?,r.get(1)?))).optional()? {return Ok(quote);}
    c.query_row(
        "SELECT rate,rate_date FROM fx_rates WHERE source=? AND target='CAD' AND ((rate_date BETWEEN ? AND ?) OR (provider='Manual' AND rate_date IS NULL))",
        params![currency,earliest,date],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .context("缺少单据日或之前一周内的本地 CAD 汇率；请同步汇率，或在汇率页手工设置适用于该单据的 CAD 汇率")
}
pub fn post(
    c: &Connection,
    source: &str,
    document: i64,
    line: i64,
    date: &str,
    currency: &str,
    calc: &Value,
) -> Result<()> {
    let parts = calc["parts"].as_array().context("税额快照无效")?;
    if parts.is_empty() {
        return Ok(());
    }
    let (fx, fx_date) = cad_rate(c, currency, date)?;
    for part in parts {
        let amount = part["amount"].as_i64().context("税额无效")?;
        let recovered = part["recoverable"].as_i64().context("抵扣额无效")?;
        c.execute("INSERT INTO tax_postings(source,document,line,agency,date,currency,base,amount,recoverable,cad_amount,cad_recoverable,fx_rate,fx_date,snapshot) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)",params![source,document,line,part["agency"].as_i64(),date,currency,calc["net"].as_i64(),amount,recovered,crate::convert(amount,fx)?,crate::convert(recovered,fx)?,fx,fx_date,part.to_string()])?;
    }
    Ok(())
}
pub fn expense_calc(c: &Connection, draft: &crate::accounting::Expense) -> Result<Value> {
    let input = crate::parse_money(&draft.amount)?;
    if draft.id > 0 {
        let saved:Option<String>=c.query_row("SELECT t.snapshot FROM expense_tax t JOIN ledger_expenses l ON l.id=t.expense WHERE expense=? AND input=? AND code=? AND inclusive=? AND recovery=? AND l.currency=? AND l.date=?",params![draft.id,input,draft.tax.code,draft.tax.inclusive,draft.tax.recovery,draft.currency,draft.date],|r|r.get(0)).optional()?;
        if let Some(saved) = saved {
            return Ok(serde_json::from_str(&saved)?);
        }
    }
    calculate(
        c,
        draft.tax.code,
        &draft.date,
        input,
        draft.tax.inclusive,
        true,
        draft.tax.recovery,
    )
}
pub fn save_expense(
    c: &Connection,
    id: i64,
    draft: &crate::accounting::Expense,
    calc: &Value,
) -> Result<()> {
    let unchanged:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM expense_tax WHERE expense=? AND snapshot=? AND input=? AND code=? AND inclusive=? AND recovery=?) AND NOT EXISTS(SELECT 1 FROM tax_postings WHERE source='expense' AND document=? AND (currency!=? OR date!=?))",params![id,calc.to_string(),crate::parse_money(&draft.amount)?,draft.tax.code,draft.tax.inclusive,draft.tax.recovery,id,draft.currency,draft.date],|r|r.get(0))?;
    if unchanged {
        return Ok(());
    }
    c.execute(
        "DELETE FROM tax_postings WHERE source='expense' AND document=?",
        [id],
    )?;
    c.execute("INSERT INTO expense_tax(expense,input,code,inclusive,recovery,net,gross,cost,snapshot) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(expense) DO UPDATE SET input=excluded.input,code=excluded.code,inclusive=excluded.inclusive,recovery=excluded.recovery,net=excluded.net,gross=excluded.gross,cost=excluded.cost,snapshot=excluded.snapshot",params![id,crate::parse_money(&draft.amount)?,draft.tax.code,draft.tax.inclusive,draft.tax.recovery,calc["net"].as_i64(),calc["gross"].as_i64(),calc["cost"].as_i64(),calc.to_string()])?;
    post(c, "expense", id, 1, &draft.date, &draft.currency, calc)
}
pub fn cash(
    c: &Connection,
    agency: i64,
    date: &str,
    kind: &str,
    amount: &str,
    reference: &str,
    reason: &str,
) -> Result<()> {
    record_cash(
        c,
        &CashDraft {
            agency,
            date: date.into(),
            kind: kind.into(),
            amount: amount.into(),
            reference: reference.into(),
            reason: reason.into(),
        },
        None,
    )
}
#[derive(Clone, Debug)]
pub struct CashDraft {
    pub agency: i64,
    pub date: String,
    pub kind: String,
    pub amount: String,
    pub reference: String,
    pub reason: String,
}
pub fn cash_review(c: &Connection, d: &CashDraft) -> Result<String> {
    crate::task_date(&d.date).map_err(anyhow::Error::msg)?;
    crate::required(&d.reason)?;
    ensure!(
        d.date <= crate::today().to_string(),
        "Tax cash date cannot be in the future."
    );
    ensure!(
        ["payment", "refund", "increase", "decrease"].contains(&d.kind.as_str()),
        "税款记录类型无效"
    );
    let amount = crate::parse_money(&d.amount)?;
    ensure!(amount > 0, "金额必须大于零");
    let (agency, payable): (String, i64) = c.query_row(
        "SELECT name,payable_account FROM tax_agencies WHERE id=?",
        [d.agency],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let account = crate::gl::role(
        c,
        if ["payment", "refund"].contains(&d.kind.as_str()) {
            "bank"
        } else {
            "tax_adjustment"
        },
    )?;
    if ["payment", "refund"].contains(&d.kind.as_str()) {
        crate::posting::bank(c, account, "CAD", false)?;
    }
    let locked: String = c.query_row(
        "SELECT locked_through FROM gl_settings WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    ensure!(d.date > locked, "Tax cash date is in a locked period.");
    let quote = crate::fx::posting_quote(c, "CAD", &d.date)?;
    Ok(serde_json::json!({"input":format!("{d:?}"),"agency":agency,"payable":payable,"opposite":account,"quote":quote}).to_string())
}
pub fn cash_reviewed(c: &Connection, d: &CashDraft, expected: &str) -> Result<()> {
    record_cash(c, d, Some(expected))
}
fn record_cash(c: &Connection, d: &CashDraft, expected: Option<&str>) -> Result<()> {
    let tx = c.unchecked_transaction()?;
    let signature = cash_review(&tx, d)?;
    if let Some(expected) = expected {
        ensure!(
            signature == expected,
            "Tax posting inputs changed. Review the record again."
        );
    }
    let amount = crate::parse_money(&d.amount)?;
    tx.execute(
        "INSERT INTO tax_cash(agency,date,kind,amount,reference,reason) VALUES(?,?,?,?,?,?)",
        params![
            d.agency,
            d.date,
            d.kind,
            amount,
            d.reference.trim(),
            d.reason.trim()
        ],
    )?;
    crate::gl::tax_cash(&tx, tx.last_insert_rowid())?;
    tx.commit()?;
    Ok(())
}
pub fn void_cash(c: &Connection, id: i64) -> Result<()> {
    void_cash_on(c, id, &crate::today().to_string())
}
fn void_cash_on(c: &Connection, id: i64, date: &str) -> Result<()> {
    crate::task_date(date).map_err(anyhow::Error::msg)?;
    let tx = c.unchecked_transaction()?;
    let original: String = tx
        .query_row(
            "SELECT date FROM tax_cash WHERE id=? AND voided=0",
            [id],
            |r| r.get(0),
        )
        .context("税款记录不存在或已撤销")?;
    ensure!(date >= original.as_str(), "撤销日期不能早于原税款日期");
    let locked: String = tx.query_row(
        "SELECT locked_through FROM gl_settings WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    ensure!(date > locked.as_str(), "撤销日期属于已锁定期间");
    crate::gl::void_source(&tx, "tax", id, date, "Tax record voided")?;
    ensure!(
        tx.execute(
            "UPDATE tax_cash SET voided=1,voided_on=? WHERE id=? AND voided=0",
            params![date, id]
        )? == 1,
        "税款记录不存在或已撤销"
    );
    tx.commit()?;
    Ok(())
}
pub fn validate(c: &Connection) -> Result<()> {
    let bad:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM expense_tax t JOIN ledger_expenses l ON l.id=t.expense WHERE t.cost!=l.amount OR t.cost!=json_extract(t.snapshot,'$.cost') OR t.gross!=json_extract(t.snapshot,'$.gross') OR t.net!=json_extract(t.snapshot,'$.net'))",[],|r|r.get(0))?;
    ensure!(!bad, "费用税额快照与成本不一致");
    let mut expected = 0usize;
    let mut invoices = c.prepare("SELECT id,snapshot FROM invoices")?;
    for row in invoices.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (id, raw) = row?;
        let data: Value = serde_json::from_str(&raw)?;
        crate::invoice::validate(&data)?;
        if let Some(items) = data["items"].as_array() {
            for (index, item) in items.iter().enumerate() {
                if item.get("tax_snapshot").is_some() {
                    expected += validate_post(
                        c,
                        "invoice",
                        id,
                        (index + 1) as i64,
                        data["invoice_date"].as_str().context("缺少税务日期")?,
                        data["currency"].as_str().context("缺少币种")?,
                        &item["tax_snapshot"],
                    )?;
                }
            }
        }
    }
    let mut expenses=c.prepare("SELECT t.expense,t.snapshot,l.date,l.currency,t.code,t.inclusive,t.input FROM expense_tax t JOIN ledger_expenses l ON l.id=t.expense")?;
    for row in expenses.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, bool>(5)?,
            r.get::<_, i64>(6)?,
        ))
    })? {
        let (id, raw, date, currency, code, inclusive, input) = row?;
        let calc: Value = serde_json::from_str(&raw)?;
        ensure!(
            calc["code_id"].as_i64() == Some(code)
                && if inclusive {
                    calc["gross"].as_i64() == Some(input)
                } else {
                    calc["net"].as_i64() == Some(input)
                },
            "费用税额输入与快照不一致"
        );
        expected += validate_post(c, "expense", id, 1, &date, &currency, &calc)?;
    }
    ensure!(
        c.query_row("SELECT count(*) FROM tax_postings", [], |r| r
            .get::<_, i64>(0))?
            == expected as i64,
        "存在无来源或缺失的税额记录"
    );
    let mut dates =
        c.prepare("SELECT date FROM tax_cash UNION ALL SELECT voided_on FROM tax_cash WHERE voided_on IS NOT NULL UNION ALL SELECT effective FROM tax_rates")?;
    for date in dates.query_map([], |r| r.get::<_, String>(0))? {
        crate::task_date(&date?).map_err(anyhow::Error::msg)?;
    }
    ensure!(!c.query_row("SELECT EXISTS(SELECT 1 FROM tax_cash WHERE (voided=0 AND voided_on IS NOT NULL) OR voided_on<date)",[],|r|r.get::<_,bool>(0))?,"税款撤销日期无效");
    Ok(())
}
fn validate_post(
    c: &Connection,
    source: &str,
    document: i64,
    line: i64,
    date: &str,
    currency: &str,
    calc: &Value,
) -> Result<usize> {
    crate::task_date(date).map_err(anyhow::Error::msg)?;
    let parts = calc["parts"].as_array().context("税目快照损坏")?;
    let mut stmt=c.prepare("SELECT agency,base,amount,recoverable,cad_amount,cad_recoverable,fx_rate,date,currency,snapshot FROM tax_postings WHERE source=? AND document=? AND line=? ORDER BY id")?;
    let rows = stmt
        .query_map(params![source, document, line], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, String>(9)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(rows.len() == parts.len(), "税目数量与单据快照不一致");
    let (mut taxes, mut recovered) = (0i64, 0i64);
    for (part, row) in parts.iter().zip(rows) {
        let (agency, base, amount, credit, cad, cad_credit, fx, posted_date, posted_currency, raw) =
            row;
        ensure!(
            part["agency"].as_i64() == Some(agency)
                && calc["net"].as_i64() == Some(base)
                && part["amount"].as_i64() == Some(amount)
                && part["recoverable"].as_i64() == Some(credit)
                && credit >= 0
                && credit <= amount
                && amount >= 0
                && serde_json::from_str::<Value>(&raw)? == *part
                && posted_date == date
                && posted_currency == currency
                && fx > 0
                && cad == crate::convert(amount, fx)?
                && cad_credit == crate::convert(credit, fx)?,
            "税务流水与快照不一致"
        );
        taxes = taxes.checked_add(amount).context("税额过大")?;
        recovered = recovered.checked_add(credit).context("抵扣额过大")?;
    }
    let net = calc["net"].as_i64().context("未税金额损坏")?;
    let gross = net.checked_add(taxes).context("含税金额过大")?;
    ensure!(
        net >= 0
            && calc["tax"].as_i64() == Some(taxes)
            && calc["gross"].as_i64() == Some(gross)
            && calc["recoverable"].as_i64() == Some(recovered)
            && calc["cost"].as_i64() == Some(gross - recovered),
        "税额合计损坏"
    );
    Ok(parts.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tax_cash_review_is_read_only_and_rechecks_bank_and_period() -> Result<()> {
        let c = fixture()?;
        let mut draft = CashDraft {
            agency: 1,
            date: crate::today().to_string(),
            kind: "payment".into(),
            amount: "10.00".into(),
            reference: "Review".into(),
            reason: "Tax remittance".into(),
        };
        let changes = c.total_changes();
        let reviewed = cash_review(&c, &draft)?;
        assert_eq!(changes, c.total_changes());
        let bank = crate::gl::role(&c, "bank")?;
        c.execute("INSERT INTO ap_banks(id,currency) VALUES(?,'USD')", [bank])?;
        assert!(cash_reviewed(&c, &draft, &reviewed).is_err());
        c.execute("DELETE FROM ap_banks WHERE id=?", [bank])?;
        c.execute(
            "UPDATE gl_settings SET locked_through=? WHERE id=1",
            [&draft.date],
        )?;
        assert!(cash_reviewed(&c, &draft, &reviewed).is_err());
        c.execute("UPDATE gl_settings SET locked_through='' WHERE id=1", [])?;
        draft.amount = "11.00".into();
        assert!(cash_reviewed(&c, &draft, &reviewed).is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM tax_cash", [], |r| r.get::<_, i64>(0))?,
            0
        );
        draft.date = (crate::today() + chrono::Duration::days(1)).to_string();
        assert!(cash_review(&c, &draft).is_err());
        draft.date = crate::today().to_string();
        cash_reviewed(&c, &draft, &cash_review(&c, &draft)?)?;
        assert_eq!(
            c.query_row("SELECT count(*) FROM tax_cash", [], |r| r.get::<_, i64>(0))?,
            1
        );
        crate::storage::validate_projections(&c)?;
        Ok(())
    }
    #[test]
    fn tax_void_preserves_historical_balances_and_posts_on_reversal_date() -> Result<()> {
        let c = fixture()?;
        cash(&c, 1, "2026-10-01", "payment", "5", "REF", "Tax remittance")?;
        let id: i64 = c.query_row("SELECT max(id) FROM tax_cash", [], |r| r.get(0))?;
        let before = admin::summary(&c, "2026-10-01", "2026-10-10")?;
        assert_eq!(before[0][5], "-5.00");
        assert!(void_cash_on(&c, id, "2026-09-30").is_err());
        c.execute("UPDATE gl_settings SET locked_through='2026-10-15'", [])?;
        assert!(void_cash_on(&c, id, "2026-10-15").is_err());
        assert_eq!(
            c.query_row("SELECT voided FROM tax_cash WHERE id=?", [id], |r| r
                .get::<_, i64>(0))?,
            0
        );
        c.execute("UPDATE gl_settings SET locked_through=''", [])?;
        void_cash_on(&c, id, "2026-10-15")?;
        assert_eq!(admin::summary(&c, "2026-10-01", "2026-10-10")?, before);
        assert_eq!(
            admin::summary(&c, "2026-10-01", "2026-10-15")?[0][5],
            "0.00"
        );
        assert!(admin::detail(&c, "2026-10-01", "2026-10-10")?
            .iter()
            .any(|r| r[2] == "payment" && r[7] == "-5.00"));
        assert!(admin::detail(&c, "2026-10-11", "2026-10-20")?
            .iter()
            .any(|r| r[2] == "void_payment" && r[7] == "5.00"));
        assert!(void_cash_on(&c, id, "2026-10-16").is_err());
        crate::storage::validate_projections(&c)?;
        Ok(())
    }
    #[test]
    fn migration_uses_actual_reversal_date_and_leaves_unknown_legacy_dates_empty() -> Result<()> {
        let c = fixture()?;
        cash(&c, 1, "2026-10-01", "payment", "5", "REF", "Tax payment")?;
        let id: i64 = c.query_row("SELECT max(id) FROM tax_cash", [], |r| r.get(0))?;
        void_cash_on(&c, id, "2026-10-15")?;
        c.execute_batch("DROP TRIGGER tax_cash_fixed;DROP TRIGGER tax_cash_initial;DROP TRIGGER audit_tax_cash_INSERT;DROP TRIGGER audit_tax_cash_UPDATE;DROP TRIGGER audit_tax_cash_DELETE;ALTER TABLE tax_cash DROP COLUMN voided_on;INSERT INTO tax_cash(agency,date,kind,amount,reference,reason,voided) VALUES(1,'2020-01-01','payment',100,'legacy','Unknown void date',1);PRAGMA user_version=13;")?;
        crate::init(&c)?;
        assert_eq!(
            c.query_row("SELECT voided_on FROM tax_cash WHERE id=?", [id], |r| {
                r.get::<_, String>(0)
            })?,
            "2026-10-15"
        );
        assert!(c
            .query_row(
                "SELECT voided_on FROM tax_cash WHERE reference='legacy'",
                [],
                |r| r.get::<_, Option<String>>(0)
            )?
            .is_none());
        assert!(crate::compliance::review(&c)?
            .iter()
            .any(|f| f.code == "LEGACY_VOID_DATE"));
        crate::init(&c)?;
        assert_eq!(
            admin::summary(&c, "2026-10-01", "2026-10-10")?[0][5],
            "-5.00"
        );
        Ok(())
    }
    fn fixture() -> Result<Connection> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute_batch("UPDATE tax_agencies SET registered=1,registration='TEST-ONLY'; INSERT INTO customers(id,name,contact) VALUES(1,'Customer',''); INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor',''); INSERT INTO prices(id,vendor,name,unit,cost,price,currency,tax_code) VALUES(1,1,'Service','item',0,10000,'CAD',3);")?;
        Ok(c)
    }
    #[test]
    fn populated_legacy_tables_migrate_without_rewriting_amounts() -> Result<()> {
        let c = Connection::open_in_memory()?;
        c.execute_batch("CREATE TABLE vendors(id INTEGER PRIMARY KEY,name TEXT NOT NULL,contact TEXT NOT NULL); INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');
            CREATE TABLE prices(id INTEGER PRIMARY KEY,vendor INTEGER NOT NULL,name TEXT NOT NULL,unit TEXT NOT NULL,cost INTEGER NOT NULL,price INTEGER NOT NULL,currency TEXT NOT NULL); INSERT INTO prices VALUES(1,1,'Historical service','item',100,5000,'CAD');
            CREATE TABLE tasks(id INTEGER PRIMARY KEY,title TEXT NOT NULL,customer TEXT NOT NULL,progress INTEGER NOT NULL DEFAULT 0); INSERT INTO tasks VALUES(1,'Task','Customer',0);
            CREATE TABLE items(id INTEGER PRIMARY KEY,task INTEGER NOT NULL,description TEXT NOT NULL,unit TEXT NOT NULL,quantity INTEGER NOT NULL,cost INTEGER NOT NULL,price INTEGER NOT NULL); INSERT INTO items VALUES(1,1,'Historical service','item',1,100,5000); PRAGMA user_version=5;")?;
        crate::init(&c)?;
        assert_eq!(
            c.query_row("SELECT price FROM prices WHERE id=1", [], |r| r
                .get::<_, i64>(0))?,
            5000
        );
        assert_eq!(
            c.query_row("SELECT tax_code FROM items WHERE id=1", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        assert!(c
            .execute("UPDATE items SET tax_code=999 WHERE id=1", [])
            .is_err());
        assert!(c.execute("DELETE FROM tax_codes WHERE id=0", []).is_err());
        crate::storage::validate_projections(&c)?;
        Ok(())
    }
    #[test]
    fn exact_rates_inclusive_rounding_and_effective_dates() -> Result<()> {
        let c = fixture()?;
        assert_eq!(percent("9.975")?, 99750);
        assert_eq!(percent("0")?, 0);
        for bad in ["-5", "1e2", "100.0001", "5.00001", ""] {
            assert!(percent(bad).is_err());
        }
        let calc = calculate(&c, 3, "2026-10-26", 11200, true, true, 100)?;
        assert_eq!(calc["net"], 10000);
        assert_eq!(calc["tax"], 1200);
        assert_eq!(calc["recoverable"], 500);
        assert_eq!(calc["cost"], 10700);
        for amount in 0..=600 {
            let calc = calculate(&c, 3, "2026-10-26", amount, true, true, 50)?;
            assert_eq!(calc["gross"].as_i64(), Some(amount));
            assert_eq!(
                calc["net"].as_i64().unwrap() + calc["tax"].as_i64().unwrap(),
                amount
            );
            assert!(calc["parts"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p["amount"].as_i64().unwrap() >= 0));
        }
        assert_eq!(
            calculate(&c, 5, "2025-03-31", 10000, false, false, 0)?["tax"],
            1500
        );
        assert_eq!(
            calculate(&c, 5, "2025-04-01", 10000, false, false, 0)?["tax"],
            1400
        );
        assert_eq!(
            calculate(&c, 7, "2026-10-26", 10000, false, false, 0)?["kind"],
            "zero"
        );
        assert_eq!(
            calculate(&c, 8, "2026-10-26", 10000, false, false, 0)?["kind"],
            "exempt"
        );
        c.execute("UPDATE tax_agencies SET registered=0 WHERE id=1", [])?;
        assert!(calculate(&c, 1, "2026-10-26", 10000, false, false, 0).is_err());
        assert_eq!(
            calculate(&c, 1, "2026-10-26", 10500, true, true, 100)?["recoverable"],
            0
        );
        Ok(())
    }
    #[test]
    fn invoices_expenses_receipts_and_tax_cash_reconcile() -> Result<()> {
        let mut c = fixture()?;
        for i in 1..=2 {
            crate::add_task(&c, "Task", 1, "CAD")?;
            let item = crate::add_item(&c, i, 1, 1, None, None)?;
            if i == 2 {
                c.execute("UPDATE items SET tax_code=1 WHERE id=?", [item])?;
            }
            crate::set_task_status(&mut c, i, false)?;
        }
        let id = crate::billing::merge(&mut c, &[1, 2])?;
        assert_eq!(crate::billing::balance(&c, id)?.total, 21700);
        let raw: String = c.query_row("SELECT snapshot FROM invoices WHERE id=?", [id], |r| {
            r.get(0)
        })?;
        let data: Value = serde_json::from_str(&raw)?;
        assert_eq!(data["profit"], 20000);
        assert_eq!(data["tax_total"], 1700);
        crate::billing::add_payment(&mut c, id, &crate::today().to_string(), "200", "Cash", "")?;
        assert_eq!(crate::billing::balance(&c, id)?.remaining, 1700);
        assert!(crate::billing::add_payment(
            &mut c,
            id,
            &crate::today().to_string(),
            "17.01",
            "Cash",
            ""
        )
        .is_err());
        let account = crate::accounting::default_account(&c)?;
        let e = crate::accounting::Expense {
            id: 0,
            date: crate::today().to_string(),
            description: "Purchase".into(),
            account,
            vendor: Some(1),
            currency: "CAD".into(),
            amount: "112.00".into(),
            task: None,
            tax: Input {
                code: 3,
                inclusive: true,
                recovery: 100,
            },
        };
        let expense = crate::accounting::save_expense(&mut c, &e)?;
        assert_eq!(
            c.query_row(
                "SELECT amount FROM ledger_expenses WHERE id=?",
                [expense],
                |r| r.get::<_, i64>(0)
            )?,
            10700
        );
        let summary = admin::summary(&c, "2000-01-01", "2099-12-31")?;
        assert_eq!(summary[0][5], "5.00");
        assert_eq!(summary[1][5], "7.00");
        cash(
            &c,
            1,
            &crate::today().to_string(),
            "payment",
            "5",
            "REF",
            "Paid",
        )?;
        assert_eq!(
            admin::summary(&c, "2000-01-01", "2099-12-31")?[0][5],
            "0.00"
        );
        cash(
            &c,
            1,
            &crate::today().to_string(),
            "increase",
            "2",
            "REF",
            "Adjustment",
        )?;
        assert_eq!(
            admin::summary(&c, "2000-01-01", "2099-12-31")?[0][5],
            "2.00"
        );
        void_cash(&c, 2)?;
        assert_eq!(
            admin::summary(&c, "2000-01-01", "2099-12-31")?[0][5],
            "0.00"
        );
        c.execute(
            "UPDATE tax_agencies SET registration='Changed' WHERE id=1",
            [],
        )?;
        assert_eq!(
            c.query_row("SELECT snapshot FROM invoices WHERE id=?", [id], |r| r
                .get::<_, String>(
                0
            ))?,
            raw
        );
        assert!(c.execute("UPDATE tax_rates SET rate=0", []).is_err());
        assert!(c
            .execute("DELETE FROM tax_postings WHERE source='invoice'", [])
            .is_err());
        crate::storage::validate_projections(&c)?;
        c.execute(
            "UPDATE tax_postings SET cad_amount=cad_amount+1 WHERE source='expense'",
            [],
        )?;
        assert!(validate(&c).is_err());
        Ok(())
    }
    #[test]
    fn taxed_task_cost_excludes_recoverable_tax_and_reports_remain_separate() -> Result<()> {
        let mut c = fixture()?;
        crate::add_task(&c, "Task", 1, "CAD")?;
        crate::add_item(&c, 1, 1, 1, None, None)?;
        let e = crate::accounting::Expense {
            id: 0,
            date: crate::today().to_string(),
            description: "Purchase".into(),
            account: crate::accounting::default_account(&c)?,
            vendor: Some(1),
            currency: "CAD".into(),
            amount: "112".into(),
            task: Some(1),
            tax: Input {
                code: 3,
                inclusive: true,
                recovery: 100,
            },
        };
        crate::accounting::save_expense(&mut c, &e)?;
        assert_eq!(crate::snapshot(&c, 1)?["cost"], 10700);
        assert_eq!(crate::snapshot(&c, 1)?["profit"], -700);
        crate::set_task_status(&mut c, 1, false)?;
        let invoice = crate::billing::merge(&mut c, &[1])?;
        assert_eq!(crate::billing::balance(&c, invoice)?.total, 11200);
        let rows = admin::summary(&c, "2000-01-01", "2099-12-31")?;
        assert_eq!(rows[0][5], "0.00");
        assert_eq!(rows[1][5], "7.00");
        crate::storage::validate_projections(&c)?;
        Ok(())
    }
    #[test]
    fn tax_configuration_assignments_and_rate_history_are_validated() -> Result<()> {
        let mut c = fixture()?;
        assert!(admin::code(
            &mut c,
            -1,
            "BAD",
            "Duplicate GST/HST",
            "taxable",
            true,
            &[1, 3]
        )
        .is_err());
        let custom = admin::code(&mut c, -1, "CUSTOM", "Custom GST", "taxable", true, &[1])?;
        admin::assign(&c, "customer", 1, custom)?;
        admin::assign(&c, "price", 1, 0)?;
        crate::add_task(&c, "Task", 1, "CAD")?;
        let item = crate::add_item(&c, 1, 1, 1, None, None)?;
        assert_eq!(
            c.query_row("SELECT tax_code FROM items WHERE id=?", [item], |r| r
                .get::<_, i64>(0))?,
            custom
        );
        admin::rate(&mut c, 1, 1, "GST", true, "2027-01-01", "6")?;
        assert_eq!(
            calculate(&c, 1, "2026-12-31", 10000, false, false, 0)?["tax"],
            500
        );
        assert_eq!(
            calculate(&c, 1, "2027-01-01", 10000, false, false, 0)?["tax"],
            600
        );
        assert!(admin::rate(&mut c, 1, 1, "GST", true, "2027-01-01", "7").is_err());
        assert!(admin::agency(&c, 1, "CRA", "", true, 2, Some(3)).is_err());
        Ok(())
    }
    #[test]
    fn detail_export_includes_cash_and_zero_rated_documents() -> Result<()> {
        let mut c = fixture()?;
        crate::add_task(&c, "Zero-rated", 1, "CAD")?;
        let item = crate::add_item(&c, 1, 1, 1, None, None)?;
        c.execute("UPDATE items SET tax_code=7 WHERE id=?", [item])?;
        crate::set_task_status(&mut c, 1, false)?;
        crate::billing::merge(&mut c, &[1])?;
        cash(
            &c,
            1,
            "2026-10-26",
            "payment",
            "5",
            "=HYPERLINK(\"x\")",
            "Payment recorded",
        )?;
        let rows = admin::detail(&c, "2000-01-01", "2099-12-31")?;
        assert!(rows.iter().all(|r| r.len() == 12));
        assert!(rows.iter().any(|r| r[2] == "payment" && r[7] == "-5.00"));
        assert!(rows.iter().any(|r| r[1] == "Z" && r[3] == "INV0001"));
        let path = std::env::temp_dir().join(format!(
            "axibiz-tax-export-{}-{}.csv",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        admin::export(
            &[vec!["=SUM(1,2)".into(), "A,\"B\"".into()]],
            &["One", "Two"],
            &path,
        )?;
        let content = std::fs::read_to_string(&path)?;
        assert!(content.contains("'=SUM(1,2)"));
        assert!(content.contains("A,\"\"B\"\""));
        assert!(admin::export(&rows, &[], &path).is_err());
        std::fs::remove_file(path)?;
        Ok(())
    }
    #[test]
    fn cad_quotes_do_not_use_future_automatic_rates() -> Result<()> {
        let c = fixture()?;
        c.execute("INSERT INTO fx_rates(source,target,rate,provider,rate_date) VALUES('USD','CAD',140000000,'Bank of Canada','2026-10-26')",[])?;
        assert!(cad_rate(&c, "USD", "2026-09-01").is_err());
        c.execute("INSERT INTO fx_history(source,target,rate,provider,rate_date) VALUES('USD','CAD',135000000,'Bank of Canada','2026-08-31')",[])?;
        assert_eq!(cad_rate(&c, "USD", "2026-09-01")?.0, 135000000);
        Ok(())
    }
    #[test]
    fn foreign_expense_description_edits_preserve_cad_snapshot() -> Result<()> {
        let mut c = fixture()?;
        c.execute(
            "INSERT INTO fx_rates(source,target,rate) VALUES('USD','CAD',135000000)",
            [],
        )?;
        let mut e = crate::accounting::Expense {
            id: 0,
            date: "2026-10-26".into(),
            description: "Purchase".into(),
            account: crate::accounting::default_account(&c)?,
            vendor: Some(1),
            currency: "USD".into(),
            amount: "105".into(),
            task: None,
            tax: Input {
                code: 1,
                inclusive: true,
                recovery: 100,
            },
        };
        e.id = crate::accounting::save_expense(&mut c, &e)?;
        let before: i64 = c.query_row(
            "SELECT cad_recoverable FROM tax_postings WHERE source='expense'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(before, 675);
        c.execute(
            "UPDATE fx_rates SET rate=140000000 WHERE source='USD' AND target='CAD'",
            [],
        )?;
        e.description = "Description changed".into();
        crate::accounting::save_expense(&mut c, &e)?;
        assert_eq!(
            c.query_row(
                "SELECT cad_recoverable FROM tax_postings WHERE source='expense'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            675
        );
        validate(&c)?;
        Ok(())
    }
}
