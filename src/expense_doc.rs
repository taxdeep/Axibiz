use crate::accounting::Expense;
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub id: i64,
    pub vendor: Option<i64>,
    pub date: String,
    pub currency: String,
    pub reference: String,
    pub due_date: Option<String>,
    pub memo: String,
    pub terms: String,
    pub lines: Vec<Expense>,
}
pub fn init(c: &Connection) -> Result<()> {
    let has: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('ledger_expenses') WHERE name='number')",
        [],
        |r| r.get(0),
    )?;
    if !has {
        c.execute_batch("ALTER TABLE ledger_expenses ADD COLUMN number TEXT;")?;
    }
    c.execute_batch("CREATE TABLE IF NOT EXISTS expense_documents(id INTEGER PRIMARY KEY,number TEXT UNIQUE,date TEXT,vendor INTEGER REFERENCES vendors(id),currency TEXT NOT NULL,reference TEXT NOT NULL DEFAULT '',due_date TEXT,memo TEXT NOT NULL DEFAULT '',created TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        CREATE TABLE IF NOT EXISTS expense_document_lines(document INTEGER NOT NULL REFERENCES expense_documents(id),expense INTEGER NOT NULL UNIQUE REFERENCES ledger_expenses(id),PRIMARY KEY(document,expense));")?;
    if !c.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('expense_documents') WHERE name='terms')",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        c.execute_batch(
            "ALTER TABLE expense_documents ADD COLUMN terms TEXT NOT NULL DEFAULT '';",
        )?;
    }
    let ids = {
        let mut stmt=c.prepare("SELECT id FROM ledger_expenses WHERE id NOT IN (SELECT expense FROM expense_document_lines) ORDER BY id")?;
        let rows = stmt
            .query_map([], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for id in ids {
        c.execute("INSERT INTO expense_documents(number,date,vendor,currency) SELECT number,date,vendor,currency FROM ledger_expenses WHERE id=?",[id])?;
        c.execute(
            "INSERT INTO expense_document_lines VALUES(?,?)",
            params![c.last_insert_rowid(), id],
        )?;
    }
    Ok(())
}
pub fn register_single(c: &Connection, expense: i64) -> Result<i64> {
    c.execute("INSERT INTO expense_documents(date,vendor,currency) SELECT date,vendor,currency FROM ledger_expenses WHERE id=?",[expense])?;
    let id = c.last_insert_rowid();
    crate::numbering::assign(c, "expense", id)?;
    c.execute(
        "INSERT INTO expense_document_lines VALUES(?,?)",
        params![id, expense],
    )?;
    crate::gl::expense(c, id, true)?;
    Ok(id)
}
pub fn load(c: &Connection, id: i64) -> Result<Document> {
    let mut doc=c.query_row("SELECT id,vendor,COALESCE(date,''),currency,reference,due_date,memo,terms FROM expense_documents WHERE id=?",[id],|r|Ok(Document{id:r.get(0)?,vendor:r.get(1)?,date:r.get(2)?,currency:r.get(3)?,reference:r.get(4)?,due_date:r.get(5)?,memo:r.get(6)?,terms:r.get(7)?,lines:Vec::new()})).context("费用单不存在")?;
    let mut stmt =
        c.prepare("SELECT expense FROM expense_document_lines WHERE document=? ORDER BY expense")?;
    for id in stmt.query_map([id], |r| r.get::<_, i64>(0))? {
        doc.lines.push(crate::accounting::load_expense(c, id?)?);
    }
    Ok(doc)
}
pub struct Review {
    pub gross: i64,
    pub recoverable: i64,
    pub signature: String,
}
/// Read-only review of the same tax/FX inputs that will be checked under the write lock.
pub fn review(c: &Connection, doc: &Document) -> Result<Review> {
    let mut taxes = Vec::new();
    let (mut gross, mut recoverable) = (0i64, 0i64);
    for line in &doc.lines {
        let mut line = line.clone();
        line.date = doc.date.clone();
        line.currency = doc.currency.clone();
        let tax = crate::tax::expense_calc(c, &line)?;
        gross = gross
            .checked_add(tax["gross"].as_i64().context("Invalid gross")?)
            .context("Bill total too large")?;
        recoverable = recoverable
            .checked_add(tax["recoverable"].as_i64().context("Invalid recovery")?)
            .context("Bill recovery too large")?;
        taxes.push(tax);
    }
    let old = if doc.id > 0 {
        Some(load(c, doc.id)?)
    } else {
        None
    };
    let fixed:Option<i64>=c.query_row("SELECT e.fx_rate FROM gl_sources s JOIN gl_entries e ON e.id=s.entry WHERE s.kind='expense' AND s.record=?1 AND e.date=?2 AND e.currency=?3",params![doc.id,doc.date,doc.currency],|r|r.get(0)).optional()?;
    let rate = match fixed {
        Some(rate) => rate,
        None => crate::gl::quote(c, &doc.currency, &doc.date)?,
    };
    let mut stmt = c.prepare("SELECT role,account FROM gl_accounts ORDER BY role")?;
    let roles = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut stmt = c.prepare(
        "SELECT target,rate,provider,rate_date FROM fx_rates WHERE source=? ORDER BY target",
    )?;
    let rates = stmt
        .query_map([&doc.currency], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Review{gross,recoverable,signature:serde_json::json!({"input":format!("{doc:?}"),"old":format!("{old:?}"),"tax":taxes,"rate":rate,"roles":roles,"rates":rates}).to_string()})
}
pub fn save(c: &mut Connection, doc: &Document) -> Result<i64> {
    save_checked(c, doc, None)
}
pub fn save_reviewed(c: &mut Connection, doc: &Document, signature: &str) -> Result<i64> {
    save_checked(c, doc, Some(signature))
}
fn save_checked(c: &mut Connection, doc: &Document, expected: Option<&str>) -> Result<i64> {
    ensure!(!doc.lines.is_empty(), "请至少填写一行费用");
    ensure!(
        doc.vendor.is_some_and(|id| id > 0),
        "Vendor 为必填项，请选择供应商"
    );
    let currency = crate::currency(&doc.currency).map_err(anyhow::Error::msg)?;
    crate::task_date(&doc.date).map_err(anyhow::Error::msg)?;
    if let Some(date) = &doc.due_date {
        crate::task_date(date).map_err(anyhow::Error::msg)?;
        ensure!(date >= &doc.date, "到期日不能早于单据日期");
    }
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if let Some(expected) = expected {
        ensure!(
            review(&tx, doc)?.signature == expected,
            "Bill amounts, source or exchange inputs changed. Review the bill again."
        );
    }
    if doc.id > 0 {
        crate::ap::allow_revision(&tx, doc.id)?;
    }
    let old = if doc.id > 0 {
        Some(load(&tx, doc.id)?)
    } else {
        None
    };
    crate::vendor::allow_usage(
        &tx,
        doc.vendor.unwrap(),
        old.as_ref().and_then(|doc| doc.vendor),
    )?;
    if let Some(old) = &old {
        for line in &old.lines {
            if let Some(task) = line.task {
                crate::editable(&tx, task)?;
            }
        }
    }
    let mut used = std::collections::BTreeSet::new();
    let mut total = 0i64;
    for line in &doc.lines {
        if line.id > 0 {
            ensure!(used.insert(line.id), "费用行重复");
            ensure!(
                old.as_ref()
                    .is_some_and(|old| old.lines.iter().any(|v| v.id == line.id)),
                "费用行不属于当前单据"
            );
        }
        let mut draft = line.clone();
        draft.date = doc.date.clone();
        draft.currency = currency.clone();
        let gross = crate::tax::expense_calc(&tx, &draft)?["gross"]
            .as_i64()
            .context("费用含税金额无效")?;
        total = total.checked_add(gross).context("费用合计过大")?;
    }
    let id = if doc.id == 0 {
        tx.execute("INSERT INTO expense_documents(date,vendor,currency,reference,due_date,memo,terms) VALUES(?,?,?,?,?,?,?)",params![doc.date,doc.vendor,currency,doc.reference.trim(),doc.due_date,doc.memo.trim(),doc.terms.trim()])?;
        let id = tx.last_insert_rowid();
        crate::numbering::assign(&tx, "expense", id)?;
        id
    } else {
        tx.execute("UPDATE expense_documents SET date=?,vendor=?,currency=?,reference=?,due_date=?,memo=?,terms=? WHERE id=?",params![doc.date,doc.vendor,currency,doc.reference.trim(),doc.due_date,doc.memo.trim(),doc.terms.trim(),doc.id])?;
        doc.id
    };
    for line in &doc.lines {
        let mut line = line.clone();
        line.date = doc.date.clone();
        line.vendor = doc.vendor;
        line.currency = currency.clone();
        let expense = crate::accounting::save_expense_on(&tx, &line)?;
        if line.id == 0 {
            tx.execute(
                "INSERT INTO expense_document_lines VALUES(?,?)",
                params![id, expense],
            )?;
        }
    }
    if let Some(old) = old {
        for removed in old.lines.iter().filter(|line| !used.contains(&line.id)) {
            let link: Option<i64> = tx.query_row(
                "SELECT task_expense FROM ledger_expenses WHERE id=?",
                [removed.id],
                |r| r.get(0),
            )?;
            tx.execute(
                "DELETE FROM expense_document_lines WHERE document=? AND expense=?",
                params![id, removed.id],
            )?;
            tx.execute(
                "DELETE FROM tax_postings WHERE source='expense' AND document=?",
                [removed.id],
            )?;
            tx.execute("DELETE FROM expense_tax WHERE expense=?", [removed.id])?;
            tx.execute("DELETE FROM ledger_expenses WHERE id=?", [removed.id])?;
            if let Some(link) = link {
                tx.execute("DELETE FROM expenses WHERE id=?", [link])?;
            }
        }
    }
    crate::gl::expense(&tx, id, doc.id == 0)?;
    tx.commit()?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Result<(Connection, Document)> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer',''); INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor',''); INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','item',0,10000,'CAD');")?;
        crate::add_task(&c, "Task", 1, "CAD")?;
        crate::add_item(&c, 1, 1, 1, None, None)?;
        let account = crate::accounting::default_account(&c)?;
        let line = Expense {
            tax: Default::default(),
            id: 0,
            date: String::new(),
            description: "Consulting".into(),
            account,
            vendor: None,
            currency: String::new(),
            amount: "25.00".into(),
            task: Some(1),
        };
        let mut other = line.clone();
        other.description = "Office".into();
        other.amount = "15.00".into();
        other.task = None;
        Ok((
            c,
            Document {
                id: 0,
                vendor: Some(1),
                date: "2026-10-25".into(),
                currency: "CAD".into(),
                reference: "BILL-42".into(),
                due_date: Some("2026-11-04".into()),
                memo: "Monthly bill".into(),
                terms: "Net 10".into(),
                lines: vec![line, other],
            },
        ))
    }
    #[test]
    fn reviewed_bill_is_read_only_and_rejects_changed_inputs_atomically() -> Result<()> {
        let (mut c, mut doc) = fixture()?;
        let changes = c.total_changes();
        let review = review(&c, &doc)?;
        assert_eq!(review.gross, 4000);
        assert_eq!(changes, c.total_changes());
        doc.lines[0].amount = "26.00".into();
        assert!(save_reviewed(&mut c, &doc, &review.signature).is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM expense_documents", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        let review = super::review(&c, &doc)?;
        // A changed posting role invalidates a previously displayed review.
        c.execute("UPDATE gl_accounts SET account=(SELECT id FROM accounts WHERE code='6400') WHERE role='fx'",[])?;
        assert!(save_reviewed(&mut c, &doc, &review.signature).is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM ledger_expenses", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        let fresh = super::review(&c, &doc)?;
        let id = save_reviewed(&mut c, &doc, &fresh.signature)?;
        assert_eq!(
            c.query_row(
                "SELECT number FROM expense_documents WHERE id=?",
                [id],
                |r| r.get::<_, String>(0)
            )?,
            "EXP0001"
        );
        crate::storage::validate_projections(&c)?;
        Ok(())
    }
    #[test]
    fn reviewed_bill_still_checks_locked_period_and_rolls_back_numbering() -> Result<()> {
        let (mut c, doc) = fixture()?;
        let review = review(&c, &doc)?;
        c.execute(
            "UPDATE gl_settings SET locked_through=? WHERE id=1",
            [&doc.date],
        )?;
        assert!(save_reviewed(&mut c, &doc, &review.signature).is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM expense_documents", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        c.execute("UPDATE gl_settings SET locked_through='' WHERE id=1", [])?;
        let id = save_reviewed(&mut c, &doc, &review.signature)?;
        assert_eq!(
            c.query_row(
                "SELECT number FROM expense_documents WHERE id=?",
                [id],
                |r| r.get::<_, String>(0)
            )?,
            "EXP0001"
        );
        Ok(())
    }
    #[test]
    fn multiline_expense_is_atomic_and_has_one_number() -> Result<()> {
        let (mut c, mut doc) = fixture()?;
        doc.lines[1].account = 999;
        assert!(save(&mut c, &doc).is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM ledger_expenses", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        doc.lines[1].account = crate::accounting::default_account(&c)?;
        let id = save(&mut c, &doc)?;
        assert_eq!(
            c.query_row(
                "SELECT number FROM expense_documents WHERE id=?",
                [id],
                |r| r.get::<_, String>(0)
            )?,
            "EXP0001"
        );
        let mut saved = load(&c, id)?;
        assert_eq!(saved.lines.len(), 2);
        assert_eq!(saved.reference, "BILL-42");
        assert_eq!(saved.memo, "Monthly bill");
        assert_eq!(crate::snapshot(&c, 1)?["cost"], 2500);
        saved.lines.remove(0);
        saved.lines[0].amount = "20.00".into();
        save(&mut c, &saved)?;
        assert_eq!(crate::snapshot(&c, 1)?["cost"], 0);
        assert_eq!(load(&c, id)?.lines.len(), 1);
        crate::storage::validate_projections(&c)?;
        let next = save(&mut c, &doc)?;
        assert_eq!(
            c.query_row(
                "SELECT number FROM expense_documents WHERE id=?",
                [next],
                |r| r.get::<_, String>(0)
            )?,
            "EXP0002"
        );
        Ok(())
    }
    #[test]
    fn locked_task_prevents_header_and_line_changes() -> Result<()> {
        let (mut c, doc) = fixture()?;
        let id = save(&mut c, &doc)?;
        crate::set_task_status(&mut c, 1, false)?;
        crate::billing::merge(&mut c, &[1])?;
        let mut edit = load(&c, id)?;
        edit.reference = "Changed".into();
        edit.lines.remove(0);
        assert!(save(&mut c, &edit).is_err());
        assert_eq!(load(&c, id)?.reference, "BILL-42");
        assert_eq!(load(&c, id)?.lines.len(), 2);
        Ok(())
    }
}
