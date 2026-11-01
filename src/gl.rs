use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct Line {
    pub account: i64,
    pub debit: i64,
    pub credit: i64,
    pub description: String,
    pub customer: Option<i64>,
    pub vendor: Option<i64>,
    pub tax_reference: String,
    pub base_debit: i64,
    pub base_credit: i64,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Journal {
    pub id: i64,
    pub number: String,
    pub date: String,
    pub currency: String,
    pub rate: i64,
    pub memo: String,
    pub status: String,
    pub origin: String,
    pub lines: Vec<Line>,
}
pub const ROLES: [(&str, &str, &str, &str); 6] = [
    ("receivable", "1100", "Accounts Receivable", "Asset"),
    ("payable", "2000", "Accounts Payable", "Liability"),
    ("bank", "1010", "Bank - CAD", "Asset"),
    ("revenue", "4000", "Service Revenue", "Income"),
    ("fx", "6300", "Foreign Exchange Gain or Loss", "Expense"),
    ("tax_adjustment", "6400", "Sales Tax Adjustments", "Expense"),
];
pub fn init(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
        CREATE TABLE IF NOT EXISTS gl_settings(id INTEGER PRIMARY KEY CHECK(id=1),base_currency TEXT NOT NULL,locked_through TEXT NOT NULL DEFAULT '',started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        INSERT OR IGNORE INTO gl_settings(id,base_currency) SELECT 1,currency FROM settings WHERE id=1;
        CREATE TABLE IF NOT EXISTS gl_accounts(id INTEGER PRIMARY KEY,role TEXT NOT NULL UNIQUE,account INTEGER NOT NULL REFERENCES accounts(id));
        CREATE TABLE IF NOT EXISTS gl_entries(id INTEGER PRIMARY KEY,number TEXT UNIQUE,date TEXT NOT NULL,currency TEXT NOT NULL,fx_rate INTEGER NOT NULL CHECK(typeof(fx_rate)='integer' AND fx_rate>0),memo TEXT NOT NULL DEFAULT '',status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN('draft','posted')),origin TEXT NOT NULL DEFAULT 'manual',created TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,posted_at TEXT);
        CREATE TABLE IF NOT EXISTS gl_lines(id INTEGER PRIMARY KEY,entry INTEGER NOT NULL REFERENCES gl_entries(id),position INTEGER NOT NULL,account INTEGER NOT NULL REFERENCES accounts(id),account_code TEXT NOT NULL,account_name TEXT NOT NULL,account_kind TEXT NOT NULL,debit INTEGER NOT NULL CHECK(typeof(debit)='integer' AND debit>=0),credit INTEGER NOT NULL CHECK(typeof(credit)='integer' AND credit>=0),base_debit INTEGER NOT NULL CHECK(typeof(base_debit)='integer' AND base_debit>=0),base_credit INTEGER NOT NULL CHECK(typeof(base_credit)='integer' AND base_credit>=0),description TEXT NOT NULL DEFAULT '',customer INTEGER REFERENCES customers(id),vendor INTEGER REFERENCES vendors(id),tax_reference TEXT NOT NULL DEFAULT '',CHECK(debit=0 OR credit=0),CHECK(base_debit=0 OR base_credit=0),CHECK(customer IS NULL OR vendor IS NULL),UNIQUE(entry,position));
        CREATE INDEX IF NOT EXISTS gl_lines_account_idx ON gl_lines(account,entry);
        CREATE INDEX IF NOT EXISTS gl_entries_date_idx ON gl_entries(date,status);
        CREATE TABLE IF NOT EXISTS gl_reversals(id INTEGER PRIMARY KEY,entry INTEGER NOT NULL UNIQUE REFERENCES gl_entries(id),reversal INTEGER NOT NULL UNIQUE REFERENCES gl_entries(id),reason TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS gl_sources(id INTEGER PRIMARY KEY,kind TEXT NOT NULL,record INTEGER NOT NULL,entry INTEGER REFERENCES gl_entries(id),snapshot TEXT NOT NULL,UNIQUE(kind,record));
        CREATE TABLE IF NOT EXISTS gl_attachments(id INTEGER PRIMARY KEY,entry INTEGER NOT NULL REFERENCES gl_entries(id),name TEXT NOT NULL,data BLOB NOT NULL CHECK(length(data)<=20971520));
        CREATE TRIGGER IF NOT EXISTS gl_insert_draft BEFORE INSERT ON gl_entries WHEN NEW.status!='draft' BEGIN SELECT RAISE(ABORT,'Create a draft before posting'); END;
        CREATE TRIGGER IF NOT EXISTS gl_post_guard BEFORE UPDATE OF status ON gl_entries WHEN NEW.status='posted' AND (
            OLD.status!='draft' OR NEW.number IS NULL OR NEW.posted_at IS NULL OR
            NEW.date<=(SELECT locked_through FROM gl_settings WHERE id=1) OR
            NEW.currency NOT IN('CAD','USD','CNY','EUR','GBP','HKD','AUD','SGD') OR
            (NEW.currency=(SELECT base_currency FROM gl_settings WHERE id=1) AND NEW.fx_rate!=100000000) OR
            (SELECT COUNT(*) FROM gl_lines WHERE entry=NEW.id)<2 OR
            (SELECT COALESCE(sum(debit),0) FROM gl_lines WHERE entry=NEW.id)<=0 OR
            (SELECT sum(debit-credit) FROM gl_lines WHERE entry=NEW.id)!=0 OR
            (SELECT sum(base_debit-base_credit) FROM gl_lines WHERE entry=NEW.id)!=0 OR
            EXISTS(SELECT 1 FROM gl_lines l JOIN accounts a ON a.id=l.account WHERE l.entry=NEW.id AND ((a.active=0 AND NEW.origin!='reversal') OR l.account_kind!=a.kind))
        ) BEGIN SELECT RAISE(ABORT,'Journal must have active accounts and balanced debits/credits'); END;
        CREATE TRIGGER IF NOT EXISTS gl_posted_fixed BEFORE UPDATE ON gl_entries WHEN OLD.status='posted' BEGIN SELECT RAISE(ABORT,'Reverse posted journals instead of editing'); END;
        CREATE TRIGGER IF NOT EXISTS gl_posted_no_delete BEFORE DELETE ON gl_entries WHEN OLD.status='posted' BEGIN SELECT RAISE(ABORT,'Posted journals cannot be deleted'); END;
        CREATE TRIGGER IF NOT EXISTS gl_lines_insert_guard BEFORE INSERT ON gl_lines WHEN (SELECT status FROM gl_entries WHERE id=NEW.entry)='posted' BEGIN SELECT RAISE(ABORT,'Posted journal lines are fixed'); END;
        CREATE TRIGGER IF NOT EXISTS gl_lines_update_guard BEFORE UPDATE ON gl_lines WHEN (SELECT status FROM gl_entries WHERE id=OLD.entry)='posted' OR (SELECT status FROM gl_entries WHERE id=NEW.entry)='posted' BEGIN SELECT RAISE(ABORT,'Posted journal lines are fixed'); END;
        CREATE TRIGGER IF NOT EXISTS gl_lines_delete_guard BEFORE DELETE ON gl_lines WHEN (SELECT status FROM gl_entries WHERE id=OLD.entry)='posted' BEGIN SELECT RAISE(ABORT,'Posted journal lines are fixed'); END;
        CREATE TRIGGER IF NOT EXISTS gl_base_fixed BEFORE UPDATE OF base_currency ON gl_settings WHEN EXISTS(SELECT 1 FROM gl_entries WHERE status='posted') AND NEW.base_currency!=OLD.base_currency BEGIN SELECT RAISE(ABORT,'GL base currency is fixed after posting'); END;
        CREATE TRIGGER IF NOT EXISTS gl_reversal_fixed BEFORE UPDATE ON gl_reversals BEGIN SELECT RAISE(ABORT,'Reversals are fixed'); END;
        CREATE TRIGGER IF NOT EXISTS gl_reversal_no_delete BEFORE DELETE ON gl_reversals BEGIN SELECT RAISE(ABORT,'Reversals are fixed'); END;
        CREATE TRIGGER IF NOT EXISTS gl_attachment_insert BEFORE INSERT ON gl_attachments WHEN (SELECT status FROM gl_entries WHERE id=NEW.entry)='posted' BEGIN SELECT RAISE(ABORT,'Posted attachments are fixed'); END;
        CREATE TRIGGER IF NOT EXISTS gl_attachment_update BEFORE UPDATE ON gl_attachments WHEN (SELECT status FROM gl_entries WHERE id=OLD.entry)='posted' OR (SELECT status FROM gl_entries WHERE id=NEW.entry)='posted' BEGIN SELECT RAISE(ABORT,'Posted attachments are fixed'); END;
        CREATE TRIGGER IF NOT EXISTS gl_attachment_delete BEFORE DELETE ON gl_attachments WHEN (SELECT status FROM gl_entries WHERE id=OLD.entry)='posted' BEGIN SELECT RAISE(ABORT,'Posted attachments are fixed'); END;
        CREATE TRIGGER IF NOT EXISTS accounts_gl_kind_fixed BEFORE UPDATE OF kind ON accounts WHEN NEW.kind!=OLD.kind AND EXISTS(SELECT 1 FROM gl_lines l JOIN gl_entries e ON e.id=l.entry WHERE l.account=OLD.id AND e.status='posted') BEGIN SELECT RAISE(ABORT,'Posted GL account type is fixed'); END;
        CREATE TRIGGER IF NOT EXISTS accounts_gl_mapping_guard BEFORE UPDATE OF kind,active ON accounts WHEN (NEW.kind!=OLD.kind OR NEW.active=0) AND EXISTS(SELECT 1 FROM gl_accounts WHERE account=OLD.id) BEGIN SELECT RAISE(ABORT,'Change GL account mapping before deactivating or changing type'); END;
    "#)?;
    for (role, code, name, kind) in ROLES {
        if c.query_row(
            "SELECT EXISTS(SELECT 1 FROM gl_accounts WHERE role=?)",
            [role],
            |r| r.get::<_, bool>(0),
        )? {
            continue;
        }
        let existing=c.query_row("SELECT id FROM accounts WHERE kind=?1 AND active=1 AND (code=?2 OR name=?3) ORDER BY (name=?3) DESC,id LIMIT 1",params![kind,code,name],|r|r.get::<_,i64>(0)).optional()?;
        let account = if let Some(id) = existing {
            id
        } else {
            let mut code = code.to_owned();
            while c.query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE code=?)",
                [&code],
                |r| r.get::<_, bool>(0),
            )? {
                code.push_str("-GL");
            }
            c.execute(
                "INSERT INTO accounts(code,name,kind) VALUES(?,?,?)",
                params![code, name, kind],
            )?;
            c.last_insert_rowid()
        };
        c.execute(
            "INSERT INTO gl_accounts(role,account) VALUES(?,?)",
            params![role, account],
        )?;
    }
    Ok(())
}
pub fn base(c: &Connection) -> Result<String> {
    Ok(c.query_row(
        "SELECT base_currency FROM gl_settings WHERE id=1",
        [],
        |r| r.get(0),
    )?)
}
pub fn role(c: &Connection, name: &str) -> Result<i64> {
    let kind = ROLES
        .iter()
        .find(|r| r.0 == name)
        .context("GL role unknown")?
        .3;
    c.query_row("SELECT a.id FROM gl_accounts g JOIN accounts a ON a.id=g.account WHERE role=? AND a.kind=? AND a.active=1",params![name,kind],|r|r.get(0)).context(format!("请在 GL Setup 配置启用的 {name} 科目"))
}
pub fn configure(
    c: &mut Connection,
    currency: &str,
    roles: &[(String, i64)],
    locked: &str,
) -> Result<()> {
    let currency = crate::currency(currency).map_err(anyhow::Error::msg)?;
    if !locked.is_empty() {
        crate::task_date(locked).map_err(anyhow::Error::msg)?;
    }
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute(
        "UPDATE gl_settings SET base_currency=?,locked_through=? WHERE id=1",
        params![currency, locked],
    )?;
    for (name, id) in roles {
        let kind = ROLES
            .iter()
            .find(|r| r.0 == name)
            .context("GL role unknown")?
            .3;
        ensure!(
            tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=? AND active=1 AND kind=?)",
                params![id, kind],
                |r| r.get::<_, bool>(0)
            )?,
            "GL 科目类型或启用状态无效"
        );
        tx.execute(
            "UPDATE gl_accounts SET account=? WHERE role=?",
            params![id, name],
        )?;
    }
    tx.commit()?;
    Ok(())
}
pub fn quote(c: &Connection, currency: &str, date: &str) -> Result<i64> {
    crate::fx::dated(c, currency, &base(c)?, date)
}
pub fn blank(c: &Connection) -> Result<Journal> {
    Ok(Journal {
        id: 0,
        number: String::new(),
        date: crate::today().to_string(),
        currency: base(c)?,
        rate: crate::FX_SCALE,
        memo: String::new(),
        status: "draft".into(),
        origin: "manual".into(),
        lines: vec![Line::default(), Line::default()],
    })
}
pub fn load(c: &Connection, id: i64) -> Result<Journal> {
    let mut j=c.query_row("SELECT id,COALESCE(number,''),date,currency,fx_rate,memo,status,origin FROM gl_entries WHERE id=?",[id],|r|Ok(Journal{id:r.get(0)?,number:r.get(1)?,date:r.get(2)?,currency:r.get(3)?,rate:r.get(4)?,memo:r.get(5)?,status:r.get(6)?,origin:r.get(7)?,lines:Vec::new()}))?;
    let mut stmt=c.prepare("SELECT account,debit,credit,description,customer,vendor,tax_reference,base_debit,base_credit FROM gl_lines WHERE entry=? ORDER BY position")?;
    j.lines = stmt
        .query_map([id], |r| {
            Ok(Line {
                account: r.get(0)?,
                debit: r.get(1)?,
                credit: r.get(2)?,
                description: r.get(3)?,
                customer: r.get(4)?,
                vendor: r.get(5)?,
                tax_reference: r.get(6)?,
                base_debit: r.get(7)?,
                base_credit: r.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(j)
}
pub fn totals(lines: &[Line]) -> Result<(i64, i64)> {
    lines.iter().try_fold((0i64, 0i64), |(d, c), l| {
        Ok((
            d.checked_add(l.debit).context("借方合计过大")?,
            c.checked_add(l.credit).context("贷方合计过大")?,
        ))
    })
}
fn prepare(c: &Connection, j: &Journal, posting: bool, fixed_base: bool) -> Result<Vec<Line>> {
    crate::task_date(&j.date).map_err(anyhow::Error::msg)?;
    crate::currency(&j.currency).map_err(anyhow::Error::msg)?;
    ensure!(
        j.rate > 0 && (j.currency != base(c)? || j.rate == crate::FX_SCALE),
        "本币汇率必须为 1，外币汇率须大于零"
    );
    let mut lines: Vec<_> = j
        .lines
        .iter()
        .filter(|l| {
            l.account != 0
                || l.debit != 0
                || l.credit != 0
                || !l.description.trim().is_empty()
                || l.customer.is_some()
                || l.vendor.is_some()
                || !l.tax_reference.is_empty()
        })
        .cloned()
        .collect();
    for line in &mut lines {
        ensure!(
            line.debit >= 0 && line.credit >= 0 && (line.debit == 0 || line.credit == 0),
            "每行只能填写借方或贷方，金额不可为负"
        );
        ensure!(
            line.customer.is_none() || line.vendor.is_none(),
            "每行仅能关联一个客户或供应商"
        );
        if posting {
            ensure!(
                line.account > 0
                    && (line.debit > 0
                        || line.credit > 0
                        || (fixed_base && (line.base_debit > 0 || line.base_credit > 0))),
                "过账时每行必须有科目及金额"
            );
        }
        if line.account > 0 {
            ensure!(
                c.query_row(
                    "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=? AND (?=0 OR active=1))",
                    params![line.account, posting && j.origin != "reversal"],
                    |r| r.get::<_, bool>(0)
                )?,
                "科目不存在或已停用"
            );
        } else {
            ensure!(!posting, "请选择科目");
        }
        if !fixed_base {
            line.base_debit = crate::convert(line.debit, j.rate)?;
            line.base_credit = crate::convert(line.credit, j.rate)?;
        }
    }
    let (debit, credit) = totals(&lines)?;
    if posting {
        let locked: String = c.query_row(
            "SELECT locked_through FROM gl_settings WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        ensure!(
            j.date > locked,
            "分录日期属于已锁定期间，请在 GL Setup 检查截止日期"
        );
        ensure!(
            lines.len() >= 2 && debit > 0 && debit == credit,
            "借贷必须相等且大于零，至少填写两行"
        );
    }
    if !fixed_base {
        for side in [true, false] {
            let amount = if side { debit } else { credit };
            let target = crate::convert(amount, j.rate)?;
            let sum = lines.iter().try_fold(0i64, |sum, l| {
                sum.checked_add(if side { l.base_debit } else { l.base_credit })
                    .context("本币合计过大")
            })?;
            let preferred = if !side && j.origin == "invoice" {
                Some(role(c, "revenue")?)
            } else {
                None
            };
            if let Some(line) = lines
                .iter_mut()
                .filter(|l| {
                    (if side { l.debit > 0 } else { l.credit > 0 })
                        && preferred.is_none_or(|account| l.account == account)
                })
                .max_by_key(|l| if side { l.debit } else { l.credit })
            {
                let value = if side {
                    &mut line.base_debit
                } else {
                    &mut line.base_credit
                };
                *value = value.checked_add(target - sum).context("换算尾差过大")?;
                ensure!(*value >= 0, "换算金额无效");
            }
        }
    }
    if posting {
        ensure!(
            lines
                .iter()
                .map(|l| i128::from(l.base_debit) - i128::from(l.base_credit))
                .sum::<i128>()
                == 0,
            "本币借贷不平衡"
        );
    }
    Ok(lines)
}
pub fn validate_fixed(c: &Connection, j: &Journal) -> Result<()> {
    prepare(c, j, true, true)?;
    Ok(())
}
pub fn vendor_payment(c: &Connection, id: i64, j: Journal) -> Result<()> {
    ensure!(
        source(c, "vendor_payment", id)?.is_none(),
        "Supplier payment already posted."
    );
    auto(c, "vendor_payment", id, j)
}
pub fn preview_posting(c: &Connection, j: &Journal) -> Result<Vec<Line>> {
    prepare(c, j, true, false)
}
pub fn preview_lines(c: &Connection, j: &Journal) -> Result<Vec<Line>> {
    prepare(c, j, false, false)
}
fn write(c: &Connection, j: &Journal, posting: bool, fixed_base: bool) -> Result<i64> {
    let lines = prepare(c, j, posting, fixed_base)?;
    let id = if j.id == 0 {
        c.execute(
            "INSERT INTO gl_entries(date,currency,fx_rate,memo,origin) VALUES(?,?,?,?,?)",
            params![j.date, j.currency, j.rate, j.memo, j.origin],
        )?;
        let id = c.last_insert_rowid();
        crate::numbering::assign(c, "journal", id)?;
        id
    } else {
        ensure!(c.execute("UPDATE gl_entries SET date=?,currency=?,fx_rate=?,memo=? WHERE id=? AND status='draft' AND origin='manual'",params![j.date,j.currency,j.rate,j.memo,j.id])?==1,"仅能编辑手工草稿，已过账分录请冲销");
        c.execute("DELETE FROM gl_lines WHERE entry=?", [j.id])?;
        j.id
    };
    for (i, line) in lines.iter().enumerate() {
        // Incomplete draft rows with no account remain in the saved form via a
        // placeholder account only after the user selects one; never guess it.
        ensure!(line.account > 0, "保存前请为已填写的行选择科目");
        c.execute("INSERT INTO gl_lines(entry,position,account,account_code,account_name,account_kind,debit,credit,base_debit,base_credit,description,customer,vendor,tax_reference) SELECT ?1,?2,id,code,name,kind,?3,?4,?5,?6,?7,?8,?9,?10 FROM accounts WHERE id=?11",params![id,(i+1) as i64,line.debit,line.credit,line.base_debit,line.base_credit,line.description,line.customer,line.vendor,line.tax_reference,line.account])?;
    }
    if posting {
        c.execute(
            "UPDATE gl_entries SET status='posted',posted_at=CURRENT_TIMESTAMP WHERE id=?",
            [id],
        )?;
    }
    Ok(id)
}
pub fn save(c: &mut Connection, j: &Journal, posting: bool) -> Result<i64> {
    save_with_files(c, j, posting, &[])
}
pub fn save_with_files(
    c: &mut Connection,
    j: &Journal,
    posting: bool,
    files: &[(String, Vec<u8>)],
) -> Result<i64> {
    ensure!(j.origin == "manual", "自动分录请通过来源单据处理");
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    // Validate posting before allocating a number or saving any attachments.
    prepare(&tx, j, posting, false)?;
    let id = write(&tx, j, false, false)?;
    for (name, data) in files {
        crate::required(name)?;
        ensure!(data.len() <= 20 * 1024 * 1024, "每个附件最多 20 MB");
        tx.execute(
            "INSERT INTO gl_attachments(entry,name,data) VALUES(?,?,?)",
            params![id, name, data],
        )?;
        let file = tx.last_insert_rowid();
        tx.execute("INSERT INTO audit_log(entity,record_id,action,after_json) VALUES('gl_attachments',?,'INSERT',?)",params![file,serde_json::json!({"entry":id,"name":name,"bytes":data.len()}).to_string()])?;
    }
    if posting {
        let saved = load(&tx, id)?;
        write(&tx, &saved, true, false)?;
    }
    tx.commit()?;
    Ok(id)
}
fn reverse_on(c: &Connection, id: i64, date: &str, reason: &str) -> Result<i64> {
    crate::required(reason)?;
    crate::task_date(date).map_err(anyhow::Error::msg)?;
    let mut j = load(c, id)?;
    ensure!(j.status == "posted", "只能冲销已过账分录");
    ensure!(date >= j.date.as_str(), "冲销日期不可早于原分录");
    ensure!(
        !c.query_row(
            "SELECT EXISTS(SELECT 1 FROM gl_reversals WHERE entry=?)",
            [id],
            |r| r.get::<_, bool>(0)
        )?,
        "分录已经冲销"
    );
    j.id = 0;
    j.date = date.into();
    j.memo = format!("Reverse {}: {reason}", j.number);
    j.origin = "reversal".into();
    for line in &mut j.lines {
        std::mem::swap(&mut line.debit, &mut line.credit);
        std::mem::swap(&mut line.base_debit, &mut line.base_credit);
    }
    let reversed = write(c, &j, true, true)?;
    c.execute(
        "INSERT INTO gl_reversals(entry,reversal,reason) VALUES(?,?,?)",
        params![id, reversed, reason],
    )?;
    Ok(reversed)
}
pub fn reverse(c: &mut Connection, id: i64, date: &str, reason: &str) -> Result<i64> {
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    ensure!(
        load(&tx, id)?.origin == "manual",
        "自动分录请通过来源单据更正或撤销"
    );
    let id = reverse_on(&tx, id, date, reason)?;
    tx.commit()?;
    Ok(id)
}
fn source(c: &Connection, kind: &str, id: i64) -> Result<Option<(Option<i64>, Value)>> {
    let row = c
        .query_row(
            "SELECT entry,snapshot FROM gl_sources WHERE kind=? AND record=?",
            params![kind, id],
            |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?;
    row.map(|(id, s)| Ok((id, serde_json::from_str(&s)?)))
        .transpose()
}
fn auto(c: &Connection, kind: &str, id: i64, mut j: Journal) -> Result<()> {
    j.id = 0;
    j.number.clear();
    j.status = "draft".into();
    j.origin = kind.into();
    let data = serde_json::to_value(&j)?;
    if let Some((old, expected)) = source(c, kind, id)? {
        if expected == data {
            return Ok(());
        }
        if let Some(old) = old {
            let date = load(c, old)?.date;
            reverse_on(c, old, &date, "Source revised")?;
        }
    }
    let entry = if totals(&j.lines)?.0 == 0 {
        None
    } else {
        Some(write(c, &j, true, true)?)
    };
    c.execute("INSERT INTO gl_sources(kind,record,entry,snapshot) VALUES(?,?,?,?) ON CONFLICT(kind,record) DO UPDATE SET entry=excluded.entry,snapshot=excluded.snapshot",params![kind,id,entry,data.to_string()])?;
    Ok(())
}
pub fn void_source(c: &Connection, kind: &str, id: i64, date: &str, reason: &str) -> Result<()> {
    if let Some((Some(entry), _)) = source(c, kind, id)? {
        if !c.query_row(
            "SELECT EXISTS(SELECT 1 FROM gl_reversals WHERE entry=?)",
            [entry],
            |r| r.get::<_, bool>(0),
        )? {
            let date = date.max(load(c, entry)?.date.as_str()).to_owned();
            reverse_on(c, entry, &date, reason)?;
        }
    }
    Ok(())
}
fn line(account: i64, debit: i64, credit: i64, description: &str) -> Line {
    Line {
        account,
        debit,
        credit,
        description: description.into(),
        ..Default::default()
    }
}
pub fn invoice_plan(c: &Connection, data: &Value) -> Result<Journal> {
    let mut j = blank(c)?;
    j.date = data["invoice_date"]
        .as_str()
        .context("缺少开票日期")?
        .into();
    j.currency = data["currency"].as_str().context("缺少币种")?.into();
    j.origin = "invoice".into();
    j.memo = format!("Invoice {}", data["number"].as_str().unwrap_or("Preview"));
    let total = data["total"].as_i64().context("缺少总额")?;
    if total == 0 {
        j.lines.clear();
        return Ok(j);
    }
    j.rate = quote(c, &j.currency, &j.date)?;
    let mut ar = line(role(c, "receivable")?, total, 0, "Accounts receivable");
    ar.customer = data["customer_id"].as_i64();
    j.lines = vec![ar];
    for item in data["items"].as_array().context("缺少明细")? {
        let net = item["subtotal"].as_i64().context("金额无效")?;
        if net > 0 {
            j.lines.push(line(
                role(c, "revenue")?,
                0,
                net,
                item["description"].as_str().unwrap_or("Service"),
            ));
        }
        for part in item["tax_snapshot"]["parts"]
            .as_array()
            .context("缺少税额快照")?
        {
            let amount = part["amount"].as_i64().context("税额无效")?;
            if amount > 0 {
                j.lines.push(line(
                    part["payable_account"].as_i64().context("缺少税务科目")?,
                    0,
                    amount,
                    part["name"].as_str().unwrap_or("Tax"),
                ));
            }
        }
    }
    j.lines = prepare(c, &j, true, false)?;
    Ok(j)
}
pub fn invoice(c: &Connection, id: i64, data: &Value) -> Result<()> {
    auto(c, "invoice", id, invoice_plan(c, data)?)
}
pub fn payment(c: &Connection, id: i64) -> Result<()> {
    let (invoice, date, amount): (i64, String, i64) = c.query_row(
        "SELECT invoice,date,amount FROM payments WHERE id=?",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let raw: String = c.query_row("SELECT snapshot FROM invoices WHERE id=?", [invoice], |r| {
        r.get(0)
    })?;
    let data: Value = serde_json::from_str(&raw)?;
    let mut j = blank(c)?;
    j.date = date;
    j.currency = data["currency"].as_str().context("缺少币种")?.into();
    j.rate = quote(c, &j.currency, &j.date)?;
    j.origin = "payment".into();
    j.memo = format!(
        "Payment for {}",
        data["number"].as_str().unwrap_or("Legacy invoice")
    );
    let (ar, ar_base) = if let Some((Some(entry), _)) = source(c, "invoice", invoice)? {
        let invoice_j = load(c, entry)?;
        let ar = &invoice_j.lines[0];
        let received:i64=c.query_row("SELECT COALESCE(sum(l.base_credit),0) FROM payments p JOIN gl_sources s ON s.kind='payment' AND s.record=p.id JOIN gl_lines l ON l.entry=s.entry WHERE p.invoice=? AND p.voided=0 AND p.id!=? AND l.account=?",params![invoice,id,ar.account],|r|r.get(0))?;
        let rem_base = ar.base_debit - received;
        let rem:i64=c.query_row("SELECT json_extract(snapshot,'$.total')-(SELECT COALESCE(sum(amount),0) FROM payments WHERE invoice=?1 AND voided=0 AND id!=?2) FROM invoices WHERE id=?1",params![invoice,id],|r|r.get(0))?;
        ensure!(rem > 0 && rem_base >= 0, "应收余额无效");
        let portion = i64::try_from(
            (i128::from(rem_base) * i128::from(amount) + i128::from(rem / 2)) / i128::from(rem),
        )?;
        (ar.account, portion)
    } else {
        (role(c, "receivable")?, crate::convert(amount, j.rate)?)
    };
    let bank_base = crate::convert(amount, j.rate)?;
    let mut bank = line(role(c, "bank")?, amount, 0, "Receipt");
    bank.base_debit = bank_base;
    let mut receivable = line(ar, 0, amount, "Settle receivable");
    receivable.base_credit = ar_base;
    receivable.customer = data["customer_id"].as_i64();
    j.lines = vec![bank, receivable];
    if bank_base != ar_base {
        let delta = bank_base - ar_base;
        let mut fx = line(role(c, "fx")?, 0, 0, "Realized exchange difference");
        if delta > 0 {
            fx.base_credit = delta;
        } else {
            fx.base_debit = -delta;
        }
        j.lines.push(fx);
    }
    auto(c, "payment", id, j)
}
pub fn expense(c: &Connection, id: i64, is_new: bool) -> Result<()> {
    if !is_new {
        crate::ap::allow_revision(c, id)?;
    }
    let existing = source(c, "expense", id)?;
    let doc = crate::expense_doc::load(c, id)?;
    let locked: String = c.query_row(
        "SELECT locked_through FROM gl_settings WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    ensure!(doc.date > locked, "费用日期属于已锁定期间");
    if !is_new && existing.is_none() {
        return Ok(());
    }
    let mut j = blank(c)?;
    j.date = doc.date;
    j.currency = doc.currency;
    let number: String = c.query_row(
        "SELECT COALESCE(number,'') FROM expense_documents WHERE id=?",
        [id],
        |r| r.get(0),
    )?;
    j.memo = format!("Expense {number} · {} · {}", doc.reference, doc.memo);
    j.origin = "expense".into();
    j.rate = if let Some((Some(entry), _)) = &existing {
        let old = load(c, *entry)?;
        if old.currency == j.currency && old.date == j.date {
            old.rate
        } else {
            quote(c, &j.currency, &j.date)?
        }
    } else {
        quote(c, &j.currency, &j.date)?
    };
    j.lines.clear();
    let mut gross = 0i64;
    let mut recoverable_base = Vec::new();
    for draft in doc.lines {
        let tax_raw: String = c.query_row(
            "SELECT snapshot FROM expense_tax WHERE expense=?",
            [draft.id],
            |r| r.get(0),
        )?;
        let tax: Value = serde_json::from_str(&tax_raw)?;
        let cost = tax["cost"].as_i64().context("费用成本无效")?;
        gross = gross
            .checked_add(tax["gross"].as_i64().context("费用总额无效")?)
            .context("费用合计过大")?;
        let mut expense = line(draft.account, cost, 0, &draft.description);
        expense.vendor = doc.vendor;
        j.lines.push(expense);
        for part in tax["parts"].as_array().context("费用税额无效")? {
            let recovery = part["recoverable"].as_i64().context("抵扣无效")?;
            if recovery > 0 {
                if base(c)? == "CAD" {
                    let amount:i64=c.query_row("SELECT COALESCE(sum(cad_recoverable),0) FROM tax_postings WHERE source='expense' AND document=? AND json_extract(snapshot,'$.item')=?",params![draft.id,part["item"].as_i64()],|r|r.get(0))?;
                    recoverable_base.push((j.lines.len(), amount));
                }
                j.lines.push(line(
                    part["recoverable_account"]
                        .as_i64()
                        .context("缺少抵扣科目")?,
                    recovery,
                    0,
                    part["name"].as_str().unwrap_or("ITC"),
                ));
            }
        }
    }
    let ap = existing
        .as_ref()
        .and_then(|(_, s)| s["lines"].as_array()?.last()?["account"].as_i64())
        .unwrap_or(role(c, "payable")?);
    let mut payable = line(ap, 0, gross, "Accounts payable");
    payable.vendor = doc.vendor;
    j.lines.push(payable);
    j.lines = prepare(c, &j, true, false)?;
    for (index, amount) in recoverable_base {
        j.lines[index].base_debit = amount;
    }
    let delta = j
        .lines
        .iter()
        .map(|l| i128::from(l.base_debit) - i128::from(l.base_credit))
        .sum::<i128>();
    if delta != 0 {
        let mut adjustment = line(
            role(c, "fx")?,
            0,
            0,
            "Tax translation and rounding adjustment",
        );
        if delta > 0 {
            adjustment.base_credit = delta.try_into()?;
        } else {
            adjustment.base_debit = (-delta).try_into()?;
        }
        j.lines.insert(j.lines.len() - 1, adjustment);
    }
    auto(c, "expense", id, j)?;
    crate::ap::register(c, id, is_new)
}
pub fn tax_cash(c: &Connection, id: i64) -> Result<()> {
    let (agency, date, kind, amount, reason): (i64, String, String, i64, String) = c.query_row(
        "SELECT agency,date,kind,amount,reason FROM tax_cash WHERE id=?",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;
    let payable: i64 = c.query_row(
        "SELECT payable_account FROM tax_agencies WHERE id=?",
        [agency],
        |r| r.get(0),
    )?;
    let mut j = blank(c)?;
    j.currency = "CAD".into();
    j.date = date;
    j.rate = quote(c, "CAD", &j.date)?;
    j.memo = reason;
    j.origin = "tax".into();
    let opposite = role(
        c,
        if kind == "payment" || kind == "refund" {
            "bank"
        } else {
            "tax_adjustment"
        },
    )?;
    j.lines = if kind == "payment" || kind == "decrease" {
        vec![
            line(payable, amount, 0, "Tax liability decrease"),
            line(opposite, 0, amount, "Tax settlement"),
        ]
    } else {
        vec![
            line(opposite, amount, 0, "Tax settlement"),
            line(payable, 0, amount, "Tax liability increase"),
        ]
    };
    j.lines = prepare(c, &j, true, false)?;
    auto(c, "tax", id, j)
}
pub fn validate(c: &Connection) -> Result<()> {
    let base = base(c)?;
    crate::currency(&base).map_err(anyhow::Error::msg)?;
    let mut entries = c.prepare("SELECT id FROM gl_entries")?;
    for id in entries.query_map([], |r| r.get::<_, i64>(0))? {
        let j = load(c, id?)?;
        crate::task_date(&j.date).map_err(anyhow::Error::msg)?;
        crate::currency(&j.currency).map_err(anyhow::Error::msg)?;
        ensure!(
            j.rate > 0 && (j.currency != base || j.rate == crate::FX_SCALE),
            "GL 汇率无效"
        );
        if j.status != "posted" {
            continue;
        }
        if j.origin == "manual" {
            ensure!(
                prepare(c, &j, false, false)? == j.lines,
                "手工分录本币金额与汇率不一致"
            );
        }
        let (d, cr) = totals(&j.lines)?;
        ensure!(
            j.lines.len() >= 2
                && d > 0
                && d == cr
                && j.lines
                    .iter()
                    .map(|l| i128::from(l.base_debit) - i128::from(l.base_credit))
                    .sum::<i128>()
                    == 0,
            "GL 过账分录借贷不平衡"
        );
    }
    let invalid:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM gl_sources s LEFT JOIN gl_entries e ON e.id=s.entry WHERE s.entry IS NOT NULL AND (e.status IS NOT 'posted' OR e.origin!=s.kind)) OR EXISTS(SELECT 1 FROM gl_reversals r JOIN gl_entries a ON a.id=r.entry JOIN gl_entries b ON b.id=r.reversal WHERE a.status!='posted' OR b.status!='posted')",[],|r|r.get(0))?;
    ensure!(!invalid, "GL 来源或冲销关联损坏");
    let mut sources = c.prepare("SELECT entry,snapshot FROM gl_sources WHERE entry IS NOT NULL")?;
    for row in sources.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (id, raw) = row?;
        let expected: Journal = serde_json::from_str(&raw)?;
        let mut actual = load(c, id)?;
        actual.id = 0;
        actual.number.clear();
        actual.status = "draft".into();
        ensure!(actual == expected, "GL 来源快照与过账分录不一致");
    }
    let mut reversals = c.prepare("SELECT entry,reversal FROM gl_reversals")?;
    for row in reversals.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
        let (id, reverse) = row?;
        let mut original = load(c, id)?;
        let reversed = load(c, reverse)?;
        for l in &mut original.lines {
            std::mem::swap(&mut l.debit, &mut l.credit);
            std::mem::swap(&mut l.base_debit, &mut l.base_credit);
        }
        ensure!(
            original.currency == reversed.currency
                && original.rate == reversed.rate
                && original.lines == reversed.lines
                && reversed.date >= original.date,
            "GL 冲销金额或汇率不一致"
        );
    }
    Ok(())
}
pub fn trial_balance(c: &Connection, as_of: &str) -> Result<Vec<Vec<String>>> {
    crate::task_date(as_of).map_err(anyhow::Error::msg)?;
    let mut stmt=c.prepare("SELECT a.code,a.name,a.kind,COALESCE(sum(CASE WHEN e.status='posted' AND e.date<=? THEN l.base_debit-l.base_credit ELSE 0 END),0) FROM accounts a LEFT JOIN gl_lines l ON l.account=a.id LEFT JOIN gl_entries e ON e.id=l.entry GROUP BY a.id ORDER BY a.code,a.id")?;
    let rows = stmt
        .query_map([as_of], |r| {
            let v: i64 = r.get(3)?;
            Ok(vec![
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                crate::format_money(v.max(0)),
                crate::format_money(
                    (-i128::from(v))
                        .max(0)
                        .try_into()
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                ),
            ])
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
pub fn ledger(
    c: &Connection,
    from: &str,
    to: &str,
    account: i64,
) -> Result<(i64, Vec<Vec<String>>, i64)> {
    crate::task_date(from).map_err(anyhow::Error::msg)?;
    crate::task_date(to).map_err(anyhow::Error::msg)?;
    ensure!(from <= to, "开始日期不能晚于结束日期");
    let mut balances = std::collections::BTreeMap::new();
    let mut opening=c.prepare("SELECT l.account,sum(l.base_debit-l.base_credit) FROM gl_lines l JOIN gl_entries e ON e.id=l.entry WHERE e.status='posted' AND e.date<? AND (?=0 OR l.account=?) GROUP BY l.account")?;
    for row in opening.query_map(params![from, account, account], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
    })? {
        let (account, balance) = row?;
        balances.insert(account, balance);
    }
    let start = balances.get(&account).copied().unwrap_or(0);
    let mut stmt=c.prepare("SELECT e.id,e.number,e.date,e.origin,l.account_code || ' · ' || l.account_name,l.description,e.currency,l.debit,l.credit,l.base_debit,l.base_credit,l.account FROM gl_lines l JOIN gl_entries e ON e.id=l.entry WHERE e.status='posted' AND e.date BETWEEN ?1 AND ?2 AND (?3=0 OR l.account=?3) ORDER BY e.date,e.id,l.position")?;
    let mut result = Vec::new();
    let mut rows = stmt.query(params![from, to, account])?;
    while let Some(r) = rows.next()? {
        let debit: i64 = r.get(9)?;
        let credit: i64 = r.get(10)?;
        let account: i64 = r.get(11)?;
        let balance = balances.entry(account).or_insert(0i64);
        *balance = balance
            .checked_add(debit - credit)
            .context("科目余额过大")?;
        result.push(vec![
            r.get::<_, i64>(0)?.to_string(),
            r.get(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
            r.get(6)?,
            crate::format_money(r.get(7)?),
            crate::format_money(r.get(8)?),
            crate::format_money(debit),
            crate::format_money(credit),
            crate::format_money(*balance),
        ]);
    }
    Ok((start, result, balances.get(&account).copied().unwrap_or(0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Result<Connection> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer',''); INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor',''); INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','item',0,10000,'CAD'),(2,1,'USD Service','item',0,10000,'USD');")?;
        Ok(c)
    }
    fn manual(c: &Connection) -> Result<Journal> {
        let mut j = blank(c)?;
        j.memo = "Opening balance".into();
        j.lines = vec![
            line(role(c, "bank")?, 10000, 0, "Bank opening"),
            line(role(c, "revenue")?, 0, 10000, "Revenue"),
        ];
        Ok(j)
    }
    fn invoice(c: &mut Connection, price: i64, currency: &str) -> Result<i64> {
        let t = crate::add_task(c, "Task", 1, currency)?;
        crate::add_item(c, t, price, 1, None, None)?;
        crate::set_task_status(c, t, false)?;
        crate::billing::merge(c, &[t])
    }
    fn amount(c: &Connection, account: i64) -> Result<i64> {
        Ok(c.query_row("SELECT COALESCE(sum(base_debit-base_credit),0) FROM gl_lines l JOIN gl_entries e ON e.id=l.entry WHERE account=? AND e.status='posted'",[account],|r|r.get(0))?)
    }
    #[test]
    fn drafts_posting_locks_reversals_and_attachments_are_atomic() -> Result<()> {
        let mut c = db()?;
        let mut j = manual(&c)?;
        j.lines[1].credit = 9000;
        assert!(save(&mut c, &j, true).is_err());
        assert_eq!(
            c.query_row(
                "SELECT next_value FROM number_sequences WHERE kind='journal'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        let id = save(&mut c, &j, false)?;
        assert_eq!(amount(&c, role(&c, "bank")?)?, 0);
        j = load(&c, id)?;
        j.lines[1].credit = 10000;
        save_with_files(&mut c, &j, true, &[("proof.txt".into(), b"proof".to_vec())])?;
        assert_eq!(amount(&c, role(&c, "bank")?)?, 10000);
        assert!(c
            .execute("UPDATE gl_lines SET debit=1 WHERE entry=?", [id])
            .is_err());
        assert!(c
            .execute("DELETE FROM gl_entries WHERE id=?", [id])
            .is_err());
        assert!(c.execute("INSERT INTO gl_entries(date,currency,fx_rate,status) VALUES('2026-10-26','CAD',100000000,'posted')",[]).is_err());
        assert!(c
            .execute("DELETE FROM gl_attachments WHERE entry=?", [id])
            .is_err());
        assert!(configure(&mut c, "USD", &[], "").is_err());
        let reverse = super::reverse(&mut c, id, &j.date, "Correction")?;
        assert_eq!(load(&c, reverse)?.origin, "reversal");
        assert_eq!(amount(&c, role(&c, "bank")?)?, 0);
        assert!(super::reverse(&mut c, id, &j.date, "Again").is_err());
        assert_eq!(
            c.query_row("SELECT data FROM gl_attachments WHERE entry=?", [id], |r| r
                .get::<_, Vec<u8>>(0))?,
            b"proof"
        );
        validate(&c)?;
        Ok(())
    }
    #[test]
    fn foreign_rounding_balances_both_currencies_and_reverses_exactly() -> Result<()> {
        let mut c = db()?;
        let mut j = manual(&c)?;
        j.currency = "USD".into();
        j.rate = 50_000_000;
        j.lines = vec![
            line(role(&c, "bank")?, 1, 0, "One"),
            line(role(&c, "bank")?, 1, 0, "Two"),
            line(role(&c, "revenue")?, 0, 2, "Total"),
        ];
        let id = save(&mut c, &j, true)?;
        let posted = load(&c, id)?;
        assert_eq!(posted.lines.iter().map(|l| l.base_debit).sum::<i64>(), 1);
        assert_eq!(posted.lines.iter().map(|l| l.base_credit).sum::<i64>(), 1);
        super::reverse(&mut c, id, &j.date, "Rounding correction")?;
        assert_eq!(amount(&c, role(&c, "bank")?)?, 0);
        validate(&c)?;
        Ok(())
    }
    #[test]
    fn invoices_receipts_and_voids_reconcile_with_subledger() -> Result<()> {
        let mut c = db()?;
        c.execute_batch("UPDATE tax_agencies SET registered=1,registration='TEST';UPDATE prices SET tax_code=3 WHERE id=1;")?;
        let inv = invoice(&mut c, 1, "CAD")?;
        let ar = role(&c, "receivable")?;
        assert_eq!(amount(&c, ar)?, 11200);
        let payment = crate::billing::add_payment(
            &mut c,
            inv,
            &crate::today().to_string(),
            "12",
            "Cash",
            "",
        )?;
        assert_eq!(amount(&c, ar)?, 10000);
        assert_eq!(amount(&c, role(&c, "bank")?)?, 1200);
        crate::billing::void_payment(&mut c, payment, "Correction")?;
        assert_eq!(amount(&c, ar)?, 11200);
        assert_eq!(amount(&c, role(&c, "bank")?)?, 0);
        let entry = source(&c, "invoice", inv)?.unwrap().0.unwrap();
        assert!(super::reverse(
            &mut c,
            entry,
            &crate::today().to_string(),
            "Direct reversal"
        )
        .is_err());
        validate(&c)?;
        crate::storage::validate_projections(&c)?;
        Ok(())
    }
    #[test]
    fn foreign_receipts_allocate_all_ar_and_realize_exchange_differences() -> Result<()> {
        let mut c = db()?;
        let bank = crate::gl::role(&c, "bank")?;
        crate::ap::configure_bank(&mut c, bank, "USD")?;
        c.execute_batch("INSERT INTO fx_rates(source,target,rate) VALUES('USD','CAD',50000000);UPDATE prices SET price=3 WHERE id=2;")?;
        let inv = invoice(&mut c, 2, "USD")?;
        let ar = role(&c, "receivable")?;
        assert_eq!(amount(&c, ar)?, 2);
        let mut ids = Vec::new();
        for _ in 0..3 {
            ids.push(crate::billing::add_payment(
                &mut c,
                inv,
                &crate::today().to_string(),
                "0.01",
                "Bank",
                "",
            )?);
        }
        assert_eq!(amount(&c, ar)?, 0);
        assert_eq!(amount(&c, role(&c, "bank")?)?, 3);
        assert_eq!(amount(&c, role(&c, "fx")?)?, -1);
        crate::billing::void_payment(&mut c, ids[1], "Incorrect receipt")?;
        c.execute("UPDATE fx_rates SET rate=200000000", [])?;
        crate::billing::add_payment(&mut c, inv, &crate::today().to_string(), "0.01", "Bank", "")?;
        assert_eq!(amount(&c, ar)?, 0);
        assert_eq!(crate::billing::balance(&c, inv)?.remaining, 0);
        validate(&c)?;
        Ok(())
    }
    #[test]
    fn expense_edits_reverse_original_postings_without_duplicates() -> Result<()> {
        let mut c = db()?;
        let account = crate::accounting::default_account(&c)?;
        let mut e = crate::accounting::Expense {
            id: 0,
            date: crate::today().to_string(),
            description: "Office".into(),
            account,
            vendor: Some(1),
            currency: "CAD".into(),
            amount: "10".into(),
            task: None,
            tax: Default::default(),
        };
        e.id = crate::accounting::save_expense(&mut c, &e)?;
        assert_eq!(amount(&c, account)?, 1000);
        e.amount = "20".into();
        crate::accounting::save_expense(&mut c, &e)?;
        assert_eq!(amount(&c, account)?, 2000);
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM gl_reversals", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        let count: i64 = c.query_row("SELECT COUNT(*) FROM gl_entries", [], |r| r.get(0))?;
        crate::accounting::save_expense(&mut c, &e)?;
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM gl_entries", [], |r| r
                .get::<_, i64>(0))?,
            count
        );
        assert_eq!(amount(&c, role(&c, "payable")?)?, -2000);
        validate(&c)?;
        Ok(())
    }
    #[test]
    fn period_lock_and_missing_fx_roll_back_source_documents_and_numbers() -> Result<()> {
        let mut c = db()?;
        let t = crate::add_task(&c, "USD task", 1, "USD")?;
        crate::add_item(&c, t, 2, 1, None, None)?;
        crate::set_task_status(&mut c, t, false)?;
        assert!(crate::billing::merge(&mut c, &[t]).is_err());
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM invoices", [], |r| r.get::<_, i64>(0))?,
            0
        );
        let today = crate::today().to_string();
        configure(&mut c, "CAD", &[], &today)?;
        let j = manual(&c)?;
        assert!(save(&mut c, &j, true).is_err());
        assert!(crate::tax::cash(&c, 1, &today, "payment", "10", "", "Test").is_err());
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM tax_cash", [], |r| r.get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            c.query_row(
                "SELECT next_value FROM number_sequences WHERE kind='journal'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        configure(&mut c, "CAD", &[], "")?;
        crate::tax::cash(&c, 1, &today, "payment", "10", "", "Test")?;
        assert_eq!(amount(&c, role(&c, "bank")?)?, -1000);
        crate::tax::void_cash(&c, 1)?;
        assert_eq!(amount(&c, role(&c, "bank")?)?, 0);
        validate(&c)?;
        Ok(())
    }
    #[test]
    fn legacy_invoices_are_not_backfilled_and_reports_exclude_drafts() -> Result<()> {
        let mut c = db()?;
        let t = crate::add_task(&c, "Legacy", 1, "CAD")?;
        crate::add_item(&c, t, 1, 1, None, None)?;
        let raw = crate::snapshot(&c, t)?.to_string();
        c.execute(
            "INSERT INTO invoices(task,snapshot) VALUES(?,?)",
            params![t, raw],
        )?;
        crate::init(&c)?;
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM gl_sources", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        let j = manual(&c)?;
        save(&mut c, &j, false)?;
        let rows = trial_balance(&c, &crate::today().to_string())?;
        assert!(rows.iter().all(|r| r[3] == "0.00" && r[4] == "0.00"));
        assert_eq!(
            c.query_row("SELECT snapshot FROM invoices WHERE task=?", [t], |r| {
                r.get::<_, String>(0)
            })?,
            raw
        );
        validate(&c)?;
        Ok(())
    }
    #[test]
    fn foreign_expense_recoverable_tax_matches_cad_subledger_after_rate_change() -> Result<()> {
        let mut c = db()?;
        c.execute_batch("UPDATE tax_agencies SET registered=1,registration='TEST'; INSERT INTO fx_rates(source,target,rate) VALUES('USD','CAD',135000000);")?;
        let mut draft = crate::accounting::Expense {
            id: 0,
            date: crate::today().to_string(),
            description: "Foreign service".into(),
            account: crate::accounting::default_account(&c)?,
            vendor: Some(1),
            currency: "USD".into(),
            amount: "112".into(),
            task: None,
            tax: crate::tax::Input {
                code: 3,
                inclusive: true,
                recovery: 100,
            },
        };
        draft.id = crate::accounting::save_expense(&mut c, &draft)?;
        let recoverable: i64 = c.query_row(
            "SELECT recoverable_account FROM tax_agencies WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(amount(&c, recoverable)?, 675);
        c.execute("UPDATE fx_rates SET rate=140000000", [])?;
        draft.description = "Description only".into();
        crate::accounting::save_expense(&mut c, &draft)?;
        assert_eq!(amount(&c, recoverable)?, 675);
        draft.amount = "224".into();
        crate::accounting::save_expense(&mut c, &draft)?;
        assert_eq!(amount(&c, recoverable)?, 1400);
        assert_eq!(
            c.query_row(
                "SELECT sum(cad_recoverable) FROM tax_postings WHERE source='expense'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1400
        );
        assert_eq!(amount(&c, role(&c, "fx")?)?, -50);
        configure(&mut c, "CAD", &[], &crate::today().to_string())?;
        draft.amount = "336".into();
        assert!(crate::accounting::save_expense(&mut c, &draft).is_err());
        assert_eq!(
            crate::accounting::load_expense(&c, draft.id)?.amount,
            "224.00"
        );
        validate(&c)?;
        crate::storage::validate_projections(&c)?;
        Ok(())
    }
    #[test]
    fn gl_backups_include_attachments_and_reject_corrupt_source_snapshots() -> Result<()> {
        let mut c = db()?;
        let j = manual(&c)?;
        save_with_files(
            &mut c,
            &j,
            true,
            &[("test.txt".into(), b"attachment-content".to_vec())],
        )?;
        invoice(&mut c, 1, "CAD")?;
        let folder = std::env::temp_dir().join(format!(
            "axibiz-gl-backup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::create_dir(&folder)?;
        let path = folder.join("gl.db");
        crate::backup::save(&c, &path)?;
        crate::backup::verify(&path)?;
        let copy = Connection::open(&path)?;
        crate::auth::register(&copy)?;
        assert_eq!(
            copy.query_row("SELECT data FROM gl_attachments LIMIT 1", [], |r| r
                .get::<_, Vec<u8>>(0))?,
            b"attachment-content"
        );
        copy.execute_batch("DROP TRIGGER gl_lines_update_guard;UPDATE gl_lines SET description='Corrupt' WHERE entry=(SELECT entry FROM gl_sources WHERE kind='invoice' LIMIT 1);")?;
        drop(copy);
        assert!(crate::backup::verify(&path).is_err());
        std::fs::remove_file(&path)?;
        std::fs::remove_dir(&folder)?;
        Ok(())
    }
    #[test]
    fn ledger_opening_running_and_closing_balance_respect_dates_and_reversals() -> Result<()> {
        let mut c = db()?;
        let bank = role(&c, "bank")?;
        let mut j = manual(&c)?;
        j.date = "2026-10-25".into();
        let first = save(&mut c, &j, true)?;
        j.date = "2026-10-26".into();
        for l in &mut j.lines {
            l.debit /= 5;
            l.credit /= 5;
        }
        save(&mut c, &j, true)?;
        let (opening, rows, closing) = ledger(&c, "2026-10-26", "2026-10-26", bank)?;
        assert_eq!((opening, closing), (10000, 12000));
        assert_eq!(rows[0][11], "120.00");
        super::reverse(&mut c, first, "2026-10-26", "Correct opening")?;
        let (opening, rows, closing) = ledger(&c, "2026-10-26", "2026-10-26", bank)?;
        assert_eq!((opening, closing), (10000, 2000));
        assert_eq!(rows.last().unwrap()[11], "20.00");
        let (_, _, past) = ledger(&c, "2026-10-25", "2026-10-25", bank)?;
        assert_eq!(past, 10000);
        assert!(ledger(&c, "2026-10-26", "2026-10-25", bank).is_err());
        Ok(())
    }
    #[test]
    fn free_foreign_invoice_does_not_require_an_unused_fx_quote() -> Result<()> {
        let mut c = db()?;
        c.execute("UPDATE prices SET price=0 WHERE id=2", [])?;
        let id = invoice(&mut c, 2, "USD")?;
        assert_eq!(crate::billing::balance(&c, id)?.remaining, 0);
        assert!(source(&c, "invoice", id)?.unwrap().0.is_none());
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM gl_entries", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        validate(&c)?;
        Ok(())
    }
}
