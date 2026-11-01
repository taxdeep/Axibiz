//! Reviewed customer receipts with atomic idempotency; legacy receipts are not rewritten.
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Draft {
    pub request: String,
    pub invoice: i64,
    pub date: String,
    pub amount: i64,
    pub method: String,
    pub reference: String,
    pub currency: String,
    pub bank: i64,
    pub invoice_entry: i64,
    pub remaining: i64,
    pub quote: crate::fx::Quote,
}
pub fn init(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS receipt_requests(id INTEGER PRIMARY KEY,request_key TEXT NOT NULL UNIQUE,payment INTEGER NOT NULL UNIQUE REFERENCES payments(id),payload TEXT NOT NULL CHECK(json_valid(payload))); CREATE TRIGGER IF NOT EXISTS receipt_request_fixed BEFORE UPDATE ON receipt_requests BEGIN SELECT RAISE(ABORT,'Receipt requests are fixed'); END; CREATE TRIGGER IF NOT EXISTS receipt_request_no_delete BEFORE DELETE ON receipt_requests BEGIN SELECT RAISE(ABORT,'Receipt requests are fixed'); END;")?;
    Ok(())
}
pub fn latest(c: &Connection, invoice: i64) -> Result<String> {
    Ok(c.query_row("SELECT COALESCE(max(date),'') FROM (SELECT date FROM payments WHERE invoice=?1 UNION ALL SELECT voided_on FROM payments WHERE invoice=?1)",[invoice],|r|r.get(0))?)
}
pub fn prepare(
    c: &Connection,
    request: &str,
    invoice: i64,
    date: &str,
    amount: &str,
    method: &str,
    reference: &str,
) -> Result<Draft> {
    let balance = crate::billing::balance(c, invoice)?;
    Ok(Draft {
        request: request.into(),
        invoice,
        date: date.into(),
        amount: crate::parse_money(amount)?,
        method: method.trim().into(),
        reference: reference.trim().into(),
        currency: balance.currency.clone(),
        bank: crate::gl::role(c, "bank")?,
        invoice_entry: c
            .query_row(
                "SELECT entry FROM gl_sources WHERE kind='invoice' AND record=?",
                [invoice],
                |r| r.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten()
            .context("This invoice needs historical A/R source reconciliation.")?,
        remaining: balance.remaining,
        quote: crate::fx::posting_quote(c, &balance.currency, date)?,
    })
}
pub fn validate_draft(c: &Connection, d: &Draft) -> Result<()> {
    ensure!(
        d.request.len() == 32 && d.request.bytes().all(|c| c.is_ascii_hexdigit()),
        "Invalid receipt request key."
    );
    let issue:String=c.query_row("SELECT COALESCE(json_extract(snapshot,'$.invoice_date'),substr(created,1,10)) FROM invoices WHERE id=?",[d.invoice],|r|r.get(0))?;
    crate::posting::settlement_date(c, &d.date, &issue, &latest(c, d.invoice)?)?;
    ensure!(!c.query_row("SELECT EXISTS(SELECT 1 FROM payments WHERE invoice=? AND (date IS NULL OR (voided=1 AND voided_on IS NULL)))",[d.invoice],|r|r.get::<_,bool>(0))?,"Historical receipt dates are incomplete. Reconcile the source records first.");
    ensure!(!d.method.trim().is_empty(), "Payment method is required.");
    let balance = crate::billing::balance(c, d.invoice)?;
    ensure!(
        balance.currency == d.currency && balance.remaining == d.remaining,
        "The invoice balance changed. Review the receipt again."
    );
    ensure!(
        d.amount > 0 && d.amount <= balance.remaining,
        "Receipt must be positive and cannot exceed the invoice balance."
    );
    ensure!(
        crate::fx::posting_quote(c, &d.currency, &d.date)? == d.quote,
        "The exchange quote changed. Review the receipt again."
    );
    ensure!(
        crate::gl::role(c, "bank")? == d.bank,
        "The deposit account changed. Review the receipt again."
    );
    crate::posting::bank(c, d.bank, &d.currency, false)?;
    let source_date: String = c.query_row(
        "SELECT date FROM gl_entries WHERE id=?",
        [d.invoice_entry],
        |r| r.get(0),
    )?;
    ensure!(
        d.date >= source_date,
        "Receipt date cannot precede its receivable posting."
    );

    ensure!(c.query_row("SELECT EXISTS(SELECT 1 FROM gl_sources s JOIN gl_entries e ON e.id=s.entry WHERE s.kind='invoice' AND s.record=?1 AND e.id=?2 AND e.status='posted' AND NOT EXISTS(SELECT 1 FROM gl_reversals r WHERE r.entry=e.id))",params![d.invoice,d.invoice_entry],|r|r.get::<_,bool>(0))?,"This invoice has no active receivable posting. Reconcile its historical source before recording a receipt.");
    Ok(())
}
pub fn record(c: &mut Connection, d: &Draft) -> Result<i64> {
    let payload = serde_json::to_string(d)?;
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if let Some((id, old)) = tx
        .query_row(
            "SELECT payment,payload FROM receipt_requests WHERE request_key=?",
            [&d.request],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?
    {
        ensure!(
            payload == old,
            "Receipt request already used with different input."
        );
        return Ok(id);
    }
    validate_draft(&tx, d)?;
    tx.execute(
        "INSERT INTO payments(invoice,date,amount,method,reference) VALUES(?,?,?,?,?)",
        params![d.invoice, d.date, d.amount, d.method, d.reference],
    )?;
    let id = tx.last_insert_rowid();
    crate::numbering::assign(&tx, "payment", id)?;
    tx.execute(
        "UPDATE invoices SET paid=? WHERE id=?",
        params![d.amount == d.remaining, d.invoice],
    )?;
    crate::gl::payment(&tx, id)?;
    tx.execute(
        "INSERT INTO receipt_requests(request_key,payment,payload) VALUES(?,?,?)",
        params![d.request, id, payload],
    )?;
    tx.commit()?;
    Ok(id)
}
pub fn validate(c: &Connection) -> Result<()> {
    let mut stmt = c.prepare("SELECT request_key,payment,payload FROM receipt_requests")?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (key, id, raw) = row?;
        let d: Draft = serde_json::from_str(&raw)?;
        let actual: (i64, String, i64, String, String) = c.query_row(
            "SELECT invoice,date,amount,method,reference FROM payments WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;
        ensure!(
            key == d.request
                && actual
                    == (
                        d.invoice,
                        d.date.clone(),
                        d.amount,
                        d.method.clone(),
                        d.reference.clone()
                    ),
            "Receipt request and saved payment differ."
        );
        let entry: i64 = c.query_row(
            "SELECT entry FROM gl_sources WHERE kind='payment' AND record=?",
            [id],
            |r| r.get(0),
        )?;
        let j = crate::gl::load(c, entry)?;
        let bank = j.lines.first().context("Missing receipt bank line")?;
        ensure!(
            j.currency == d.currency
                && j.rate == d.quote.rate
                && bank.account == d.bank
                && bank.debit == d.amount,
            "Receipt posting differs from its reviewed source."
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Result<(Connection, i64)> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Buyer','')",
            [],
        )?;
        let d = crate::billing::DirectInvoice {
            request_key: "receipt-fixture".into(),
            customer: 1,
            date: "2026-10-01".into(),
            currency: "CAD".into(),
            title: "Service".into(),
            lines: vec![crate::billing::DirectLine {
                price_id: 0,
                description: "Service".into(),
                unit: "item".into(),
                quantity: 1,
                unit_price: Some(10000),
                tax_code: Some(0),
            }],
        };
        let preview = crate::billing::direct_preview(&c, &d)?;
        let id = crate::billing::direct_confirm(&mut c, &d, &preview)?;
        Ok((c, id))
    }
    fn draft(c: &Connection, id: i64) -> Result<Draft> {
        prepare(
            c,
            &crate::posting::request_id(c)?,
            id,
            "2026-10-10",
            "25",
            "Cash",
            "REF-1",
        )
    }
    fn counts(c: &Connection) -> Result<(i64, i64, i64, i64, i64)> {
        Ok(c.query_row("SELECT (SELECT count(*) FROM payments),(SELECT count(*) FROM receipt_requests),(SELECT count(*) FROM gl_entries),(SELECT count(*) FROM audit_log),(SELECT next_value FROM number_sequences WHERE kind='payment')", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?)
    }
    #[test]
    fn reviewed_receipt_retries_are_idempotent_and_sources_are_immutable() -> Result<()> {
        let (mut c, id) = fixture()?;
        let d = draft(&c, id)?;
        let payment = record(&mut c, &d)?;
        let after = counts(&c)?;
        assert_eq!(record(&mut c, &d)?, payment);
        assert_eq!(counts(&c)?, after);
        let mut changed = d.clone();
        changed.amount += 1;
        assert!(record(&mut c, &changed).is_err());
        assert_eq!(counts(&c)?, after);
        assert!(c
            .execute("UPDATE receipt_requests SET payload='{}'", [])
            .is_err());
        assert!(c.execute("DELETE FROM receipt_requests", []).is_err());
        validate(&c)?;
        crate::billing::void_payment_on(&mut c, payment, "Correction", "2026-10-15")?;
        assert_eq!(record(&mut c, &d)?, payment);
        assert_eq!(crate::billing::balance(&c, id)?.remaining, 10000);
        validate(&c)?;
        Ok(())
    }
    #[test]
    fn stale_review_or_changed_bank_never_records_a_receipt() -> Result<()> {
        let (mut c, id) = fixture()?;
        let stale = draft(&c, id)?;
        let first = draft(&c, id)?;
        record(&mut c, &first)?;
        let before = counts(&c)?;
        assert!(record(&mut c, &stale).is_err());
        assert_eq!(counts(&c)?, before);
        let mut d = draft(&c, id)?;
        d.quote.rate += 1;
        assert!(record(&mut c, &d).is_err());
        let mut d = draft(&c, id)?;
        d.invoice_entry += 999;
        assert!(record(&mut c, &d).is_err());
        let d = draft(&c, id)?;
        let other: i64 = c.query_row("SELECT id FROM accounts WHERE code='1000'", [], |r| {
            r.get(0)
        })?;
        c.execute(
            "UPDATE gl_accounts SET account=? WHERE role='bank'",
            [other],
        )?;
        assert!(record(&mut c, &d).is_err());
        assert_eq!(crate::billing::balance(&c, id)?.received, 2500);
        Ok(())
    }
    #[test]
    fn chronology_period_and_bank_controls_are_enforced_before_writes() -> Result<()> {
        let (mut c, id) = fixture()?;
        let before = counts(&c)?;
        for date in [
            "2026-09-30".to_string(),
            (crate::today() + chrono::Duration::days(1)).to_string(),
        ] {
            let mut d = draft(&c, id)?;
            d.date = date;
            assert!(record(&mut c, &d).is_err());
        }
        c.execute("UPDATE gl_settings SET locked_through='2026-10-10'", [])?;
        let d = draft(&c, id)?;
        assert!(record(&mut c, &d).is_err());
        c.execute("UPDATE gl_settings SET locked_through=''", [])?;
        let ar = crate::gl::role(&c, "receivable")?;
        assert!(crate::posting::bank(&c, ar, "CAD", false).is_err());
        let bank = crate::gl::role(&c, "bank")?;
        crate::ap::configure_bank(&mut c, bank, "USD")?;
        let d = draft(&c, id)?;
        assert!(record(&mut c, &d).is_err());
        assert_eq!(counts(&c)?.0, before.0);
        c.execute("UPDATE ap_banks SET currency='CAD' WHERE id=?", [bank])?;
        let d = draft(&c, id)?;
        let p = record(&mut c, &d)?;
        crate::billing::void_payment_on(&mut c, p, "Correction", "2026-10-15")?;
        let d = draft(&c, id)?;
        assert!(record(&mut c, &d).is_err());
        assert_eq!(crate::billing::balance(&c, id)?.received, 0);
        Ok(())
    }
    #[test]
    fn posting_failure_rolls_back_payment_number_journal_and_audit() -> Result<()> {
        let (mut c, id) = fixture()?;
        let d = draft(&c, id)?;
        let before = counts(&c)?;
        c.execute_batch("CREATE TRIGGER fail_receipt BEFORE INSERT ON receipt_requests BEGIN SELECT RAISE(ABORT,'injected failure'); END;")?;
        assert!(record(&mut c, &d).is_err());
        assert_eq!(counts(&c)?, before);
        assert_eq!(crate::billing::balance(&c, id)?.received, 0);
        c.execute_batch("DROP TRIGGER fail_receipt;")?;
        record(&mut c, &d)?;
        crate::storage::validate_projections(&c)?;
        Ok(())
    }
}
