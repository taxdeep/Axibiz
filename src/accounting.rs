use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
pub const TYPES: [&str; 5] = ["Asset", "Liability", "Equity", "Income", "Expense"];
// General Expenses and the three tax-control accounts are created by their own
// initializers, which preserve configured identities even after renumbering.
const TEMPLATE: &[(&str, &str, &str)] = &[
    ("1000", "Cash on Hand", "Asset"),
    ("1010", "Bank - CAD", "Asset"),
    ("1020", "Bank - USD", "Asset"),
    ("1100", "Accounts Receivable", "Asset"),
    ("1500", "Equipment", "Asset"),
    ("1590", "Accumulated Depreciation", "Asset"),
    ("2000", "Accounts Payable", "Liability"),
    ("2100", "Credit Card Payable", "Liability"),
    ("2300", "Loans Payable", "Liability"),
    ("3000", "Owner Capital", "Equity"),
    ("3100", "Retained Earnings", "Equity"),
    ("3200", "Owner Drawings", "Equity"),
    ("4000", "Service Revenue", "Income"),
    ("4100", "Other Revenue", "Income"),
    ("5100", "Subcontractor Expenses", "Expense"),
    ("5200", "Registry and Filing Fees", "Expense"),
    ("5300", "Software and Subscriptions", "Expense"),
    ("5400", "Professional Fees", "Expense"),
    ("5500", "Office Supplies", "Expense"),
    ("5600", "Rent and Utilities", "Expense"),
    ("5700", "Bank and Payment Fees", "Expense"),
    ("5800", "Travel Expenses", "Expense"),
    ("5900", "Advertising and Marketing", "Expense"),
    ("6000", "Insurance", "Expense"),
    ("6100", "Repairs and Maintenance", "Expense"),
    ("6200", "Depreciation Expense", "Expense"),
];
pub fn seed_template(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS account_templates(name TEXT PRIMARY KEY,applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);")?;
    if c.query_row(
        "SELECT EXISTS(SELECT 1 FROM account_templates WHERE name='axibiz-services-v1')",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        return Ok(());
    }
    for (code, name, kind) in TEMPLATE {
        c.execute("INSERT INTO accounts(code,name,kind) SELECT ?1,?2,?3 WHERE NOT EXISTS(SELECT 1 FROM accounts WHERE code=?1 OR (name=?2 AND kind=?3))",params![code,name,kind])?;
    }
    c.execute(
        "INSERT INTO account_templates(name) VALUES('axibiz-services-v1')",
        [],
    )?;
    Ok(())
}
pub fn init(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS accounts(id INTEGER PRIMARY KEY,code TEXT NOT NULL UNIQUE,name TEXT NOT NULL,kind TEXT NOT NULL CHECK(kind IN ('Asset','Liability','Equity','Income','Expense')),active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1)));
        CREATE TABLE IF NOT EXISTS accounting_config(id INTEGER PRIMARY KEY CHECK(id=1),default_account INTEGER NOT NULL REFERENCES accounts(id));
        CREATE TABLE IF NOT EXISTS ledger_expenses(id INTEGER PRIMARY KEY,date TEXT,description TEXT NOT NULL,account INTEGER NOT NULL REFERENCES accounts(id),vendor INTEGER REFERENCES vendors(id),currency TEXT NOT NULL,amount INTEGER NOT NULL CHECK(amount>=0),task INTEGER REFERENCES tasks(id),task_expense INTEGER UNIQUE REFERENCES expenses(id),task_currency TEXT,task_amount INTEGER,fx_rate INTEGER,created TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        CREATE INDEX IF NOT EXISTS ledger_expenses_account_idx ON ledger_expenses(account);
        CREATE INDEX IF NOT EXISTS ledger_expenses_task_idx ON ledger_expenses(task);
        ")?;
    let configured: bool =
        c.query_row("SELECT EXISTS(SELECT 1 FROM accounting_config)", [], |r| {
            r.get(0)
        })?;
    if !configured {
        c.execute("INSERT OR IGNORE INTO accounts(code,name,kind) VALUES('5000','General Expenses','Expense')",[])?;
        let mut account=c.query_row("SELECT id FROM accounts WHERE kind='Expense' AND active=1 ORDER BY (code='5000') DESC,id LIMIT 1",[],|r|r.get::<_,i64>(0)).optional()?;
        if account.is_none() {
            let mut code = "AX-EXPENSE".to_owned();
            let mut suffix = 0;
            while c.query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE code=?)",
                [&code],
                |r| r.get::<_, bool>(0),
            )? {
                suffix += 1;
                code = format!("AX-EXPENSE-{suffix}");
            }
            c.execute(
                "INSERT INTO accounts(code,name,kind) VALUES(?,'General Expenses','Expense')",
                [code],
            )?;
            account = Some(c.last_insert_rowid());
        }
        c.execute(
            "INSERT INTO accounting_config(id,default_account) VALUES(1,?)",
            [account.unwrap()],
        )?;
    }
    c.execute_batch("INSERT INTO ledger_expenses(date,description,account,currency,amount,task,task_expense,task_currency,task_amount,fx_rate)
        SELECT t.task_date,e.description,(SELECT default_account FROM accounting_config WHERE id=1),t.currency,e.amount,e.task,e.id,t.currency,e.amount,100000000
        FROM expenses e JOIN tasks t ON t.id=e.task WHERE NOT EXISTS(SELECT 1 FROM ledger_expenses l WHERE l.task_expense=e.id);")?;
    Ok(())
}
pub fn default_account(c: &Connection) -> Result<i64> {
    Ok(c.query_row(
        "SELECT default_account FROM accounting_config WHERE id=1",
        [],
        |r| r.get(0),
    )?)
}
pub fn save_account(
    c: &Connection,
    id: i64,
    code: &str,
    name: &str,
    kind: &str,
    active: bool,
) -> Result<i64> {
    crate::required(code)?;
    crate::required(name)?;
    ensure!(TYPES.contains(&kind), "科目类型无效");
    if id > 0 {
        let gl_kind:Option<String>=c.query_row("SELECT account_kind FROM gl_lines l JOIN gl_entries e ON e.id=l.entry WHERE l.account=? AND e.status='posted' LIMIT 1",[id],|r|r.get(0)).optional()?;
        ensure!(
            gl_kind.as_deref().is_none_or(|old| old == kind),
            "已有 GL 过账的科目不能改变类型"
        );
        let mapped: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM gl_accounts WHERE account=?)",
            [id],
            |r| r.get(0),
        )?;
        if mapped {
            ensure!(active, "GL 关联科目须保持启用，请先在 GL Setup 更换关联");
        }
        let (payable,recoverable):(bool,bool)=c.query_row("SELECT EXISTS(SELECT 1 FROM tax_agencies WHERE payable_account=?1),EXISTS(SELECT 1 FROM tax_agencies WHERE recoverable_account=?1)",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        ensure!(
            !payable || (kind == "Liability" && active),
            "应缴税科目必须保持启用的 Liability 类型"
        );
        ensure!(
            !recoverable || (kind == "Asset" && active),
            "进项抵扣科目必须保持启用的 Asset 类型"
        );
        if id == default_account(c)? {
            ensure!(
                kind == "Expense" && active,
                "默认费用科目必须保持启用的 Expense 类型，可修改编号与名称"
            );
        }
        if kind != "Expense" {
            ensure!(
                !c.query_row(
                    "SELECT EXISTS(SELECT 1 FROM ledger_expenses WHERE account=?)",
                    [id],
                    |r| r.get::<_, bool>(0)
                )?,
                "已被费用引用的科目必须保持 Expense 类型"
            );
        }
        ensure!(
            c.execute(
                "UPDATE accounts SET code=?,name=?,kind=?,active=? WHERE id=?",
                params![code.trim(), name.trim(), kind, active, id]
            )? == 1,
            "科目不存在"
        );
        Ok(id)
    } else {
        c.execute(
            "INSERT INTO accounts(code,name,kind,active) VALUES(?,?,?,?)",
            params![code.trim(), name.trim(), kind, active],
        )?;
        Ok(c.last_insert_rowid())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Expense {
    pub tax: crate::tax::Input,
    pub id: i64,
    pub date: String,
    pub description: String,
    pub account: i64,
    pub vendor: Option<i64>,
    pub currency: String,
    pub amount: String,
    pub task: Option<i64>,
}
pub fn save_expense(c: &mut Connection, draft: &Expense) -> Result<i64> {
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if let Some(vendor) = draft.vendor {
        let existing = if draft.id > 0 {
            load_expense(&tx, draft.id)?.vendor
        } else {
            None
        };
        crate::vendor::allow_usage(&tx, vendor, existing)?;
    }
    if draft.id > 0 {
        let count:i64=tx.query_row("SELECT count(*) FROM expense_document_lines WHERE document=(SELECT document FROM expense_document_lines WHERE expense=?)",[draft.id],|r|r.get(0))?;
        ensure!(count <= 1, "多行费用请在 Expense 页面按整张费用单编辑");
    }
    let id = save_expense_on(&tx, draft)?;
    if draft.id == 0 {
        crate::expense_doc::register_single(&tx, id)?;
    } else {
        tx.execute("UPDATE expense_documents SET date=?,vendor=?,currency=? WHERE id IN (SELECT document FROM expense_document_lines WHERE expense=?)",params![draft.date,draft.vendor,draft.currency,id])?;
        let document: i64 = tx.query_row(
            "SELECT document FROM expense_document_lines WHERE expense=?",
            [id],
            |r| r.get(0),
        )?;
        crate::gl::expense(&tx, document, false)?;
    }
    tx.commit()?;
    Ok(id)
}
pub(crate) fn save_expense_on(tx: &Connection, draft: &Expense) -> Result<i64> {
    if draft.id > 0 {
        let document: Option<i64> = tx
            .query_row(
                "SELECT document FROM expense_document_lines WHERE expense=?",
                [draft.id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = document {
            crate::ap::allow_revision(tx, id)?;
        }
    }
    crate::required(&draft.description)?;
    crate::task_date(&draft.date).map_err(anyhow::Error::msg)?;
    let currency = crate::currency(&draft.currency).map_err(anyhow::Error::msg)?;
    let calculation = crate::tax::expense_calc(tx, draft)?;
    let amount = calculation["cost"].as_i64().context("费用成本无效")?;
    ensure!(amount > 0, "费用金额必须大于零");
    let vendor = draft
        .vendor
        .filter(|id| *id > 0)
        .context("Vendor 为必填项，请选择供应商")?;
    ensure!(
        tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM vendors WHERE id=?)",
            [vendor],
            |r| r.get::<_, bool>(0)
        )?,
        "请从候选列表选定有效供应商"
    );
    let valid: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=? AND kind='Expense' AND active=1)",
        [draft.account],
        |r| r.get(0),
    )?;
    ensure!(valid, "请选择启用的 Expense 类科目");
    let mut old_link = None;
    let mut preserved = None;
    if draft.id > 0 {
        let (task, link): (Option<i64>, Option<i64>) = tx
            .query_row(
                "SELECT task,task_expense FROM ledger_expenses WHERE id=?",
                [draft.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .context("费用不存在")?;
        if let Some(task) = task {
            crate::editable(tx, task)?;
        }
        let (old_currency, old_amount, saved_amount, saved_rate): (
            String,
            i64,
            Option<i64>,
            Option<i64>,
        ) = tx.query_row(
            "SELECT currency,amount,task_amount,fx_rate FROM ledger_expenses WHERE id=?",
            [draft.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        if task == draft.task && old_currency == currency && old_amount == amount {
            if let (Some(converted), Some(rate)) = (saved_amount, saved_rate) {
                preserved = Some((converted, rate));
            }
        }
        old_link = link;
    }
    let mut link = None;
    let mut task_currency = None;
    let mut task_amount = None;
    let mut fx = None;
    if let Some(task) = draft.task {
        crate::editable(tx, task)?;
        let settlement: String =
            tx.query_row("SELECT currency FROM tasks WHERE id=?", [task], |r| {
                r.get(0)
            })?;
        let (converted, rate) = if let Some(saved) = preserved {
            saved
        } else {
            let rate =
                if settlement == "CAD" && !calculation["parts"].as_array().unwrap().is_empty() {
                    crate::tax::cad_rate(tx, &currency, &draft.date)?.0
                } else {
                    crate::exchange(tx, &currency, &settlement)?
                };
            (crate::convert(amount, rate)?, rate)
        };
        tx.execute(
            "INSERT INTO expenses(task,description,amount) VALUES(?,?,?)",
            params![task, draft.description.trim(), converted],
        )?;
        link = Some(tx.last_insert_rowid());
        task_currency = Some(settlement);
        task_amount = Some(converted);
        fx = Some(rate);
    }
    let id = if draft.id > 0 {
        tx.execute("UPDATE ledger_expenses SET date=?,description=?,account=?,vendor=?,currency=?,amount=?,task=?,task_expense=?,task_currency=?,task_amount=?,fx_rate=? WHERE id=?",params![draft.date,draft.description.trim(),draft.account,draft.vendor,currency,amount,draft.task,link,task_currency,task_amount,fx,draft.id])?;
        draft.id
    } else {
        tx.execute("INSERT INTO ledger_expenses(date,description,account,vendor,currency,amount,task,task_expense,task_currency,task_amount,fx_rate) VALUES(?,?,?,?,?,?,?,?,?,?,?)",params![draft.date,draft.description.trim(),draft.account,draft.vendor,currency,amount,draft.task,link,task_currency,task_amount,fx])?;
        tx.last_insert_rowid()
    };
    if let Some(old) = old_link {
        tx.execute("DELETE FROM expenses WHERE id=?", [old])?;
    }
    crate::tax::save_expense(tx, id, draft, &calculation)?;
    if let Some(task) = draft.task {
        crate::snapshot(tx, task)?;
    }
    Ok(id)
}
pub fn load_expense(c: &Connection, id: i64) -> Result<Expense> {
    Ok(c.query_row("SELECT l.id,COALESCE(l.date,''),l.description,l.account,l.vendor,l.currency,COALESCE(t.input,l.amount),l.task,COALESCE(t.code,0),COALESCE(t.inclusive,0),COALESCE(t.recovery,100) FROM ledger_expenses l LEFT JOIN expense_tax t ON t.expense=l.id WHERE l.id=?",[id],|r|Ok(Expense{id:r.get(0)?,date:r.get(1)?,description:r.get(2)?,account:r.get(3)?,vendor:r.get(4)?,currency:r.get(5)?,amount:crate::format_money(r.get(6)?),task:r.get(7)?,tax:crate::tax::Input{code:r.get(8)?,inclusive:r.get(9)?,recovery:r.get(10)?}}))?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_template_covers_all_types_and_is_applied_once() -> Result<()> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get::<_, i64>(0))?,
            32
        );
        for kind in TYPES {
            assert!(c.query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE kind=? AND active=1)",
                [kind],
                |r| r.get::<_, bool>(0)
            )?);
        }
        let account: i64 = c.query_row("SELECT id FROM accounts WHERE code='5200'", [], |r| {
            r.get(0)
        })?;
        save_account(&c, account, "5299", "Custom Filing Fees", "Expense", false)?;
        crate::init(&c)?;
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get::<_, i64>(0))?,
            32
        );
        assert!(!c.query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE code='5200')",
            [],
            |r| r.get::<_, bool>(0)
        )?);
        assert_eq!(
            c.query_row(
                "SELECT name,active FROM accounts WHERE id=?",
                [account],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))
            )?,
            ("Custom Filing Fees".into(), false)
        );
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM account_templates", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        Ok(())
    }
    #[test]
    fn template_upgrade_preserves_code_conflicts_names_and_configured_default() -> Result<()> {
        let c = Connection::open_in_memory()?;
        c.execute_batch("CREATE TABLE accounts(id INTEGER PRIMARY KEY,code TEXT NOT NULL UNIQUE,name TEXT NOT NULL,kind TEXT NOT NULL,active INTEGER NOT NULL DEFAULT 1); INSERT INTO accounts VALUES(1,'7999','My Default','Expense',1),(2,'1010','Existing Liability','Liability',0),(3,'5450','Professional Fees','Expense',1); CREATE TABLE accounting_config(id INTEGER PRIMARY KEY,default_account INTEGER NOT NULL); INSERT INTO accounting_config VALUES(1,1); PRAGMA user_version=9;")?;
        crate::init(&c)?;
        assert_eq!(default_account(&c)?, 1);
        assert_eq!(
            c.query_row(
                "SELECT name,kind,active FROM accounts WHERE id=2",
                [],
                |r| Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?
                ))
            )?,
            ("Existing Liability".into(), "Liability".into(), false)
        );
        assert!(!c.query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE code IN ('5000','5400'))",
            [],
            |r| r.get::<_, bool>(0)
        )?);
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM accounts WHERE name='Professional Fees'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        assert!(c.query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE code='4000' AND kind='Income')",
            [],
            |r| r.get::<_, bool>(0)
        )?);
        crate::init(&c)?;
        assert_eq!(default_account(&c)?, 1);
        Ok(())
    }
    #[test]
    fn expenses_classify_convert_and_update_task_cost_once() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Customer','')",
            [],
        )?;
        c.execute(
            "INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','')",
            [],
        )?;
        let task = crate::add_task(&c, "Task", 1, "CAD")?;
        let account = save_account(&c, 0, "5199", "Travel", "Expense", true)?;
        let income = save_account(&c, 0, "4199", "Sales", "Income", true)?;
        let mut e = Expense {
            tax: Default::default(),
            id: 0,
            date: "2026-10-25".into(),
            description: "Travel".into(),
            account,
            vendor: Some(1),
            currency: "USD".into(),
            amount: "10.00".into(),
            task: Some(task),
        };
        c.execute(
            "INSERT INTO fx_rates(source,target,rate) VALUES('USD','CAD',135000000)",
            [],
        )?;
        e.id = save_expense(&mut c, &e)?;
        assert_eq!(crate::snapshot(&c, task)?["cost"], 1350);
        e.amount = "20.00".into();
        save_expense(&mut c, &e)?;
        assert_eq!(crate::snapshot(&c, task)?["cost"], 2700);
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM expenses", [], |r| r.get::<_, i64>(0))?,
            1
        );
        crate::init(&c)?;
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM ledger_expenses", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        c.execute("UPDATE fx_rates SET rate=150000000", [])?;
        e.description = "Travel updated".into();
        save_expense(&mut c, &e)?;
        assert_eq!(crate::snapshot(&c, task)?["cost"], 2700);
        e.account = income;
        assert!(save_expense(&mut c, &e).is_err());
        e.account = account;
        assert!(save_account(&c, account, "5199", "Travel", "Asset", true).is_err());
        crate::set_task_status(&mut c, task, true)?;
        assert!(save_expense(&mut c, &e).is_err());
        e.id = 0;
        e.task = None;
        save_expense(&mut c, &e)?;
        assert_eq!(crate::snapshot(&c, task)?["cost"], 2700);
        Ok(())
    }
    #[test]
    fn default_account_identity_survives_renumbering_and_currency_is_normalized() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        let count: i64 = c.query_row(
            "SELECT COUNT(*) FROM accounts WHERE kind='Expense'",
            [],
            |r| r.get(0),
        )?;
        let account = default_account(&c)?;
        save_account(&c, account, "5999", "Business expenses", "Expense", true)?;
        crate::init(&c)?;
        assert_eq!(default_account(&c)?, account);
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM accounts WHERE kind='Expense'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            count
        );
        assert!(save_account(&c, account, "5999", "Business expenses", "Expense", false).is_err());
        c.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Customer','')",
            [],
        )?;
        c.execute(
            "INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','')",
            [],
        )?;
        let task = crate::add_task(&c, "Task", 1, "CAD")?;
        c.execute(
            "INSERT INTO fx_rates(source,target,rate) VALUES('USD','CAD',135000000)",
            [],
        )?;
        let draft = Expense {
            tax: Default::default(),
            id: 0,
            date: "2026-10-25".into(),
            description: "Travel".into(),
            account,
            vendor: Some(1),
            currency: "usd".into(),
            amount: "10.00".into(),
            task: Some(task),
        };
        let id = save_expense(&mut c, &draft)?;
        assert_eq!(load_expense(&c, id)?.currency, "USD");
        assert_eq!(crate::snapshot(&c, task)?["cost"], 1350);
        crate::execute(
            &mut c,
            crate::Command::TaskExpense {
                tax_code: 0,
                tax_included: false,
                recovery: 100,
                task,
                description: "Local cost".into(),
                amount: "1.00".into(),
                vendor: 1,
            },
        )?;
        assert_eq!(crate::snapshot(&c, task)?["cost"], 1450);
        Ok(())
    }
    #[test]
    fn expenses_require_vendor_but_task_is_optional() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute(
            "INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','')",
            [],
        )?;
        let mut draft = Expense {
            tax: Default::default(),
            id: 0,
            date: "2026-10-25".into(),
            description: "Office supplies".into(),
            account: default_account(&c)?,
            vendor: None,
            currency: "CAD".into(),
            amount: "25.00".into(),
            task: None,
        };
        for vendor in [None, Some(0), Some(999)] {
            draft.vendor = vendor;
            assert!(save_expense(&mut c, &draft).is_err());
        }
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM ledger_expenses", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        draft.vendor = Some(1);
        draft.id = save_expense(&mut c, &draft)?;
        let saved = load_expense(&c, draft.id)?;
        assert_eq!(saved.vendor, Some(1));
        assert_eq!(saved.task, None);
        draft.vendor = None;
        assert!(save_expense(&mut c, &draft).is_err());
        assert_eq!(load_expense(&c, draft.id)?.vendor, Some(1));
        Ok(())
    }
}
