use super::*;
pub fn agency(
    c: &Connection,
    id: i64,
    name: &str,
    registration: &str,
    registered: bool,
    payable: i64,
    recoverable: Option<i64>,
) -> Result<i64> {
    crate::required(name)?;
    if registered {
        crate::required(registration)?;
    }
    ensure!(
        c.query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=? AND kind='Liability' AND active=1)",
            [payable],
            |r| r.get::<_, bool>(0)
        )?,
        "请选择启用的负债类应缴税科目"
    );
    if let Some(account) = recoverable {
        ensure!(
            c.query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=? AND kind='Asset' AND active=1)",
                [account],
                |r| r.get::<_, bool>(0)
            )?,
            "请选择启用的资产类进项税科目"
        );
    }
    if id > 0 {
        ensure!(c.execute("UPDATE tax_agencies SET name=?,registration=?,registered=?,payable_account=?,recoverable_account=? WHERE id=?",params![name.trim(),registration.trim(),registered,payable,recoverable,id])?==1,"税务机构不存在");
        Ok(id)
    } else {
        c.execute("INSERT INTO tax_agencies(name,registration,registered,payable_account,recoverable_account) VALUES(?,?,?,?,?)",params![name.trim(),registration.trim(),registered,payable,recoverable])?;
        Ok(c.last_insert_rowid())
    }
}
pub fn rate(
    c: &mut Connection,
    item: i64,
    agency: i64,
    name: &str,
    recoverable: bool,
    effective: &str,
    value: &str,
) -> Result<i64> {
    crate::required(name)?;
    crate::task_date(effective).map_err(anyhow::Error::msg)?;
    let rate = percent(value)?;
    let tx = c.transaction()?;
    let item = if item > 0 {
        item
    } else {
        tx.execute(
            "INSERT INTO tax_items(agency,name,recoverable) VALUES(?,?,?)",
            params![agency, name.trim(), recoverable],
        )?;
        tx.last_insert_rowid()
    };
    tx.execute(
        "INSERT INTO tax_rates(item,effective,rate) VALUES(?,?,?)",
        params![item, effective, rate],
    )
    .context("该税目此生效日已有税率，请使用新的生效日")?;
    tx.commit()?;
    Ok(item)
}
pub fn code(
    c: &mut Connection,
    id: i64,
    code: &str,
    name: &str,
    kind: &str,
    active: bool,
    items: &[i64],
) -> Result<i64> {
    crate::required(code)?;
    crate::required(name)?;
    ensure!(id != 0, "系统 NONE 税码不能修改");
    ensure!(
        ["taxable", "zero", "exempt", "none"].contains(&kind),
        "税码类别无效"
    );
    let tx = c.transaction()?;
    if kind == "taxable" {
        ensure!(!items.is_empty(), "应税税码请至少选择一个税目");
        let mut agencies = std::collections::BTreeSet::new();
        for item in items {
            let agency: i64 =
                tx.query_row("SELECT agency FROM tax_items WHERE id=?", [item], |r| {
                    r.get(0)
                })?;
            ensure!(
                agencies.insert(agency),
                "同一税务机构只能选择一个税目，不能同时使用 GST 与 HST"
            );
        }
    }
    let id = if id > 0 {
        ensure!(
            tx.execute(
                "UPDATE tax_codes SET code=?,name=?,kind=?,active=? WHERE id=?",
                params![code.trim(), name.trim(), kind, active, id]
            )? == 1,
            "税码不存在"
        );
        id
    } else {
        tx.execute(
            "INSERT INTO tax_codes(code,name,kind,active) VALUES(?,?,?,?)",
            params![code.trim(), name.trim(), kind, active],
        )?;
        tx.last_insert_rowid()
    };
    tx.execute("DELETE FROM tax_code_items WHERE code=?", [id])?;
    if kind == "taxable" {
        for item in items {
            tx.execute("INSERT INTO tax_code_items VALUES(?,?)", params![id, item])?;
        }
    }
    tx.commit()?;
    Ok(id)
}
pub fn assign(c: &Connection, entity: &str, record: i64, code: i64) -> Result<()> {
    ensure!(
        c.query_row(
            "SELECT EXISTS(SELECT 1 FROM tax_codes WHERE id=? AND active=1)",
            [code],
            |r| r.get::<_, bool>(0)
        )?,
        "请选择启用的税码"
    );
    let table = match entity {
        "customer" => "customers",
        "vendor" => "vendors",
        "price" => "prices",
        _ => anyhow::bail!("关联类别无效"),
    };
    ensure!(
        c.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?)"),
            [record],
            |r| r.get::<_, bool>(0)
        )?,
        "记录不存在"
    );
    if entity == "price" {
        c.execute(
            "UPDATE prices SET tax_code=? WHERE id=?",
            params![code, record],
        )?;
    } else {
        c.execute("INSERT INTO tax_assignments VALUES(?,?,?) ON CONFLICT(entity,record) DO UPDATE SET code=excluded.code",params![entity,record,code])?;
    }
    Ok(())
}
fn dates(from: &str, to: &str) -> Result<()> {
    crate::task_date(from).map_err(anyhow::Error::msg)?;
    crate::task_date(to).map_err(anyhow::Error::msg)?;
    ensure!(from <= to, "开始日期不能晚于截止日期");
    Ok(())
}
pub fn summary(c: &Connection, from: &str, to: &str) -> Result<Vec<Vec<String>>> {
    dates(from, to)?;
    let mut result = Vec::new();
    let mut stmt = c.prepare("SELECT id,name FROM tax_agencies ORDER BY id")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (id, name) = row?;
        let (out,input):(i64,i64)=c.query_row("SELECT COALESCE(sum(CASE WHEN source='invoice' THEN cad_amount ELSE 0 END),0),COALESCE(sum(CASE WHEN source='expense' THEN cad_recoverable ELSE 0 END),0) FROM tax_postings WHERE agency=? AND date BETWEEN ? AND ?",params![id,from,to],|r|Ok((r.get(0)?,r.get(1)?)))?;
        let balance:i64=c.query_row("SELECT (SELECT COALESCE(sum(CASE WHEN source='invoice' THEN cad_amount ELSE -cad_recoverable END),0) FROM tax_postings WHERE agency=?1 AND date<=?2)+(SELECT COALESCE(sum(CASE WHEN kind IN('payment','decrease') THEN -amount ELSE amount END),0) FROM tax_cash WHERE agency=?1 AND date<=?2 AND (voided=0 OR voided_on>?2))",params![id,to],|r|r.get(0))?;
        result.push(vec![
            id.to_string(),
            name,
            crate::format_money(out),
            crate::format_money(input),
            crate::format_money(out.checked_sub(input).context("税额合计过大")?),
            crate::format_money(balance),
        ]);
    }
    Ok(result)
}
pub fn detail(c: &Connection, from: &str, to: &str) -> Result<Vec<Vec<String>>> {
    dates(from, to)?;
    let mut stmt=c.prepare("SELECT p.date,a.name,p.source,COALESCE(CASE p.source WHEN 'invoice' THEN (SELECT number FROM invoices WHERE id=p.document) ELSE (SELECT d.number FROM expense_document_lines x JOIN expense_documents d ON d.id=x.document WHERE x.expense=p.document) END,CAST(p.document AS TEXT)),p.currency,p.base,p.amount,p.cad_amount,p.cad_recoverable,p.fx_rate,COALESCE(p.fx_date,''),p.snapshot FROM tax_postings p JOIN tax_agencies a ON a.id=p.agency WHERE p.date BETWEEN ? AND ? ORDER BY p.date,p.id")?;
    let mut rows = stmt
        .query_map(params![from, to], |r| {
            Ok(vec![
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get::<_, String>(3)?,
                r.get(4)?,
                crate::format_money(r.get(5)?),
                crate::format_money(r.get(6)?),
                crate::format_money(r.get(7)?),
                crate::format_money(r.get(8)?),
                format!("{:.8}", r.get::<_, i64>(9)? as f64 / crate::FX_SCALE as f64),
                r.get(10)?,
                r.get(11)?,
            ])
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut stmt=c.prepare("SELECT t.date,a.name,t.kind,t.id,t.amount,t.reference,t.reason FROM tax_cash t JOIN tax_agencies a ON a.id=t.agency WHERE t.date BETWEEN ? AND ? AND (t.voided=0 OR t.voided_on IS NOT NULL) ORDER BY t.date,t.id")?;
    for row in stmt.query_map(params![from, to], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
        ))
    })? {
        let (date, agency, kind, id, amount, reference, reason) = row?;
        let movement = if ["payment", "decrease"].contains(&kind.as_str()) {
            -amount
        } else {
            amount
        };
        rows.push(vec![
            date,
            agency,
            kind,
            id.to_string(),
            "CAD".into(),
            "0.00".into(),
            crate::format_money(amount),
            crate::format_money(movement),
            "0.00".into(),
            "1.00000000".into(),
            String::new(),
            json!({"reference":reference,"reason":reason}).to_string(),
        ]);
    }
    let mut reversals=c.prepare("SELECT t.voided_on,a.name,t.kind,t.id,t.amount,t.reference,t.reason FROM tax_cash t JOIN tax_agencies a ON a.id=t.agency WHERE t.voided=1 AND t.voided_on BETWEEN ? AND ? ORDER BY t.voided_on,t.id")?;
    for row in reversals.query_map(params![from, to], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
        ))
    })? {
        let (date, agency, kind, id, amount, reference, reason) = row?;
        let movement = if ["payment", "decrease"].contains(&kind.as_str()) {
            amount
        } else {
            -amount
        };
        rows.push(vec![
            date,
            agency,
            format!("void_{kind}"),
            id.to_string(),
            "CAD".into(),
            "0.00".into(),
            crate::format_money(amount),
            crate::format_money(movement),
            "0.00".into(),
            "1.00000000".into(),
            String::new(),
            json!({"reference":reference,"reason":reason,"reversal":true}).to_string(),
        ]);
    }
    let mut stmt=c.prepare("SELECT COALESCE(json_extract(i.snapshot,'$.invoice_date'),substr(i.created,1,10)),COALESCE(i.number,CAST(i.id AS TEXT)),i.snapshot FROM invoices i WHERE COALESCE(json_extract(i.snapshot,'$.invoice_date'),substr(i.created,1,10)) BETWEEN ? AND ?")?;
    for row in stmt.query_map(params![from, to], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (date, number, raw) = row?;
        let data: Value = serde_json::from_str(&raw)?;
        for item in data["items"].as_array().context("Invoice items missing")? {
            let tax = &item["tax_snapshot"];
            if tax["kind"] == "zero" || tax["kind"] == "exempt" {
                rows.push(vec![
                    date.clone(),
                    tax["code"].as_str().unwrap_or("").into(),
                    "invoice".into(),
                    number.clone(),
                    data["currency"].as_str().unwrap_or("").into(),
                    crate::format_money(tax["net"].as_i64().unwrap_or(0)),
                    "0.00".into(),
                    "0.00".into(),
                    "0.00".into(),
                    String::new(),
                    String::new(),
                    tax.to_string(),
                ]);
            }
        }
    }
    let mut stmt=c.prepare("SELECT l.date,d.number,l.currency,t.snapshot FROM expense_tax t JOIN ledger_expenses l ON l.id=t.expense JOIN expense_document_lines x ON x.expense=l.id JOIN expense_documents d ON d.id=x.document WHERE l.date BETWEEN ? AND ? AND json_extract(t.snapshot,'$.kind') IN('zero','exempt')")?;
    for row in stmt.query_map(params![from, to], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (date, number, currency, raw) = row?;
        let tax: Value = serde_json::from_str(&raw)?;
        rows.push(vec![
            date,
            tax["code"].as_str().unwrap_or("").into(),
            "expense".into(),
            number,
            currency,
            crate::format_money(tax["net"].as_i64().unwrap_or(0)),
            "0.00".into(),
            "0.00".into(),
            "0.00".into(),
            String::new(),
            String::new(),
            raw,
        ]);
    }
    rows.sort_by(|a, b| a[0].cmp(&b[0]));
    Ok(rows)
}
pub fn export(rows: &[Vec<String>], headers: &[&str], path: &std::path::Path) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    let cells = rows
        .iter()
        .map(|row| row.iter().cloned().map(crate::csv::Cell::Text).collect())
        .collect::<Vec<_>>();
    file.write_all(&crate::csv::encode(headers, &cells))?;
    Ok(())
}
