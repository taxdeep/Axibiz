//! Supplier settlements. All amounts are integer minor units; source journals stay immutable.
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct Bill {
    pub source: i64,
    pub document: i64,
    pub entry: i64,
    pub number: String,
    pub date: String,
    pub due: String,
    pub reference: String,
    pub account: i64,
    pub gross: i64,
    pub base: i64,
    pub remaining: i64,
    pub base_remaining: i64,
    pub verified: bool,
}
pub use crate::fx::posting_quote as quote;
pub use crate::fx::Quote;
pub use crate::posting::request_id;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Allocation {
    pub source: i64,
    pub amount: i64,
    pub remaining: i64,
    pub base_remaining: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Draft {
    pub request: String,
    pub vendor: i64,
    pub currency: String,
    pub date: String,
    pub bank: i64,
    pub method: String,
    pub reference: String,
    pub memo: String,
    pub quote: Quote,
    pub allocations: Vec<Allocation>,
}
pub struct Preview {
    pub amount: i64,
    pub released: i64,
    pub bank_base: i64,
    pub fx: i64,
    pub journal: crate::gl::Journal,
    allocations: Vec<(i64, i64, i64)>,
}
#[derive(Debug)]
pub struct Reconciliation {
    pub open: i64,
    pub base_open: i64,
    pub ledger: i64,
    pub base_ledger: i64,
    pub unverified: usize,
    pub unassigned: i64,
    pub missing: usize,
}
impl Reconciliation {
    pub fn balanced(&self) -> bool {
        self.open == self.ledger
            && self.base_open == self.base_ledger
            && self.unassigned == 0
            && self.missing == 0
    }
    pub fn ready(&self) -> bool {
        self.balanced() && self.unverified == 0
    }
}

pub fn init(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
        CREATE TABLE IF NOT EXISTS ap_sources(id INTEGER PRIMARY KEY,document INTEGER NOT NULL REFERENCES expense_documents(id),entry INTEGER NOT NULL UNIQUE REFERENCES gl_entries(id),vendor INTEGER NOT NULL REFERENCES vendors(id),currency TEXT NOT NULL,date TEXT NOT NULL,account INTEGER NOT NULL REFERENCES accounts(id),gross INTEGER NOT NULL CHECK(typeof(gross)='integer' AND gross>0),base INTEGER NOT NULL CHECK(typeof(base)='integer' AND base>=0));
        CREATE INDEX IF NOT EXISTS ap_sources_vendor ON ap_sources(vendor,currency,date);
        CREATE TABLE IF NOT EXISTS ap_reviews(id INTEGER PRIMARY KEY,source INTEGER NOT NULL UNIQUE REFERENCES ap_sources(id),cutoff TEXT NOT NULL,evidence TEXT NOT NULL CHECK(length(trim(evidence))>0),created TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        CREATE TABLE IF NOT EXISTS ap_banks(id INTEGER PRIMARY KEY REFERENCES accounts(id),currency TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS vendor_payments(id INTEGER PRIMARY KEY,number TEXT NOT NULL UNIQUE,request TEXT NOT NULL UNIQUE,payload TEXT NOT NULL CHECK(json_valid(payload)),vendor INTEGER NOT NULL REFERENCES vendors(id),currency TEXT NOT NULL,date TEXT NOT NULL,bank INTEGER NOT NULL REFERENCES ap_banks(id),method TEXT NOT NULL,reference TEXT NOT NULL,memo TEXT NOT NULL,amount INTEGER NOT NULL CHECK(typeof(amount)='integer' AND amount>0),bank_base INTEGER NOT NULL CHECK(typeof(bank_base)='integer' AND bank_base>=0),quote TEXT NOT NULL CHECK(json_valid(quote)),created TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        CREATE INDEX IF NOT EXISTS vendor_payments_scope ON vendor_payments(vendor,currency,date);
        CREATE TABLE IF NOT EXISTS ap_allocations(id INTEGER PRIMARY KEY,payment INTEGER NOT NULL REFERENCES vendor_payments(id),source INTEGER NOT NULL REFERENCES ap_sources(id),amount INTEGER NOT NULL CHECK(typeof(amount)='integer' AND amount>0),base INTEGER NOT NULL CHECK(typeof(base)='integer' AND base>=0),UNIQUE(payment,source));
        CREATE INDEX IF NOT EXISTS ap_allocations_source ON ap_allocations(source);
        CREATE TABLE IF NOT EXISTS ap_voids(id INTEGER PRIMARY KEY,payment INTEGER NOT NULL UNIQUE REFERENCES vendor_payments(id),date TEXT NOT NULL,reason TEXT NOT NULL CHECK(length(trim(reason))>0),reversal INTEGER NOT NULL UNIQUE REFERENCES gl_entries(id),created TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        CREATE TRIGGER IF NOT EXISTS ap_bank_fixed BEFORE UPDATE ON ap_banks WHEN EXISTS(SELECT 1 FROM vendor_payments WHERE bank=OLD.id) BEGIN SELECT RAISE(ABORT,'A used bank currency is fixed'); END;
        CREATE TRIGGER IF NOT EXISTS ap_bank_asset BEFORE INSERT ON ap_banks WHEN NOT EXISTS(SELECT 1 FROM accounts WHERE id=NEW.id AND kind='Asset' AND active=1) BEGIN SELECT RAISE(ABORT,'Select an active Asset account'); END;
        CREATE TRIGGER IF NOT EXISTS ap_allocation_scope BEFORE INSERT ON ap_allocations WHEN NOT EXISTS(SELECT 1 FROM vendor_payments p JOIN ap_sources s ON s.id=NEW.source JOIN ap_reviews r ON r.source=s.id WHERE p.id=NEW.payment AND p.vendor=s.vendor AND p.currency=s.currency AND p.date>=s.date) BEGIN SELECT RAISE(ABORT,'Allocation vendor, currency, date or review mismatch'); END;
        CREATE TRIGGER IF NOT EXISTS ap_bill_fixed BEFORE UPDATE ON expense_documents WHEN EXISTS(SELECT 1 FROM ap_sources s JOIN ap_allocations a ON a.source=s.id WHERE s.document=OLD.id) BEGIN SELECT RAISE(ABORT,'Bills with payment history are fixed'); END;
        CREATE TRIGGER IF NOT EXISTS ap_expense_fixed BEFORE UPDATE ON ledger_expenses WHEN EXISTS(SELECT 1 FROM expense_document_lines x JOIN ap_sources s ON s.document=x.document JOIN ap_allocations a ON a.source=s.id WHERE x.expense=OLD.id) BEGIN SELECT RAISE(ABORT,'Bill splits with payment history are fixed'); END;
        CREATE TRIGGER IF NOT EXISTS ap_source_fixed BEFORE UPDATE ON gl_sources WHEN OLD.kind='expense' AND EXISTS(SELECT 1 FROM ap_sources s JOIN ap_allocations a ON a.source=s.id WHERE s.entry=OLD.entry) BEGIN SELECT RAISE(ABORT,'Allocated bill sources are fixed'); END;
    "#)?;
    for table in [
        "ap_sources",
        "ap_reviews",
        "vendor_payments",
        "ap_allocations",
        "ap_voids",
    ] {
        for operation in ["UPDATE", "DELETE"] {
            c.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS {table}_fixed_{operation} BEFORE {operation} ON {table} BEGIN SELECT RAISE(ABORT,'A/P records are immutable; use a dated reversal'); END;"))?;
        }
    }
    let documents = {
        let mut s=c.prepare("SELECT record FROM gl_sources WHERE kind='expense' AND entry IS NOT NULL ORDER BY record")?;
        let rows = s
            .query_map([], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for id in documents {
        register(c, id, false)?;
    }
    Ok(())
}

pub fn register(c: &Connection, document: i64, is_new: bool) -> Result<()> {
    let source: Option<i64> = c
        .query_row(
            "SELECT entry FROM gl_sources WHERE kind='expense' AND record=?",
            [document],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let Some(entry) = source else { return Ok(()) };
    if c.query_row(
        "SELECT EXISTS(SELECT 1 FROM ap_sources WHERE entry=?)",
        [entry],
        |r| r.get::<_, bool>(0),
    )? {
        return Ok(());
    }
    let j = crate::gl::load(c, entry)?;
    let vendor: Option<i64> = c.query_row(
        "SELECT vendor FROM expense_documents WHERE id=?",
        [document],
        |r| r.get(0),
    )?;
    let Some(vendor) = vendor else { return Ok(()) };
    let Some(line) = j
        .lines
        .last()
        .filter(|l| l.vendor == Some(vendor) && l.credit > 0 && l.debit == 0)
    else {
        return Ok(());
    };
    let liability: bool = c.query_row(
        "SELECT kind='Liability' FROM accounts WHERE id=?",
        [line.account],
        |r| r.get(0),
    )?;
    let gross:i64=c.query_row("SELECT COALESCE(sum(COALESCE(t.gross,e.amount)),0) FROM expense_document_lines x JOIN ledger_expenses e ON e.id=x.expense LEFT JOIN expense_tax t ON t.expense=e.id WHERE x.document=?",[document],|r|r.get(0))?;
    if !liability || gross != line.credit {
        return Ok(());
    }
    let verified=is_new || c.query_row("SELECT EXISTS(SELECT 1 FROM ap_sources s JOIN ap_reviews r ON r.source=s.id WHERE s.document=?)",[document],|r|r.get::<_,bool>(0))?;
    c.execute("INSERT INTO ap_sources(document,entry,vendor,currency,date,account,gross,base) VALUES(?,?,?,?,?,?,?,?)",params![document,entry,vendor,j.currency,j.date,line.account,line.credit,line.base_credit])?;
    let id = c.last_insert_rowid();
    if verified {
        c.execute("INSERT INTO ap_reviews(source,cutoff,evidence) VALUES(?,?,'Validated bill source recorded under supplier A/P tracking')",params![id,j.date])?;
    }
    Ok(())
}
pub fn allow_revision(c: &Connection, document: i64) -> Result<()> {
    ensure!(!c.query_row("SELECT EXISTS(SELECT 1 FROM ap_sources s JOIN ap_allocations a ON a.source=s.id WHERE s.document=?)",[document],|r|r.get::<_,bool>(0))?,"This bill has payment history. Its financial details are fixed; use a separate corrective document.");
    Ok(())
}
pub fn configure_bank(c: &mut Connection, account: i64, currency: &str) -> Result<()> {
    crate::currency(currency).map_err(anyhow::Error::msg)?;
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    ensure!(
        tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=? AND kind='Asset' AND active=1)",
            [account],
            |r| r.get::<_, bool>(0)
        )?,
        "Select an active Asset bank/cash account."
    );
    crate::posting::bank_identity(&tx, account)?;
    let old: Option<String> = tx
        .query_row("SELECT currency FROM ap_banks WHERE id=?", [account], |r| {
            r.get(0)
        })
        .optional()?;
    if old.as_deref() == Some(currency) {
        return Ok(());
    }
    ensure!(!tx.query_row("SELECT EXISTS(SELECT 1 FROM gl_lines l JOIN gl_entries e ON e.id=l.entry WHERE l.account=? AND e.status='posted' AND (l.debit>0 OR l.credit>0) AND e.currency!=?)",params![account,currency],|r|r.get::<_,bool>(0))?,"This account has postings in another currency. Choose a dedicated bank account.");
    tx.execute("INSERT INTO ap_banks(id,currency) VALUES(?,?) ON CONFLICT(id) DO UPDATE SET currency=excluded.currency",params![account,currency])?;
    tx.commit()?;
    Ok(())
}
pub fn bills(c: &Connection, vendor: i64, currency: &str, date: &str) -> Result<Vec<Bill>> {
    crate::task_date(date).map_err(anyhow::Error::msg)?;
    let mut s=c.prepare("SELECT s.id,s.document,s.entry,COALESCE(d.number,''),s.date,COALESCE(d.due_date,''),d.reference,s.account,s.gross,s.base,EXISTS(SELECT 1 FROM ap_reviews r WHERE r.source=s.id) FROM ap_sources s JOIN expense_documents d ON d.id=s.document WHERE s.vendor=?1 AND s.currency=?2 AND s.date<=?3 AND NOT EXISTS(SELECT 1 FROM gl_reversals r JOIN gl_entries e ON e.id=r.reversal WHERE r.entry=s.entry AND e.date<=?3) ORDER BY COALESCE(d.due_date,s.date),s.document,s.id")?;
    let rows = s
        .query_map(params![vendor, currency, date], |r| {
            Ok(Bill {
                source: r.get(0)?,
                document: r.get(1)?,
                entry: r.get(2)?,
                number: r.get(3)?,
                date: r.get(4)?,
                due: r.get(5)?,
                reference: r.get(6)?,
                account: r.get(7)?,
                gross: r.get(8)?,
                base: r.get(9)?,
                remaining: r.get(8)?,
                base_remaining: r.get(9)?,
                verified: r.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|mut b| {
            let (amount, base) = applied(c, b.source, date)?;
            b.remaining = b.gross.checked_sub(amount).context("A/P amount overflow")?;
            b.base_remaining = b.base.checked_sub(base).context("A/P carrying overflow")?;
            ensure!(
                b.remaining >= 0 && b.base_remaining >= 0,
                "A/P allocations exceed the source liability."
            );
            Ok(b)
        })
        .collect()
}
fn applied(c: &Connection, source: i64, date: &str) -> Result<(i64, i64)> {
    Ok(c.query_row("SELECT COALESCE(sum(a.amount),0),COALESCE(sum(a.base),0) FROM ap_allocations a JOIN vendor_payments p ON p.id=a.payment LEFT JOIN ap_voids v ON v.payment=p.id WHERE a.source=?1 AND p.date<=?2 AND (v.id IS NULL OR v.date>?2)",params![source,date],|r|Ok((r.get(0)?,r.get(1)?)))?)
}
pub fn reconciliation(
    c: &Connection,
    vendor: i64,
    currency: &str,
    date: &str,
) -> Result<Reconciliation> {
    let rows = bills(c, vendor, currency, date)?;
    let open = checked_sum(rows.iter().map(|b| b.remaining))?;
    let base_open = checked_sum(rows.iter().map(|b| b.base_remaining))?;
    // Include historical control accounts even after the configured role changes.
    let accounts="(SELECT account FROM ap_sources UNION SELECT account FROM gl_accounts WHERE role='payable')";
    let (ledger,base_ledger)=c.query_row(&format!("SELECT COALESCE(sum(l.credit-l.debit),0),COALESCE(sum(l.base_credit-l.base_debit),0) FROM gl_lines l JOIN gl_entries e ON e.id=l.entry WHERE l.account IN {accounts} AND l.vendor=?1 AND e.currency=?2 AND e.status='posted' AND e.date<=?3"),params![vendor,currency,date],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let unassigned=c.query_row(&format!("SELECT count(*) FROM gl_lines l JOIN gl_entries e ON e.id=l.entry WHERE l.account IN {accounts} AND l.vendor IS NULL AND e.currency=?1 AND e.status='posted' AND e.date<=?2 AND (l.debit>0 OR l.credit>0) AND NOT EXISTS(SELECT 1 FROM gl_reversals r JOIN gl_entries z ON z.id=r.reversal WHERE r.entry=e.id AND z.date<=?2) AND NOT EXISTS(SELECT 1 FROM gl_reversals r WHERE r.reversal=e.id)"),params![currency,date],|r|r.get(0))?;
    let missing:usize=c.query_row("SELECT count(*) FROM expense_documents d WHERE d.vendor=?1 AND d.currency=?2 AND (d.date IS NULL OR d.date='' OR d.date<=?3) AND EXISTS(SELECT 1 FROM expense_document_lines x JOIN ledger_expenses e ON e.id=x.expense LEFT JOIN expense_tax t ON t.expense=e.id WHERE x.document=d.id AND COALESCE(t.gross,e.amount)>0) AND NOT EXISTS(SELECT 1 FROM gl_sources g JOIN ap_sources s ON s.entry=g.entry AND s.document=d.id WHERE g.kind='expense' AND g.record=d.id)",params![vendor,currency,date],|r|r.get(0))?;
    Ok(Reconciliation {
        open,
        base_open,
        ledger,
        base_ledger,
        unverified: rows.iter().filter(|b| !b.verified).count() + missing,
        missing,
        unassigned,
    })
}
pub fn review(
    c: &mut Connection,
    vendor: i64,
    currency: &str,
    date: &str,
    evidence: &str,
) -> Result<()> {
    ensure!(
        !evidence.trim().is_empty(),
        "Add a reconciliation reference or review note."
    );
    ensure!(
        date <= crate::today().to_string().as_str(),
        "A reconciliation cutoff cannot be in the future."
    );
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let r = reconciliation(&tx, vendor, currency, date)?;
    ensure!(r.balanced(),"The open-item schedule differs from the ledger or has unassigned A/P entries. Review the ledger before approving.");
    for b in bills(&tx, vendor, currency, date)?
        .into_iter()
        .filter(|b| !b.verified)
    {
        tx.execute(
            "INSERT INTO ap_reviews(source,cutoff,evidence) VALUES(?,?,?)",
            params![b.source, date, evidence.trim()],
        )?;
    }
    tx.commit()?;
    Ok(())
}
fn checked_sum(mut values: impl Iterator<Item = i64>) -> Result<i64> {
    values.try_fold(0i64, |sum, v| {
        sum.checked_add(v).context("A/P total overflow")
    })
}
fn latest_event(c: &Connection, vendor: i64, currency: &str) -> Result<String> {
    Ok(c.query_row("SELECT COALESCE(max(date),'') FROM (SELECT date FROM vendor_payments WHERE vendor=?1 AND currency=?2 UNION ALL SELECT v.date FROM ap_voids v JOIN vendor_payments p ON p.id=v.payment WHERE p.vendor=?1 AND p.currency=?2)",params![vendor,currency],|r|r.get(0))?)
}
pub fn preview(c: &Connection, d: &Draft) -> Result<Preview> {
    ensure!(
        d.request.len() == 32 && d.request.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid payment request key."
    );
    crate::currency(&d.currency).map_err(anyhow::Error::msg)?;
    crate::posting::settlement_date(c, &d.date, "", &latest_event(c, d.vendor, &d.currency)?)?;
    ensure!(!d.method.trim().is_empty(), "Select a payment method.");
    ensure!(
        quote(c, &d.currency, &d.date)? == d.quote,
        "The saved exchange quote changed. Review the payment again."
    );
    crate::posting::bank(c, d.bank, &d.currency, true)?;

    let r = reconciliation(c, d.vendor, &d.currency, &d.date)?;
    ensure!(
        r.ready(),
        "Supplier A/P needs reconciliation before recording payments."
    );
    let rows = bills(c, d.vendor, &d.currency, &d.date)?;
    ensure!(
        !d.allocations.is_empty(),
        "Select at least one bill to pay."
    );
    let mut used = std::collections::BTreeSet::new();
    let mut allocations = Vec::new();
    let mut lines = Vec::new();
    for a in &d.allocations {
        ensure!(used.insert(a.source), "A bill was selected more than once.");
        let b = rows
            .iter()
            .find(|b| b.source == a.source)
            .context("Bill vendor, currency, date or source revision changed.")?;
        ensure!(
            b.verified && a.amount > 0 && a.amount <= b.remaining,
            "Payment must be positive and cannot exceed the bill's remaining amount."
        );
        ensure!(
            b.remaining == a.remaining && b.base_remaining == a.base_remaining,
            "A bill balance changed. Review the allocations again."
        );
        ensure!(c.query_row("SELECT EXISTS(SELECT 1 FROM gl_sources WHERE kind='expense' AND record=? AND entry=?)",params![b.document,b.entry],|r|r.get::<_,bool>(0))?,"This bill has a newer posting revision.");
        let released = if a.amount == b.remaining {
            b.base_remaining
        } else {
            i64::try_from(
                (i128::from(b.base) * i128::from(a.amount) + i128::from(b.gross) / 2)
                    / i128::from(b.gross),
            )?
            .min(b.base_remaining)
        };
        allocations.push((b.source, a.amount, released));
        lines.push(crate::gl::Line {
            account: b.account,
            debit: a.amount,
            base_debit: released,
            vendor: Some(d.vendor),
            description: format!("Pay {}", b.number),
            ..Default::default()
        });
    }
    let amount = checked_sum(allocations.iter().map(|a| a.1))?;
    let released = checked_sum(allocations.iter().map(|a| a.2))?;
    let bank_base = crate::convert(amount, d.quote.rate)?;
    let fx = bank_base
        .checked_sub(released)
        .context("FX difference overflow")?;
    lines.push(crate::gl::Line {
        account: d.bank,
        credit: amount,
        base_credit: bank_base,
        description: d.reference.clone(),
        ..Default::default()
    });
    if fx != 0 {
        lines.push(crate::gl::Line {
            account: crate::gl::role(c, "fx")?,
            base_debit: fx.max(0),
            base_credit: fx.checked_neg().context("FX amount overflow")?.max(0),
            description: "Realized supplier settlement FX".into(),
            ..Default::default()
        });
    }
    let mut journal = crate::gl::blank(c)?;
    journal.date = d.date.clone();
    journal.currency = d.currency.clone();
    journal.rate = d.quote.rate;
    journal.origin = "vendor_payment".into();
    journal.memo = format!("Supplier payment: {} {}", d.reference, d.memo);
    journal.lines = lines;
    crate::gl::validate_fixed(c, &journal)?;
    Ok(Preview {
        amount,
        released,
        bank_base,
        fx,
        journal,
        allocations,
    })
}
pub fn record(c: &mut Connection, d: &Draft) -> Result<i64> {
    let payload = serde_json::to_string(d)?;
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if let Some((id, old)) = tx
        .query_row(
            "SELECT id,payload FROM vendor_payments WHERE request=?",
            [&d.request],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?
    {
        ensure!(
            old == payload,
            "This payment request was already used with different input."
        );
        return Ok(id);
    }
    let p = preview(&tx, d)?;
    let number = crate::numbering::allocate(&tx, "vendor_payment")?;
    tx.execute("INSERT INTO vendor_payments(number,request,payload,vendor,currency,date,bank,method,reference,memo,amount,bank_base,quote) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",params![number,d.request,payload,d.vendor,d.currency,d.date,d.bank,d.method,d.reference,d.memo,p.amount,p.bank_base,serde_json::to_string(&d.quote)?])?;
    let id = tx.last_insert_rowid();
    for (source, amount, base) in &p.allocations {
        tx.execute(
            "INSERT INTO ap_allocations(payment,source,amount,base) VALUES(?,?,?,?)",
            params![id, source, amount, base],
        )?;
    }
    crate::gl::vendor_payment(&tx, id, p.journal)?;
    tx.commit()?;
    Ok(id)
}
pub fn void(c: &mut Connection, id: i64, date: &str, reason: &str) -> Result<()> {
    crate::task_date(date).map_err(anyhow::Error::msg)?;
    ensure!(!reason.trim().is_empty(), "A reversal reason is required.");
    ensure!(
        date <= crate::today().to_string().as_str(),
        "A payment reversal cannot be dated in the future."
    );
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let (vendor, currency, payment_date): (i64, String, String) = tx.query_row(
        "SELECT vendor,currency,date FROM vendor_payments WHERE id=?",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    crate::posting::settlement_date(
        &tx,
        date,
        &payment_date,
        &latest_event(&tx, vendor, &currency)?,
    )?;
    ensure!(
        !tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM ap_voids WHERE payment=?)",
            [id],
            |r| r.get::<_, bool>(0)
        )?,
        "This supplier payment is already voided."
    );
    let entry: i64 = tx.query_row(
        "SELECT entry FROM gl_sources WHERE kind='vendor_payment' AND record=?",
        [id],
        |r| r.get(0),
    )?;
    ensure!(
        !tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM gl_reversals WHERE entry=?)",
            [entry],
            |r| r.get::<_, bool>(0)
        )?,
        "The payment source was already reversed."
    );
    crate::gl::void_source(&tx, "vendor_payment", id, date, reason)?;
    let reversal: i64 = tx.query_row(
        "SELECT reversal FROM gl_reversals WHERE entry=?",
        [entry],
        |r| r.get(0),
    )?;
    tx.execute(
        "INSERT INTO ap_voids(payment,date,reason,reversal) VALUES(?,?,?,?)",
        params![id, date, reason.trim(), reversal],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn validate(c: &Connection) -> Result<()> {
    let invalid:bool=c.query_row(r#"SELECT
        EXISTS(SELECT 1 FROM vendor_payments p WHERE p.request IS NOT json_extract(p.payload,'$.request') OR p.vendor IS NOT json_extract(p.payload,'$.vendor') OR p.bank IS NOT json_extract(p.payload,'$.bank') OR p.currency IS NOT json_extract(p.payload,'$.currency') OR p.date IS NOT json_extract(p.payload,'$.date') OR p.method IS NOT json_extract(p.payload,'$.method') OR p.reference IS NOT json_extract(p.payload,'$.reference') OR p.memo IS NOT json_extract(p.payload,'$.memo')) OR
        EXISTS(SELECT 1 FROM ap_allocations a JOIN vendor_payments p ON p.id=a.payment JOIN ap_sources b ON b.id=a.source WHERE p.vendor!=b.vendor OR p.currency!=b.currency OR p.date<b.date) OR
        EXISTS(SELECT 1 FROM vendor_payments p LEFT JOIN gl_sources s ON s.kind='vendor_payment' AND s.record=p.id LEFT JOIN gl_entries e ON e.id=s.entry LEFT JOIN ap_banks b ON b.id=p.bank WHERE e.id IS NULL OR e.origin!='vendor_payment' OR e.status!='posted' OR e.date!=p.date OR e.currency!=p.currency OR b.currency!=p.currency OR p.amount!=(SELECT COALESCE(sum(amount),0) FROM ap_allocations WHERE payment=p.id)) OR
        EXISTS(SELECT 1 FROM ap_voids v JOIN vendor_payments p ON p.id=v.payment LEFT JOIN gl_sources s ON s.kind='vendor_payment' AND s.record=p.id LEFT JOIN gl_reversals r ON r.entry=s.entry LEFT JOIN gl_entries e ON e.id=r.reversal WHERE r.reversal IS NOT v.reversal OR e.date IS NOT v.date OR v.date<p.date) OR
        EXISTS(SELECT 1 FROM gl_sources s JOIN gl_reversals r ON r.entry=s.entry WHERE s.kind='vendor_payment' AND NOT EXISTS(SELECT 1 FROM ap_voids v WHERE v.payment=s.record AND v.reversal=r.reversal)) OR
        EXISTS(SELECT 1 FROM gl_sources s WHERE s.kind='vendor_payment' AND NOT EXISTS(SELECT 1 FROM vendor_payments p WHERE p.id=s.record))"#,[],|r|r.get(0))?;
    ensure!(
        !invalid,
        "Supplier payment, allocation or reversal links are inconsistent."
    );
    let mut sources =
        c.prepare("SELECT entry,vendor,currency,date,account,gross,base FROM ap_sources")?;
    for row in sources.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, i64>(6)?,
        ))
    })? {
        let (entry, vendor, currency, date, account, gross, base) = row?;
        let j = crate::gl::load(c, entry)?;
        let line = j.lines.last().context("Missing supplier liability line")?;
        ensure!(
            j.origin == "expense"
                && j.status == "posted"
                && j.currency == currency
                && j.date == date
                && line.account == account
                && line.vendor == Some(vendor)
                && line.credit == gross
                && line.base_credit == base
                && line.debit == 0
                && line.base_debit == 0,
            "Supplier bill carrying snapshot differs from its posted source."
        );
    }
    let mut stmt = c.prepare(
        "SELECT id,payload,quote,bank_base,amount,date FROM vendor_payments ORDER BY id",
    )?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, String>(5)?,
        ))
    })? {
        let (id, raw, quote, bank_base, amount, date) = row?;
        crate::task_date(&date).map_err(anyhow::Error::msg)?;
        let d: Draft = serde_json::from_str(&raw)?;
        let q: Quote = serde_json::from_str(&quote)?;
        ensure!(
            d.quote == q && d.date == date && crate::convert(amount, q.rate)? == bank_base,
            "Supplier payment snapshot/rate mismatch."
        );
        let entry: i64 = c.query_row(
            "SELECT entry FROM gl_sources WHERE kind='vendor_payment' AND record=?",
            [id],
            |r| r.get(0),
        )?;
        let j = crate::gl::load(c, entry)?;
        ensure!(
            j.rate == q.rate && j.currency == d.currency && j.date == d.date,
            "Supplier journal quote or date mismatch."
        );
        let mut s =
            c.prepare("SELECT source,amount,base FROM ap_allocations WHERE payment=? ORDER BY id")?;
        let a = s
            .query_map([id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(
            a.len() == d.allocations.len() && j.lines.len() > a.len(),
            "Supplier allocation snapshot mismatch."
        );
        for ((source, amount, base), (snapshot, line)) in
            a.iter().zip(d.allocations.iter().zip(&j.lines))
        {
            let (account, vendor): (i64, i64) = c.query_row(
                "SELECT account,vendor FROM ap_sources WHERE id=?",
                [source],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            ensure!(
                *source == snapshot.source
                    && *amount == snapshot.amount
                    && line.account == account
                    && line.vendor == Some(vendor)
                    && line.debit == *amount
                    && line.credit == 0
                    && line.base_debit == *base
                    && line.base_credit == 0,
                "Supplier A/P release does not match the allocation."
            );
        }
        let bank = &j.lines[a.len()];
        ensure!(
            bank.account == d.bank
                && bank.credit == amount
                && bank.base_credit == bank_base
                && bank.debit == 0
                && bank.base_debit == 0,
            "Supplier bank posting does not match the payment."
        );
    }
    let mut groups=c.prepare("SELECT DISTINCT vendor,currency,date FROM (SELECT vendor,currency,date FROM ap_sources UNION SELECT vendor,currency,date FROM vendor_payments UNION SELECT p.vendor,p.currency,v.date FROM ap_voids v JOIN vendor_payments p ON p.id=v.payment)")?;
    for row in groups.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (vendor, currency, date) = row?;
        bills(c, vendor, &currency, &date)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(currency: &str) -> Result<(Connection, i64, i64, i64)> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        let vendor = crate::vendor::save(
            &mut c,
            &crate::vendor::Vendor {
                name: "Supplier".into(),
                currency: currency.into(),
                ..Default::default()
            },
        )?;
        let bank: i64 = c.query_row(
            "SELECT id FROM accounts WHERE code=?",
            [if currency == "CAD" { "1010" } else { "1020" }],
            |r| r.get(0),
        )?;
        configure_bank(&mut c, bank, currency)?;
        rate(&c, 135000000)?;
        let bill = make_bill(&mut c, vendor, currency, "1000")?;
        Ok((c, vendor, bank, bill))
    }
    fn day(n: i64) -> String {
        (crate::today() - chrono::Duration::days(n)).to_string()
    }
    fn rate(c: &Connection, rate: i64) -> Result<()> {
        c.execute("INSERT INTO fx_rates(source,target,rate) VALUES('USD','CAD',?) ON CONFLICT(source,target) DO UPDATE SET rate=excluded.rate",[rate])?;
        Ok(())
    }
    fn make_bill(c: &mut Connection, vendor: i64, currency: &str, amount: &str) -> Result<i64> {
        let account = crate::accounting::default_account(c)?;
        crate::expense_doc::save(
            c,
            &crate::expense_doc::Document {
                id: 0,
                vendor: Some(vendor),
                date: day(20),
                currency: currency.into(),
                reference: "SUP-1".into(),
                due_date: Some(day(2)),
                memo: String::new(),
                terms: String::new(),
                lines: vec![crate::accounting::Expense {
                    id: 0,
                    date: day(20),
                    vendor: Some(vendor),
                    currency: currency.into(),
                    amount: amount.into(),
                    description: "Services".into(),
                    account,
                    task: None,
                    tax: Default::default(),
                }],
            },
        )
    }
    fn draft(
        c: &Connection,
        vendor: i64,
        bank: i64,
        currency: &str,
        days: i64,
        amount: i64,
    ) -> Result<Draft> {
        let b = bills(c, vendor, currency, &day(days))?.remove(0);
        Ok(Draft {
            request: request_id(c)?,
            vendor,
            bank,
            currency: currency.into(),
            date: day(days),
            method: "Bank transfer".into(),
            reference: "TEST".into(),
            memo: String::new(),
            quote: quote(c, currency, &day(days))?,
            allocations: vec![Allocation {
                source: b.source,
                amount,
                remaining: b.remaining,
                base_remaining: b.base_remaining,
            }],
        })
    }
    #[test]
    fn partial_multi_stage_fx_and_dated_voids_preserve_cutoffs() -> Result<()> {
        let (mut c, v, bank, bill) = fixture("USD")?;
        rate(&c, 137000000)?;
        let d1 = draft(&c, v, bank, "USD", 15, 40000)?;
        let p = preview(&c, &d1)?;
        assert_eq!((p.released, p.bank_base, p.fx), (54000, 54800, 800));
        let p1 = record(&mut c, &d1)?;
        assert_eq!(record(&mut c, &d1)?, p1);
        let mut changed = d1.clone();
        changed.memo = "Changed duplicate".into();
        assert!(record(&mut c, &changed).is_err());
        rate(&c, 134000000)?;
        let d2 = draft(&c, v, bank, "USD", 10, 60000)?;
        let p = preview(&c, &d2)?;
        assert_eq!((p.released, p.bank_base, p.fx), (81000, 80400, -600));
        let p2 = record(&mut c, &d2)?;
        assert_eq!(bills(&c, v, "USD", &day(16))?[0].remaining, 100000);
        assert_eq!(bills(&c, v, "USD", &day(12))?[0].remaining, 60000);
        assert_eq!(bills(&c, v, "USD", &day(8))?[0].remaining, 0);
        let saved_bill = crate::expense_doc::load(&c, bill)?;
        assert!(crate::expense_doc::save(&mut c, &saved_bill).is_err());
        assert!(void(&mut c, p1, &day(11), "Backdated").is_err());
        void(&mut c, p1, &day(5), "Wrong reference")?;
        assert_eq!(bills(&c, v, "USD", &day(4))?[0].remaining, 40000);
        assert_eq!(bills(&c, v, "USD", &day(8))?[0].remaining, 0);
        assert!(void(&mut c, p1, &day(0), "Again").is_err());
        let mut backdated = draft(&c, v, bank, "USD", 12, 10000)?;
        backdated.allocations[0].remaining = 60000;
        assert!(record(&mut c, &backdated).is_err());
        void(&mut c, p2, &day(0), "Reversal")?;
        let b = &bills(&c, v, "USD", &day(0))?[0];
        assert_eq!((b.remaining, b.base_remaining), (100000, 135000));
        assert!(reconciliation(&c, v, "USD", &day(0))?.ready());
        crate::storage::validate_projections(&c)?;
        // Exercise an additive v18-to-v19 upgrade with allocated and voided supplier history.
        let before: Vec<(i64, String, String)> = {
            let mut q = c.prepare("SELECT id,payload,quote FROM vendor_payments ORDER BY id")?;
            let rows = q
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?;
            rows
        };
        c.execute_batch("DROP TABLE receipt_requests; PRAGMA user_version=18;")?;
        crate::init(&c)?;
        for (id, payload, quote) in before {
            let actual: (String, String) = c.query_row(
                "SELECT payload,quote FROM vendor_payments WHERE id=?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            assert_eq!(actual, (payload, quote));
        }
        assert_eq!(record(&mut c, &d1)?, p1);
        assert_eq!(
            c.query_row("SELECT count(*) FROM receipt_requests", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        crate::storage::validate_projections(&c)?;
        Ok(())
    }
    #[test]
    fn allocation_checks_idempotence_atomic_failure_and_locking() -> Result<()> {
        let (mut c, v, bank, _) = fixture("CAD")?;
        let d = draft(&c, v, bank, "CAD", 10, 100000)?;
        let mut invalid = d.clone();
        invalid.allocations[0].amount = 100001;
        assert!(record(&mut c, &invalid).is_err());
        invalid = d.clone();
        invalid.allocations.push(invalid.allocations[0].clone());
        assert!(record(&mut c, &invalid).is_err());
        invalid = d.clone();
        invalid.currency = "USD".into();
        assert!(record(&mut c, &invalid).is_err());
        invalid = d.clone();
        invalid.allocations[0].remaining -= 1;
        assert!(record(&mut c, &invalid).is_err());
        c.execute_batch("CREATE TRIGGER fail_supplier_source BEFORE INSERT ON gl_sources WHEN NEW.kind='vendor_payment' BEGIN SELECT RAISE(ABORT,'Injected failure'); END;")?;
        let seq: i64 = c.query_row(
            "SELECT next_value FROM number_sequences WHERE kind='vendor_payment'",
            [],
            |r| r.get(0),
        )?;
        assert!(record(&mut c, &d).is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM vendor_payments", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            c.query_row(
                "SELECT next_value FROM number_sequences WHERE kind='vendor_payment'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            seq
        );
        c.execute_batch("DROP TRIGGER fail_supplier_source")?;
        c.execute("UPDATE gl_settings SET locked_through=?", [day(10)])?;
        assert!(record(&mut c, &d).is_err());
        c.execute("UPDATE gl_settings SET locked_through=''", [])?;
        let id = record(&mut c, &d)?;
        assert_eq!(record(&mut c, &d)?, id);
        let mut stale = d.clone();
        stale.request = request_id(&c)?;
        assert!(record(&mut c, &stale).is_err());
        assert!(c
            .execute("UPDATE vendor_payments SET amount=1 WHERE id=?", [id])
            .is_err());
        assert!(c
            .execute("DELETE FROM ap_allocations WHERE payment=?", [id])
            .is_err());
        c.execute("UPDATE gl_settings SET locked_through=?", [day(0)])?;
        assert!(void(&mut c, id, &day(0), "Locked").is_err());
        assert_eq!(bills(&c, v, "CAD", &day(0))?[0].remaining, 0);
        Ok(())
    }
    #[test]
    fn historical_reviews_and_manual_or_unassigned_entries_gate_payments() -> Result<()> {
        let (mut c, v, bank, _) = fixture("CAD")?;
        c.execute_batch(
            "DROP TRIGGER ap_reviews_fixed_DELETE; DELETE FROM ap_reviews; PRAGMA user_version=17;",
        )?;
        crate::init(&c)?;
        crate::init(&c)?;
        assert_eq!(reconciliation(&c, v, "CAD", &day(0))?.unverified, 1);
        let d = draft(&c, v, bank, "CAD", 0, 1000)?;
        assert!(record(&mut c, &d).is_err());
        review(&mut c, v, "CAD", &day(0), "Verified supplier statement")?;
        let mut j = crate::gl::blank(&c)?;
        j.date = day(15);
        j.origin = "manual".into();
        j.lines = vec![
            crate::gl::Line {
                account: crate::gl::role(&c, "payable")?,
                debit: 10000,
                vendor: Some(v),
                ..Default::default()
            },
            crate::gl::Line {
                account: bank,
                credit: 10000,
                ..Default::default()
            },
        ];
        let entry = crate::gl::save(&mut c, &j, true)?;
        assert!(!reconciliation(&c, v, "CAD", &day(0))?.balanced());
        assert!(record(&mut c, &d).is_err());
        crate::gl::reverse(&mut c, entry, &day(0), "Reviewed correction")?;
        assert!(reconciliation(&c, v, "CAD", &day(0))?.ready());
        c.execute("UPDATE vendors SET active=0 WHERE id=?", [v])?;
        record(&mut c, &d)?; // Inactive suppliers still receive settlement of old liabilities.
        Ok(())
    }
    #[test]
    fn changed_quote_control_accounts_and_final_cent_release() -> Result<()> {
        let (mut c, v, bank, _) = fixture("USD")?;
        let d = draft(&c, v, bank, "USD", 10, 1000)?;
        rate(&c, 137000000)?;
        assert!(record(&mut c, &d).is_err());
        let old = crate::gl::role(&c, "payable")?;
        c.execute(
            "INSERT INTO accounts(code,name,kind) VALUES('2099','New AP','Liability')",
            [],
        )?;
        let new = c.last_insert_rowid();
        c.execute(
            "UPDATE gl_accounts SET account=? WHERE role='payable'",
            [new],
        )?;
        make_bill(&mut c, v, "USD", "0.03")?;
        let all = bills(&c, v, "USD", &day(0))?;
        let mut d = draft(&c, v, bank, "USD", 5, 100000)?;
        d.allocations = all
            .iter()
            .map(|b| Allocation {
                source: b.source,
                amount: b.remaining,
                remaining: b.remaining,
                base_remaining: b.base_remaining,
            })
            .collect();
        let p = preview(&c, &d)?;
        assert_eq!(p.journal.lines[0].account, old);
        assert_eq!(p.journal.lines[1].account, new);
        record(&mut c, &d)?;
        assert!(bills(&c, v, "USD", &day(0))?
            .iter()
            .all(|b| b.remaining == 0 && b.base_remaining == 0));
        assert!(reconciliation(&c, v, "USD", &day(0))?.ready());
        Ok(())
    }

    #[test]
    fn independent_connections_cannot_pay_the_same_remaining_balance_twice() -> Result<()> {
        let (source, vendor, bank, _) = fixture("CAD")?;
        let path = std::env::temp_dir().join(format!(
            "axibiz-ap-concurrent-{}-{}.db",
            std::process::id(),
            request_id(&source)?
        ));
        source.backup(rusqlite::DatabaseName::Main, &path, None)?;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let c = Connection::open(&path)?;
            crate::auth::register(&c)?;
            c.execute_batch("PRAGMA foreign_keys=ON")?;
            c.busy_timeout(std::time::Duration::from_secs(5))?;
            let d = draft(&c, vendor, bank, "CAD", 0, 100000)?;
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                let mut c = c;
                barrier.wait();
                record(&mut c, &d)
            }));
        }
        barrier.wait();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        let c = Connection::open(&path)?;
        assert_eq!(
            c.query_row("SELECT count(*) FROM vendor_payments", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        assert_eq!(bills(&c, vendor, "CAD", &day(0))?[0].remaining, 0);
        validate(&c)?;
        drop(c);
        std::fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn final_partial_settlement_consumes_the_exact_remaining_base_cent() -> Result<()> {
        let (mut c, vendor, bank, _) = fixture("USD")?;
        make_bill(&mut c, vendor, "USD", "0.03")?;
        let b = bills(&c, vendor, "USD", &day(0))?.pop().unwrap();
        assert_eq!(b.base, 4);
        let mut first = draft(&c, vendor, bank, "USD", 10, 1)?;
        first.allocations = vec![Allocation {
            source: b.source,
            amount: 1,
            remaining: 3,
            base_remaining: 4,
        }];
        assert_eq!(preview(&c, &first)?.released, 1);
        record(&mut c, &first)?;
        let mut last = draft(&c, vendor, bank, "USD", 5, 2)?;
        last.allocations = vec![Allocation {
            source: b.source,
            amount: 2,
            remaining: 2,
            base_remaining: 3,
        }];
        assert_eq!(preview(&c, &last)?.released, 3);
        record(&mut c, &last)?;
        let b = bills(&c, vendor, "USD", &day(0))?.pop().unwrap();
        assert_eq!((b.remaining, b.base_remaining), (0, 0));
        validate(&c)?;
        Ok(())
    }

    #[test]
    fn missing_historical_sources_never_turn_into_a_verified_zero_balance() -> Result<()> {
        let (mut c, vendor, _, bill) = fixture("CAD")?;
        crate::gl::void_source(&c, "expense", bill, &day(0), "Historical reversal evidence")?;
        c.execute(
            "DELETE FROM gl_sources WHERE kind='expense' AND record=?",
            [bill],
        )?;
        crate::init(&c)?;
        let r = reconciliation(&c, vendor, "CAD", &day(0))?;
        assert_eq!((r.open, r.ledger), (0, 0));
        assert_eq!(r.missing, 1);
        assert!(!r.ready());
        assert!(review(
            &mut c,
            vendor,
            "CAD",
            &day(0),
            "Must not bypass missing history"
        )
        .is_err());
        Ok(())
    }
}
