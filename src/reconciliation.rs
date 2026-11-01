//! Read-only cutoff reconciliation of recorded subledgers and control accounts.
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub struct CurrencyBalance {
    pub currency: String,
    pub remaining: Option<i64>,
}
pub struct ControlBalance {
    pub label: String,
    pub source_currency: String,
    pub source: i64,
    pub ledger: i64,
    pub difference: Option<i64>,
}
pub struct Report {
    pub date: String,
    pub base_currency: String,
    pub currencies: Vec<CurrencyBalance>,
    pub controls: Vec<ControlBalance>,
    pub notes: Vec<String>,
}
fn add(map: &mut BTreeMap<i64, i64>, account: i64, amount: i64) -> Result<()> {
    let value = map.entry(account).or_default();
    *value = value.checked_add(amount).context("控制科目金额过大")?;
    Ok(())
}
fn ledger_balance(c: &Connection, account: i64, date: &str) -> Result<i64> {
    Ok(c.query_row("SELECT COALESCE(sum(l.base_debit-l.base_credit),0) FROM gl_lines l JOIN gl_entries e ON e.id=l.entry WHERE l.account=? AND e.status='posted' AND e.date<=?",params![account,date],|r|r.get(0))?)
}
pub fn build(c: &Connection, date: &str) -> Result<Report> {
    crate::task_date(date).map_err(anyhow::Error::msg)?;
    ensure!(c.is_autocommit(), "请在业务事务之外执行对账");
    let tx = c.unchecked_transaction()?;
    crate::storage::validate_projections(&tx)?;
    let report = build_on(&tx, date)?;
    tx.commit()?;
    Ok(report)
}
fn build_on(c: &Connection, date: &str) -> Result<Report> {
    let base_currency = crate::gl::base(c)?;
    let mut notes=vec!["只核对已记录业务及当前、来源快照能识别的控制科目；不是历史文件时点复原，也不自动确认税务资格。".into()];
    let mut amounts = BTreeMap::<String, (i64, bool)>::new();
    let mut expected = BTreeMap::<i64, i64>::new();
    let mut ar_accounts = BTreeSet::from([crate::gl::role(c, "receivable")?]);
    let mut ar_complete = true;
    let mut missing = 0usize;
    let mut invoices=c.prepare("SELECT i.id,i.snapshot,s.entry,s.snapshot FROM invoices i LEFT JOIN gl_sources s ON s.kind='invoice' AND s.record=i.id ORDER BY i.id")?;
    for row in invoices.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<i64>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })? {
        let (id, raw, entry, source) = row?;
        let Some(historical) = crate::billing::balance_as_of(c, id, date)? else {
            continue;
        };
        let balance = historical.balance;
        if balance.remaining < 0 {
            ar_complete = false;
            missing += 1;
            notes.push(format!("发票 ID {id} 截止日出现负数应收，需核对重记收款的业务日期或预收款分类；即使 GL 同为负数也不认定已对平。"));
        }
        let total = amounts.entry(balance.currency.clone()).or_insert((0, true));
        total.0 = total
            .0
            .checked_add(balance.remaining)
            .context("应收合计过大")?;
        total.1 &= historical.complete;
        if !historical.complete {
            ar_complete = false;
            missing += 1;
        }
        if balance.total == 0 {
            continue;
        }
        if entry.is_none() {
            ar_complete = false;
            missing += 1;
            continue;
        }
        let data: Value = serde_json::from_str(&raw)?;
        let journal: crate::gl::Journal =
            serde_json::from_str(source.as_deref().context("缺少发票来源账面值")?)?;
        let ar = journal.lines.first().context("缺少应收账面行")?;
        ensure!(
            ar.debit == balance.total && ar.credit == 0 && journal.currency == balance.currency,
            "发票与应收来源金额不一致"
        );
        ar_accounts.insert(ar.account);
        add(&mut expected, ar.account, ar.base_debit)?;
        let mut payments=c.prepare("SELECT p.amount,p.date,p.voided,p.voided_on,s.entry,s.snapshot FROM payments p LEFT JOIN gl_sources s ON s.kind='payment' AND s.record=p.id WHERE p.invoice=? AND (p.date<=? OR p.date IS NULL)")?;
        for payment in payments.query_map(params![id, date], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, bool>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })? {
            let (amount, paid_on, voided, voided_on, entry, source) = payment?;
            if paid_on.is_none() || (voided && voided_on.is_none()) {
                ar_complete = false;
                continue;
            }
            if voided
                && voided_on
                    .as_deref()
                    .is_some_and(|void_date| void_date <= date)
            {
                continue;
            }
            if paid_on.as_deref().is_some_and(|paid| {
                data["invoice_date"]
                    .as_str()
                    .is_some_and(|issue| paid < issue)
            }) {
                ar_complete = false;
            }
            if entry.is_none() {
                ar_complete = false;
                missing += 1;
                continue;
            }
            let payment: crate::gl::Journal =
                serde_json::from_str(source.as_deref().context("缺少收款来源账面值")?)?;
            let paid_ar = payment.lines.get(1).context("缺少收款应收账面行")?;
            ensure!(
                payment.currency == balance.currency
                    && paid_ar.credit == amount
                    && paid_ar.debit == 0,
                "收款与应收来源金额不一致"
            );
            ar_accounts.insert(paid_ar.account);
            add(&mut expected, paid_ar.account, -paid_ar.base_credit)?;
        }
    }
    let early:i64=c.query_row("SELECT count(*) FROM payments p JOIN invoices i ON i.id=p.invoice WHERE p.date<=?1 AND p.date<COALESCE(json_extract(i.snapshot,'$.invoice_date'),substr(i.created,1,10)) AND (p.voided=0 OR p.voided_on>?1 OR p.voided_on IS NULL)",[date],|r|r.get(0))?;
    if early > 0 {
        ar_complete = false;
        notes.push(format!(
            "{early} 条收款早于发票日期，需核对预收款分类；不认定应收已对平。"
        ));
    }
    if !ar_complete {
        notes.push(format!("应收存在历史期初、日期、来源或收款分类待核对（涉及 {missing} 项），来源账面值可能只是部分金额；不输出确定差额。"));
    }

    let mut tax_accounts = BTreeSet::new();
    let mut agencies = c.prepare("SELECT payable_account,recoverable_account FROM tax_agencies")?;
    for row in agencies.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?))
    })? {
        let (payable, recoverable) = row?;
        tax_accounts.insert(payable);
        if let Some(account) = recoverable {
            tax_accounts.insert(account);
        }
    }
    let mut postings = c.prepare("SELECT snapshot FROM tax_postings WHERE date<=?")?;
    for raw in postings.query_map([date], |r| r.get::<_, String>(0))? {
        let part: Value = serde_json::from_str(&raw?)?;
        for key in ["payable_account", "recoverable_account"] {
            if let Some(account) = part[key].as_i64() {
                tax_accounts.insert(account);
            }
        }
    }
    let mut settlements=c.prepare("SELECT t.kind,s.snapshot FROM tax_cash t JOIN gl_sources s ON s.kind='tax' AND s.record=t.id WHERE t.date<=? AND s.entry IS NOT NULL")?;
    for settlement in settlements.query_map([date], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (kind, raw) = settlement?;
        let journal: crate::gl::Journal = serde_json::from_str(&raw)?;
        let index = if kind == "payment" || kind == "decrease" {
            0
        } else {
            1
        };
        tax_accounts.insert(
            journal
                .lines
                .get(index)
                .context("缺少税款控制科目行")?
                .account,
        );
    }
    let mixed_ar = ar_accounts.contains(&crate::gl::role(c, "bank")?)
        || ar_accounts.contains(&crate::gl::role(c, "payable")?);
    if mixed_ar {
        ar_complete = false;
        notes.push("应收科目与默认银行或应付科目重叠，需核对映射，不能认定应收对平。".into());
    }
    let overlap = ar_accounts.intersection(&tax_accounts).next().is_some();
    if overlap {
        ar_complete = false;
        notes.push("应收与税务控制科目重叠，需先核对科目映射；不能自动认定对平。".into());
    }
    let mut controls = Vec::new();
    for account in ar_accounts {
        let (code, name): (String, String) = c.query_row(
            "SELECT code,name FROM accounts WHERE id=?",
            [account],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let source = *expected.get(&account).unwrap_or(&0);
        let ledger = ledger_balance(c, account, date)?;
        let manual:i64=c.query_row("SELECT count(DISTINCT e.id) FROM gl_lines l JOIN gl_entries e ON e.id=l.entry WHERE l.account=? AND e.status='posted' AND e.origin='manual' AND e.date<=?",params![account,date],|r|r.get(0))?;
        if manual > 0 {
            notes.push(format!(
                "应收科目 {code} 存在 {manual} 条手工分录，可能为合法期初或调整，需核对支持凭证。"
            ));
        }
        controls.push(ControlBalance {
            label: format!("AR {code} · {name}"),
            source_currency: base_currency.clone(),
            source,
            ledger,
            difference: if ar_complete {
                Some(ledger.checked_sub(source).context("差额过大")?)
            } else {
                None
            },
        });
    }
    let tax_amount:i64=c.query_row("SELECT COALESCE(sum(CASE WHEN source='invoice' THEN cad_amount ELSE -cad_recoverable END),0) FROM tax_postings WHERE date<=?",[date],|r|r.get(0))?;
    let cash:i64=c.query_row("SELECT COALESCE(sum(CASE WHEN kind IN('payment','decrease') THEN -amount ELSE amount END),0) FROM tax_cash WHERE date<=?1 AND (voided=0 OR voided_on>?1)",[date],|r|r.get(0))?;
    let unknown: i64 = c.query_row(
        "SELECT count(*) FROM tax_cash WHERE date<=? AND voided=1 AND voided_on IS NULL",
        [date],
        |r| r.get(0),
    )?;
    let source = tax_amount.checked_add(cash).context("税务余额过大")?;
    let ambiguous = tax_accounts.contains(&crate::gl::role(c, "bank")?)
        || tax_accounts.contains(&crate::gl::role(c, "payable")?);
    if ambiguous {
        notes.push("税务科目与默认银行或应付科目重叠，不能隔离税务控制余额，请核对映射。".into());
    }
    let mut ledger = 0i64;
    for account in tax_accounts {
        ledger = ledger
            .checked_sub(ledger_balance(c, account, date)?)
            .context("税务 GL 余额过大")?;
    }
    let comparable = base_currency == "CAD" && unknown == 0 && !overlap && !ambiguous;
    if base_currency != "CAD" {
        notes.push("税务子账按 CAD 记录，GL 本币非 CAD，税务两栏单位不同，不计算差额；不得用当前汇率直接重估。".into());
    }
    if unknown > 0 {
        notes.push(format!(
            "{unknown} 条历史税款撤销日期未知，税务截至余额可能不完整，不计算确定差额。"
        ));
    }
    controls.push(ControlBalance {
        label: "Tax net · 税务净控制余额".into(),
        source_currency: "CAD".into(),
        source,
        ledger,
        difference: if comparable {
            Some(ledger.checked_sub(source).context("税务差额过大")?)
        } else {
            None
        },
    });
    notes.push("应收比较使用来源快照的历史本币账面值，税务为合并控制余额；差额须查明手工分录、期初、科目映射和期间，不自动做调整。".into());
    let mut groups=c.prepare("SELECT DISTINCT s.vendor,s.currency,v.name FROM (SELECT vendor,currency FROM ap_sources UNION SELECT vendor,currency FROM expense_documents WHERE vendor IS NOT NULL) s JOIN vendors v ON v.id=s.vendor ORDER BY v.name,s.currency")?;
    for row in groups.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (vendor, currency, name) = row?;
        let r = crate::ap::reconciliation(c, vendor, &currency, date)?;
        let comparable = r.unverified == 0 && r.unassigned == 0;
        if !r.ready() {
            notes.push(format!("Supplier A/P needs reconciliation: {name} / {currency}; unreviewed items {}, unassigned lines {}.",r.unverified,r.unassigned));
        }
        controls.push(ControlBalance {
            label: format!("AP {name} ({currency})"),
            source_currency: currency.clone(),
            source: r.open,
            ledger: r.ledger,
            difference: if comparable {
                Some(
                    r.ledger
                        .checked_sub(r.open)
                        .context("A/P difference overflow")?,
                )
            } else {
                None
            },
        });
        controls.push(ControlBalance {
            label: format!("AP carrying {name} ({currency})"),
            source_currency: base_currency.clone(),
            source: r.base_open,
            ledger: r.base_ledger,
            difference: if comparable {
                Some(
                    r.base_ledger
                        .checked_sub(r.base_open)
                        .context("A/P base difference overflow")?,
                )
            } else {
                None
            },
        });
    }
    Ok(Report {
        date: date.into(),
        base_currency,
        currencies: amounts
            .into_iter()
            .map(|(currency, (amount, complete))| CurrencyBalance {
                currency,
                remaining: complete.then_some(amount),
            })
            .collect(),
        controls,
        notes,
    })
}
impl Report {
    pub fn csv_rows(&self) -> Vec<Vec<String>> {
        let mut rows = Vec::new();
        for balance in &self.currencies {
            rows.push(vec![
                self.date.clone(),
                "AR original currency".into(),
                balance.currency.clone(),
                balance
                    .remaining
                    .map(crate::format_money)
                    .unwrap_or_default(),
                String::new(),
                String::new(),
                String::new(),
                if balance.remaining.is_some() {
                    "Recorded balance"
                } else {
                    "Incomplete dates"
                }
                .into(),
            ]);
        }
        for control in &self.controls {
            rows.push(vec![
                self.date.clone(),
                control.label.clone(),
                control.source_currency.clone(),
                crate::format_money(control.source),
                self.base_currency.clone(),
                crate::format_money(control.ledger),
                control
                    .difference
                    .map(crate::format_money)
                    .unwrap_or_default(),
                match control.difference {
                    Some(0) => "Matched recorded scope",
                    Some(_) => "Review difference",
                    None => "Incomplete or non-comparable",
                }
                .into(),
            ]);
        }
        for note in &self.notes {
            rows.push(vec![
                self.date.clone(),
                "NOTE".into(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                note.clone(),
            ]);
        }
        rows
    }
}
pub const CSV_HEADERS: [&str; 8] = [
    "As of",
    "Scope",
    "Source currency",
    "Recorded source",
    "GL base currency",
    "GL",
    "Difference",
    "Status / notes",
];

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Result<Connection> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Buyer','');UPDATE tax_agencies SET registered=1,registration='TEST';")?;
        Ok(c)
    }
    fn invoice(
        c: &mut Connection,
        key: &str,
        currency: &str,
        amount: i64,
        tax: i64,
    ) -> Result<i64> {
        let draft = crate::billing::DirectInvoice {
            request_key: key.into(),
            customer: 1,
            date: "2026-10-01".into(),
            currency: currency.into(),
            title: "Service".into(),
            lines: vec![crate::billing::DirectLine {
                price_id: 0,
                description: "Service".into(),
                unit: "item".into(),
                quantity: 1,
                unit_price: Some(amount),
                tax_code: Some(tax),
            }],
        };
        let preview = crate::billing::direct_preview(c, &draft)?;
        crate::billing::direct_confirm(c, &draft, &preview)
    }
    fn all_matched(report: &Report) -> bool {
        report
            .controls
            .iter()
            .all(|control| control.difference == Some(0))
    }
    #[test]
    fn cutoff_reconciles_receipt_and_tax_reversals_and_identifies_manual_differences() -> Result<()>
    {
        let mut c = fixture()?;
        let id = invoice(&mut c, "CAD", "CAD", 5000, 1)?;
        let payment = crate::billing::add_payment(&mut c, id, "2026-10-05", "20", "Cash", "")?;
        crate::tax::cash(&c, 1, "2026-10-03", "payment", "1", "REF", "Remittance")?;
        let tax: i64 = c.query_row("SELECT max(id) FROM tax_cash", [], |r| r.get(0))?;
        crate::billing::void_payment_on(&mut c, payment, "Correction", "2026-10-15")?;
        crate::tax::void_cash(&c, tax)?;
        let changes = c.total_changes();
        let before = build(&c, "2026-10-10")?;
        assert_eq!(c.total_changes(), changes);
        assert!(all_matched(&before));
        assert_eq!(before.currencies[0].remaining, Some(3250));
        assert_eq!(before.controls.last().unwrap().source, 150);
        let after = build(&c, &crate::today().to_string())?;
        assert!(all_matched(&after));
        assert_eq!(after.currencies[0].remaining, Some(5250));
        assert_eq!(after.controls.last().unwrap().source, 250);
        let mut manual = crate::gl::blank(&c)?;
        manual.date = "2026-10-08".into();
        manual.lines = vec![
            crate::gl::Line {
                account: crate::gl::role(&c, "receivable")?,
                debit: 100,
                ..Default::default()
            },
            crate::gl::Line {
                account: crate::gl::role(&c, "revenue")?,
                credit: 100,
                ..Default::default()
            },
        ];
        crate::gl::save(&mut c, &manual, true)?;
        let report = build(&c, "2026-10-10")?;
        assert_eq!(report.controls[0].difference, Some(100));
        assert!(report.notes.iter().any(|note| note.contains("手工分录")));
        assert!(report
            .csv_rows()
            .iter()
            .all(|row| row.len() == CSV_HEADERS.len()));
        assert!(build(&c, "not-a-date").is_err());
        Ok(())
    }
    #[test]
    fn foreign_receivable_uses_original_book_values_and_tracks_changed_control_accounts(
    ) -> Result<()> {
        let mut c = fixture()?;
        let bank = crate::gl::role(&c, "bank")?;
        crate::ap::configure_bank(&mut c, bank, "USD")?;
        crate::execute(
            &mut c,
            crate::Command::FxSet {
                source: "USD".into(),
                target: "CAD".into(),
                rate: "1.5".into(),
            },
        )?;
        let id = invoice(&mut c, "USD", "USD", 10000, 0)?;
        crate::execute(
            &mut c,
            crate::Command::FxSet {
                source: "USD".into(),
                target: "CAD".into(),
                rate: "2".into(),
            },
        )?;
        crate::billing::add_payment(&mut c, id, "2026-10-05", "40", "Bank transfer", "")?;
        let other = crate::accounting::save_account(&c, 0, "1190", "Other AR", "Asset", true)?;
        crate::gl::configure(&mut c, "CAD", &[("receivable".into(), other)], "")?;
        invoice(&mut c, "CAD2", "CAD", 2000, 0)?;
        crate::execute(
            &mut c,
            crate::Command::FxSet {
                source: "USD".into(),
                target: "CAD".into(),
                rate: "3".into(),
            },
        )?;
        let report = build(&c, "2026-10-10")?;
        assert!(all_matched(&report));
        assert_eq!(
            report
                .currencies
                .iter()
                .find(|balance| balance.currency == "USD")
                .unwrap()
                .remaining,
            Some(6000)
        );
        assert!(report
            .controls
            .iter()
            .any(|control| control.label.starts_with("AR 1100") && control.source == 9000));
        assert!(report
            .controls
            .iter()
            .any(|control| control.label.starts_with("AR 1190") && control.source == 2000));
        crate::gl::configure(&mut c, "CAD", &[("bank".into(), other)], "")?;
        assert!(build(&c, "2026-10-10")?
            .controls
            .iter()
            .filter(|control| control.label.starts_with("AR"))
            .all(|control| control.difference.is_none()));
        Ok(())
    }
    #[test]
    fn incomplete_history_and_non_cad_tax_controls_never_claim_a_match() -> Result<()> {
        let mut c = fixture()?;
        let id = invoice(&mut c, "Legacy", "CAD", 5000, 0)?;
        // A missing historical source cannot be treated as a known opening balance.
        c.execute(
            "DELETE FROM gl_sources WHERE kind='invoice' AND record=?",
            [id],
        )?;
        let report = build(&c, "2026-10-10")?;
        assert!(report
            .controls
            .iter()
            .filter(|control| control.label.starts_with("AR"))
            .all(|control| control.difference.is_none()));
        c.execute(
            "INSERT INTO payments(invoice,amount,method) VALUES(?,100,'Legacy')",
            [id],
        )?;
        assert!(build(&c, "2026-10-10")?.currencies[0].remaining.is_none());
        let mut other = fixture()?;
        crate::gl::configure(&mut other, "USD", &[], "")?;
        crate::execute(
            &mut other,
            crate::Command::FxSet {
                source: "CAD".into(),
                target: "USD".into(),
                rate: "0.75".into(),
            },
        )?;
        invoice(&mut other, "TaxUSD", "CAD", 5000, 1)?;
        let report = build(&other, "2026-10-10")?;
        assert_eq!(report.controls.last().unwrap().source_currency, "CAD");
        assert!(report.controls.last().unwrap().difference.is_none());
        assert_eq!(report.base_currency, "USD");
        Ok(())
    }
    #[test]
    fn backdated_replacement_receipt_flags_negative_historical_ar_even_when_gl_agrees() -> Result<()>
    {
        let mut c = fixture()?;
        let id = invoice(&mut c, "Backdated", "CAD", 5000, 0)?;
        let payment = crate::billing::add_payment(&mut c, id, "2026-10-05", "50", "Cash", "")?;
        crate::billing::void_payment_on(&mut c, payment, "Correction", "2026-10-15")?;
        assert!(crate::billing::add_payment(&mut c, id, "2026-10-06", "50", "Cash", "").is_err());
        // Simulate an existing legacy receipt that predates the new write controls.
        // Historical diagnostics must still surface its negative A/R without rewriting it.
        let tx = c.transaction()?;
        tx.execute("INSERT INTO payments(invoice,date,amount,method,reference) VALUES(?,'2026-10-06',5000,'Cash','')", [id])?;
        let legacy = tx.last_insert_rowid();
        crate::numbering::assign(&tx, "payment", legacy)?;
        tx.execute("UPDATE invoices SET paid=1 WHERE id=?", [id])?;
        crate::gl::payment(&tx, legacy)?;
        tx.commit()?;
        let report = build(&c, "2026-10-10")?;
        assert_eq!(report.currencies[0].remaining, Some(-5000));
        assert!(report
            .controls
            .iter()
            .filter(|control| control.label.starts_with("AR"))
            .all(|control| control.difference.is_none()));
        assert!(report.notes.iter().any(|note| note.contains("负数应收")));
        assert_eq!(crate::billing::balance(&c, id)?.remaining, 0);
        Ok(())
    }
}
