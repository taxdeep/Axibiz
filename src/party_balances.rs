//! Canonical batch read models. Vendor recorded costs are deliberately not unpaid A/P.
use anyhow::Result;
use rusqlite::Connection;
use std::collections::BTreeMap;
pub type Totals = BTreeMap<i64, Vec<(String, i64)>>;
fn read(c: &Connection, sql: &str) -> Result<Totals> {
    let mut stmt = c.prepare(sql)?;
    let mut totals = Totals::new();
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })? {
        let (id, currency, amount) = row?;
        totals.entry(id).or_default().push((currency, amount));
    }
    Ok(totals)
}
pub fn customers(c: &Connection) -> Result<Totals> {
    read(c,"SELECT h.customer_id,h.currency,sum(h.total-COALESCE(p.received,0)) FROM invoice_headers h LEFT JOIN (SELECT invoice,sum(amount) AS received FROM payments WHERE voided=0 GROUP BY invoice) p ON p.invoice=h.invoice WHERE h.customer_id IS NOT NULL GROUP BY h.customer_id,h.currency")
}
pub fn vendors(c: &Connection) -> Result<Totals> {
    read(c,"SELECT d.vendor,d.currency,sum(COALESCE(t.gross,e.amount)) FROM expense_documents d JOIN expense_document_lines x ON x.document=d.id JOIN ledger_expenses e ON e.id=x.expense LEFT JOIN expense_tax t ON t.expense=e.id WHERE d.vendor IS NOT NULL GROUP BY d.vendor,d.currency")
}
