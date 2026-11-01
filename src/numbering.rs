use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub kind: String,
    pub prefix: String,
    pub width: usize,
    pub next: i64,
}
pub const KINDS: [(&str, &str); 6] = [
    ("task", "TD"),
    ("invoice", "INV"),
    ("expense", "EXP"),
    ("payment", "PAY"),
    ("journal", "JE"),
    ("vendor_payment", "VP"),
];
fn table(kind: &str) -> Result<&'static str> {
    match kind {
        "task" => Ok("tasks"),
        "invoice" => Ok("invoices"),
        "expense" => Ok("expense_documents"),
        "payment" => Ok("payments"),
        "journal" => Ok("gl_entries"),
        "vendor_payment" => Ok("vendor_payments"),
        _ => anyhow::bail!("编号类型无效"),
    }
}
pub fn validate(rule: &Rule) -> Result<()> {
    table(&rule.kind)?;
    ensure!((1..=12).contains(&rule.width), "编号位数必须为 1 到 12");
    ensure!(rule.next > 0 && rule.next < i64::MAX, "起始序号无效");
    ensure!(
        !rule.prefix.is_empty()
            && rule.prefix.chars().count() <= 24
            && rule
                .prefix
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_'),
        "前缀请使用 1–24 个字母、数字、中文、连字符或下划线"
    );
    Ok(())
}
pub fn rules(c: &Connection) -> Result<Vec<Rule>> {
    let mut stmt=c.prepare("SELECT kind,prefix,width,next_value FROM number_sequences ORDER BY CASE kind WHEN 'task' THEN 0 WHEN 'invoice' THEN 1 WHEN 'expense' THEN 2 WHEN 'payment' THEN 3 ELSE 4 END")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Rule {
                kind: r.get(0)?,
                prefix: r.get(1)?,
                width: r.get::<_, i64>(2)? as usize,
                next: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
pub fn configure(c: &mut Connection, rules: &[Rule]) -> Result<()> {
    for rule in rules {
        validate(rule)?;
    }
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    for rule in rules {
        tx.execute(
            "UPDATE number_sequences SET prefix=?,width=?,next_value=? WHERE kind=?",
            params![rule.prefix, rule.width as i64, rule.next, rule.kind],
        )?;
    }
    tx.commit()?;
    Ok(())
}
pub fn allocate(c: &Connection, kind: &str) -> Result<String> {
    let table = table(kind)?;
    let (prefix, width, mut next): (String, usize, i64) = c.query_row(
        "SELECT prefix,width,next_value FROM number_sequences WHERE kind=?",
        [kind],
        |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize, r.get(2)?)),
    )?;
    loop {
        let number = format!("{prefix}{next:0width$}");
        next = next.checked_add(1).context("编号序列已用尽")?;
        let used: bool = c.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE number=?)"),
            [&number],
            |r| r.get(0),
        )?;
        if !used {
            c.execute(
                "UPDATE number_sequences SET next_value=? WHERE kind=?",
                params![next, kind],
            )?;
            return Ok(number);
        }
    }
}
pub fn assign(c: &Connection, kind: &str, id: i64) -> Result<String> {
    let number = allocate(c, kind)?;
    c.execute(
        &format!("UPDATE {} SET number=? WHERE id=?", table(kind)?),
        params![number, id],
    )?;
    Ok(number)
}
pub fn init(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS number_sequences(kind TEXT PRIMARY KEY,prefix TEXT NOT NULL,width INTEGER NOT NULL,next_value INTEGER NOT NULL);")?;
    for (kind, prefix) in KINDS {
        c.execute(
            "INSERT OR IGNORE INTO number_sequences VALUES(?,?,4,1)",
            params![kind, prefix],
        )?;
        let rule = c.query_row(
            "SELECT prefix,width,next_value FROM number_sequences WHERE kind=?",
            [kind],
            |r| {
                Ok(Rule {
                    kind: kind.into(),
                    prefix: r.get(0)?,
                    width: r.get::<_, i64>(1)? as usize,
                    next: r.get(2)?,
                })
            },
        )?;
        validate(&rule)?;
        let table = table(kind)?;
        let exists: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?) WHERE name='number')",
            [table],
            |r| r.get(0),
        )?;
        if !exists {
            c.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN number TEXT;"))?;
        }
        c.execute_batch(&format!(
            "CREATE UNIQUE INDEX IF NOT EXISTS {table}_number_idx ON {table}(number);"
        ))?;
        let ids = {
            let mut stmt = c.prepare(&format!(
                "SELECT id FROM {table} WHERE number IS NULL ORDER BY id"
            ))?;
            let result = stmt
                .query_map([], |r| r.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            result
        };
        for id in ids {
            assign(c, kind, id)?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configurable_numbers_skip_existing_and_rollback() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Customer','')",
            [],
        )?;
        let a = crate::add_task(&c, "A", 1, "CAD")?;
        assert_eq!(crate::snapshot(&c, a)?["number"], "TD0001");
        configure(
            &mut c,
            &[Rule {
                kind: "task".into(),
                prefix: "TD".into(),
                width: 4,
                next: 1,
            }],
        )?;
        {
            let tx = c.transaction()?;
            crate::add_task(&tx, "Rolled back", 1, "CAD")?;
        }
        let b = crate::add_task(&c, "B", 1, "CAD")?;
        assert_eq!(crate::snapshot(&c, b)?["number"], "TD0002");
        configure(
            &mut c,
            &[Rule {
                kind: "task".into(),
                prefix: "JOB-".into(),
                width: 5,
                next: 20,
            }],
        )?;
        let d = crate::add_task(&c, "D", 1, "CAD")?;
        assert_eq!(crate::snapshot(&c, d)?["number"], "JOB-00020");
        Ok(())
    }
}
