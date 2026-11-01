use anyhow::{ensure, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Profile {
    pub company: String,
    pub contact_name: String,
    pub phone: String,
    pub mobile: String,
    pub email: String,
    pub website: String,
    pub address: String,
    pub account_number: String,
    pub payment_terms: String,
    pub country: String,
    pub business_number: String,
    pub notes: String,
    pub cc_email: String,
    pub shipping_address: String,
    pub same_address: bool,
    pub vendor_type: String,
    pub default_accounts: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Vendor {
    pub id: i64,
    pub name: String,
    pub contact: String,
    pub currency: String,
    pub active: bool,
    pub tax_code: i64,
    pub profile: Profile,
}
impl Default for Vendor {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            contact: String::new(),
            currency: "CAD".into(),
            active: true,
            tax_code: 0,
            profile: Profile {
                country: "Canada".into(),
                ..Default::default()
            },
        }
    }
}

pub fn init(c: &Connection) -> Result<()> {
    if !c.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('vendors') WHERE name='active')",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        c.execute_batch("ALTER TABLE vendors ADD COLUMN active INTEGER NOT NULL DEFAULT 1 CHECK(active IN(0,1));")?;
    }
    c.execute_batch("CREATE TABLE IF NOT EXISTS vendor_profiles(vendor INTEGER PRIMARY KEY REFERENCES vendors(id),data TEXT NOT NULL CHECK(json_valid(data))); CREATE INDEX IF NOT EXISTS vendors_active_name ON vendors(active,name);")?;
    Ok(())
}

pub fn load(c: &Connection, id: i64) -> Result<Vendor> {
    let mut vendor = c.query_row(
        "SELECT id,name,contact,currency,active FROM vendors WHERE id=?",
        [id],
        |r| {
            Ok(Vendor {
                id: r.get(0)?,
                name: r.get(1)?,
                contact: r.get(2)?,
                currency: r.get(3)?,
                active: r.get(4)?,
                tax_code: 0,
                profile: Profile::default(),
            })
        },
    )?;
    if let Some(raw) = c
        .query_row(
            "SELECT data FROM vendor_profiles WHERE vendor=?",
            [id],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    {
        vendor.profile = serde_json::from_str(&raw)?;
    }
    vendor.tax_code = c
        .query_row(
            "SELECT code FROM tax_assignments WHERE entity='vendor' AND record=?",
            [id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    Ok(vendor)
}

pub fn save(c: &mut Connection, draft: &Vendor) -> Result<i64> {
    ensure!(!draft.name.trim().is_empty(), "Vendor name is required.");
    let currency = crate::currency(&draft.currency).map_err(anyhow::Error::msg)?;
    let email = draft.profile.email.trim();
    ensure!(
        email.is_empty() || (email.contains('@') && !email.chars().any(char::is_whitespace)),
        "Enter a valid email address."
    );
    ensure!(
        draft.profile.default_accounts.len() <= 3,
        "Select at most three expense accounts."
    );
    let mut used = std::collections::BTreeSet::new();
    for account in &draft.profile.default_accounts {
        ensure!(
            used.insert(*account),
            "Choose different default expense accounts."
        );
        ensure!(
            c.query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=? AND kind='Expense' AND active=1)",
                [account],
                |r| r.get::<_, bool>(0)
            )?,
            "A default expense account is missing or inactive."
        );
    }
    let tx = c.transaction()?;
    let id = if draft.id == 0 {
        tx.execute(
            "INSERT INTO vendors(name,contact,currency,active) VALUES(?,?,?,?)",
            params![draft.name.trim(), draft.contact, currency, draft.active],
        )?;
        tx.last_insert_rowid()
    } else {
        ensure!(
            tx.execute(
                "UPDATE vendors SET name=?,contact=?,currency=?,active=? WHERE id=?",
                params![
                    draft.name.trim(),
                    draft.contact,
                    currency,
                    draft.active,
                    draft.id
                ]
            )? == 1,
            "Vendor no longer exists."
        );
        draft.id
    };
    crate::tax::admin::assign(&tx, "vendor", id, draft.tax_code)?;
    tx.execute("INSERT INTO vendor_profiles(vendor,data) VALUES(?,?) ON CONFLICT(vendor) DO UPDATE SET data=excluded.data", params![id,serde_json::to_string(&draft.profile)?])?;
    tx.commit()?;
    Ok(id)
}

pub fn allow_usage(c: &Connection, id: i64, existing: Option<i64>) -> Result<()> {
    let active: bool = c.query_row("SELECT active FROM vendors WHERE id=?", [id], |r| r.get(0))?;
    ensure!(
        active || existing == Some(id),
        "This vendor is inactive. Select an active vendor for new work."
    );
    Ok(())
}

// Gross source-currency expense amounts, never an accounts-payable balance.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_accounts_reject_duplicates_inactive_and_non_expense_accounts() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        let account = crate::accounting::default_account(&c)?;
        let bank = crate::gl::role(&c, "bank")?;
        let mut v = Vendor {
            name: "Validated defaults".into(),
            ..Default::default()
        };
        v.profile.default_accounts = vec![account, account];
        assert!(save(&mut c, &v).is_err());
        v.profile.default_accounts = vec![bank];
        assert!(save(&mut c, &v).is_err());
        v.profile.default_accounts = vec![account];
        v.profile.shipping_address = "Warehouse".into();
        v.profile.vendor_type = "Services".into();
        v.id = save(&mut c, &v)?;
        assert_eq!(load(&c, v.id)?.profile, v.profile);
        let other: i64 = c.query_row("SELECT id FROM accounts WHERE code='5100'", [], |r| {
            r.get(0)
        })?;
        c.execute("UPDATE accounts SET active=0 WHERE id=?", [other])?;
        v.profile.default_accounts = vec![other];
        assert!(save(&mut c, &v).is_err());
        assert_eq!(load(&c, v.id)?.profile.default_accounts, vec![account]);
        Ok(())
    }

    #[test]
    fn legacy_vendors_keep_identity_contact_currency_and_migrate_once() -> Result<()> {
        let c = Connection::open_in_memory()?;
        c.execute_batch("CREATE TABLE vendors(id INTEGER PRIMARY KEY,name TEXT NOT NULL,contact TEXT NOT NULL,currency TEXT NOT NULL); INSERT INTO vendors VALUES(7,'Legacy supplier','Original unstructured contact','USD'); PRAGMA user_version=16;")?;
        crate::init(&c)?;
        crate::init(&c)?;
        let v = load(&c, 7)?;
        assert_eq!(v.name, "Legacy supplier");
        assert_eq!(v.contact, "Original unstructured contact");
        assert_eq!(v.currency, "USD");
        assert!(v.active);
        assert_eq!(v.profile, Profile::default());
        assert_eq!(
            c.query_row("SELECT count(*) FROM vendors", [], |r| r.get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
            crate::SCHEMA_VERSION
        );
        Ok(())
    }

    #[test]
    fn vendor_profile_tax_default_and_audit_save_atomically() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        let mut v = Vendor {
            name: "Supplier".into(),
            tax_code: 1,
            ..Default::default()
        };
        v.profile.email = "accounts@example.com".into();
        v.profile.address = "100 Main Street\nVancouver".into();
        v.id = save(&mut c, &v)?;
        assert_eq!(load(&c, v.id)?.profile, v.profile);
        let audit: i64 = c.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0))?;
        let mut invalid = v.clone();
        invalid.name = "Must roll back".into();
        invalid.tax_code = 999999;
        invalid.profile.notes = "Must roll back".into();
        assert!(save(&mut c, &invalid).is_err());
        assert_eq!(load(&c, v.id)?.name, v.name);
        assert_eq!(load(&c, v.id)?.profile, v.profile);
        assert_eq!(load(&c, v.id)?.tax_code, 1);
        assert_eq!(
            c.query_row("SELECT count(*) FROM audit_log", [], |r| r.get::<_, i64>(0))?,
            audit
        );
        invalid.id = 0;
        assert!(save(&mut c, &invalid).is_err());
        invalid = v.clone();
        invalid.profile.email = "invalid email".into();
        assert!(save(&mut c, &invalid).is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM vendors", [], |r| r.get::<_, i64>(0))?,
            1
        );
        assert!(c.query_row(
            "SELECT EXISTS(SELECT 1 FROM audit_log WHERE entity='vendor_profiles' AND record_id=?)",
            [v.id],
            |r| r.get::<_, bool>(0)
        )?);
        Ok(())
    }

    #[test]
    fn gross_expenses_stay_separate_by_currency_and_inactive_history_remains_editable() -> Result<()>
    {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute_batch("UPDATE tax_agencies SET registered=1,registration='TEST'; INSERT INTO fx_rates(source,target,rate) VALUES('USD','CAD',135000000);")?;
        let mut v = Vendor {
            name: "Supplier".into(),
            ..Default::default()
        };
        v.id = save(&mut c, &v)?;
        let account = crate::accounting::default_account(&c)?;
        c.execute("INSERT INTO prices(vendor,name,unit,currency,cost,price) VALUES(?,'Existing service','item','CAD',0,1000)",[v.id])?;
        c.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Buyer','')",
            [],
        )?;
        let task = crate::add_task(&c, "Draft task", 1, "CAD")?;
        let mut documents = Vec::new();
        for currency in ["CAD", "USD"] {
            let line = crate::accounting::Expense {
                id: 0,
                date: crate::today().to_string(),
                description: "Service".into(),
                account,
                vendor: Some(v.id),
                currency: currency.into(),
                amount: "100.00".into(),
                task: None,
                tax: crate::tax::Input {
                    code: 1,
                    inclusive: false,
                    recovery: 100,
                },
            };
            let doc = crate::expense_doc::Document {
                id: 0,
                vendor: Some(v.id),
                date: crate::today().to_string(),
                currency: currency.into(),
                reference: "BILL-1".into(),
                due_date: None,
                memo: String::new(),
                terms: String::new(),
                lines: vec![line],
            };
            let id = crate::expense_doc::save(&mut c, &doc)?;
            documents.push(id);
        }
        assert_eq!(
            crate::party_balances::vendors(&c)?
                .get(&v.id)
                .cloned()
                .unwrap_or_default(),
            vec![("CAD".into(), 10500), ("USD".into(), 10500)]
        );
        let old = crate::expense_doc::load(&c, documents[0])?;
        v.active = false;
        v.currency = "CNY".into();
        v.name = "Renamed supplier".into();
        v.profile.payment_terms = "Net 60".into();
        save(&mut c, &v)?;
        let saved = crate::expense_doc::load(&c, documents[0])?;
        assert_eq!(saved.currency, old.currency);
        assert_eq!(saved.due_date, old.due_date);
        assert_eq!(saved.lines[0].amount, old.lines[0].amount);
        let mut new = old.clone();
        new.id = 0;
        for line in &mut new.lines {
            line.id = 0;
        }
        let count: i64 = c.query_row("SELECT count(*) FROM expense_documents", [], |r| r.get(0))?;
        assert!(crate::expense_doc::save(&mut c, &new).is_err());
        assert!(crate::accounting::save_expense(&mut c, &new.lines[0]).is_err());
        assert!(crate::add_item(&c, task, 1, 1, None, None).is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM expense_documents", [], |r| r
                .get::<_, i64>(0))?,
            count
        );
        let mut edit = old;
        edit.memo = "Corrected memo".into();
        crate::expense_doc::save(&mut c, &edit)?;
        assert_eq!(
            crate::party_balances::vendors(&c)?
                .get(&v.id)
                .cloned()
                .unwrap_or_default(),
            vec![("CAD".into(), 10500), ("USD".into(), 10500)]
        );
        assert!(crate::execute(
            &mut c,
            crate::Command::PriceAdd {
                vendor: v.id,
                name: "New service".into(),
                cost: "0.00".into(),
                price: "10.00".into(),
                unit: "item".into(),
                currency: Some("CAD".into())
            }
        )
        .is_err());
        crate::storage::validate_projections(&c)?;
        Ok(())
    }
}
