//! Common settlement controls. These are accounting safeguards, not tax eligibility decisions.
use anyhow::{ensure, Result};
use rusqlite::{params, Connection, OptionalExtension};
pub fn request_id(c: &Connection) -> Result<String> {
    Ok(c.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))?)
}
pub fn settlement_date(c: &Connection, date: &str, earliest: &str, latest: &str) -> Result<()> {
    crate::task_date(date).map_err(anyhow::Error::msg)?;
    ensure!(
        date <= crate::today().to_string().as_str(),
        "Settlement date cannot be in the future."
    );
    ensure!(
        date >= earliest,
        "Settlement date cannot be before the source transaction."
    );
    ensure!(
        date >= latest,
        "Settlement date cannot precede a later payment or reversal. Review its chronology."
    );
    let locked: String = c.query_row(
        "SELECT locked_through FROM gl_settings WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    ensure!(
        date > locked.as_str(),
        "The settlement date is in a locked period."
    );
    Ok(())
}
pub fn bank(c: &Connection, account: i64, currency: &str, require_configured: bool) -> Result<()> {
    crate::currency(currency).map_err(anyhow::Error::msg)?;
    bank_identity(c, account)?;
    let registered: Option<String> = c
        .query_row("SELECT currency FROM ap_banks WHERE id=?", [account], |r| {
            r.get(0)
        })
        .optional()?;
    ensure!(
        (!require_configured && registered.is_none() && currency == crate::gl::base(c)?) || registered.as_deref() == Some(currency),
        "Configure a dedicated bank account for this transaction currency. An unconfigured deposit role accepts only the GL base currency."
    );
    ensure!(!c.query_row("SELECT EXISTS(SELECT 1 FROM gl_lines l JOIN gl_entries e ON e.id=l.entry WHERE l.account=?1 AND e.status='posted' AND (l.debit>0 OR l.credit>0) AND e.currency!=?2)",params![account,currency],|r|r.get::<_,bool>(0))?,"This bank has postings in another currency. Use a dedicated matching-currency account.");
    Ok(())
}

pub fn bank_identity(c: &Connection, account: i64) -> Result<()> {
    ensure!(
        c.query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=? AND active=1 AND kind='Asset')",
            [account],
            |r| r.get::<_, bool>(0)
        )?,
        "Select an active Asset bank/cash account."
    );
    ensure!(!c.query_row("SELECT EXISTS(SELECT 1 FROM gl_accounts WHERE account=?1 AND role!='bank') OR EXISTS(SELECT 1 FROM tax_agencies WHERE recoverable_account=?1 OR payable_account=?1)",[account],|r|r.get::<_,bool>(0))?,"A bank/cash account cannot also be a receivable, tax or other control account.");
    Ok(())
}
