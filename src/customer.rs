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
    pub cc_email: String,
    pub website: String,
    pub billing_address: String,
    pub shipping_address: String,
    pub same_shipping: bool,
    pub account_number: String,
    pub payment_terms: String,
    pub payment_method: String,
    pub country: String,
    pub business_number: String,
    pub customer_type: String,
    pub notes: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Customer {
    pub id: i64,
    pub name: String,
    pub contact: String,
    pub currency: String,
    pub active: bool,
    pub tax_code: i64,
    pub profile: Profile,
}
impl Default for Customer {
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
                same_shipping: true,
                ..Default::default()
            },
        }
    }
}
pub fn init(c: &Connection) -> Result<()> {
    if !c.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('customers') WHERE name='active')",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        c.execute_batch("ALTER TABLE customers ADD COLUMN active INTEGER NOT NULL DEFAULT 1 CHECK(active IN(0,1));")?;
    }
    c.execute_batch("CREATE TABLE IF NOT EXISTS customer_profiles(customer INTEGER PRIMARY KEY REFERENCES customers(id),data TEXT NOT NULL CHECK(json_valid(data))); CREATE INDEX IF NOT EXISTS customers_active_name ON customers(active,name);")?;
    Ok(())
}
pub fn load(c: &Connection, id: i64) -> Result<Customer> {
    let mut customer = c.query_row(
        "SELECT id,name,contact,currency,active FROM customers WHERE id=?",
        [id],
        |r| {
            Ok(Customer {
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
            "SELECT data FROM customer_profiles WHERE customer=?",
            [id],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    {
        customer.profile = serde_json::from_str(&raw)?;
    }
    customer.tax_code = c
        .query_row(
            "SELECT code FROM tax_assignments WHERE entity='customer' AND record=?",
            [id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    Ok(customer)
}
pub fn save(c: &mut Connection, draft: &Customer) -> Result<i64> {
    ensure!(!draft.name.trim().is_empty(), "Customer name is required.");
    let currency = crate::currency(&draft.currency).map_err(anyhow::Error::msg)?;
    for address in std::iter::once(draft.profile.email.as_str())
        .chain(draft.profile.cc_email.split([',', ';']))
    {
        let address = address.trim();
        ensure!(
            address.is_empty()
                || (address.contains('@') && !address.chars().any(char::is_whitespace)),
            "Enter a valid email address."
        );
    }
    let mut profile = draft.profile.clone();
    if profile.same_shipping {
        profile.shipping_address = profile.billing_address.clone();
    }
    let tx = c.transaction()?;
    let id = if draft.id == 0 {
        tx.execute(
            "INSERT INTO customers(name,contact,currency,active) VALUES(?,?,?,?)",
            params![draft.name.trim(), draft.contact, currency, draft.active],
        )?;
        tx.last_insert_rowid()
    } else {
        ensure!(
            tx.execute(
                "UPDATE customers SET name=?,contact=?,currency=?,active=? WHERE id=?",
                params![
                    draft.name.trim(),
                    draft.contact,
                    currency,
                    draft.active,
                    draft.id
                ]
            )? == 1,
            "Customer no longer exists."
        );
        draft.id
    };
    crate::tax::admin::assign(&tx, "customer", id, draft.tax_code)?;
    tx.execute("INSERT INTO customer_profiles(customer,data) VALUES(?,?) ON CONFLICT(customer) DO UPDATE SET data=excluded.data", params![id, serde_json::to_string(&profile)?])?;
    tx.commit()?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_customer_migrates_without_rewriting_contact_or_currency() -> Result<()> {
        let c = Connection::open_in_memory()?;
        c.execute_batch("CREATE TABLE customers(id INTEGER PRIMARY KEY,name TEXT NOT NULL,contact TEXT NOT NULL); INSERT INTO customers VALUES(1,'Legacy customer','Original contact');")?;
        crate::init(&c)?;
        crate::init(&c)?;
        let customer = load(&c, 1)?;
        assert_eq!(customer.name, "Legacy customer");
        assert_eq!(customer.contact, "Original contact");
        assert_eq!(customer.currency, "CAD");
        assert!(customer.active);
        assert_eq!(
            c.query_row("SELECT count(*) FROM customers", [], |r| r.get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
            crate::SCHEMA_VERSION
        );
        Ok(())
    }

    #[test]
    fn customer_profile_and_tax_default_save_atomically() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        let mut draft = Customer {
            name: "Customer".into(),
            ..Default::default()
        };
        draft.profile.email = "billing@example.com".into();
        draft.profile.billing_address = "100 Main Street\nVancouver".into();
        let id = save(&mut c, &draft)?;
        let original = load(&c, id)?;
        assert_eq!(
            original.profile.shipping_address,
            original.profile.billing_address
        );
        assert_eq!(original.profile.email, "billing@example.com");
        let log_count: i64 = c.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0))?;
        let mut invalid = original.clone();
        invalid.name = "Must roll back".into();
        invalid.profile.notes = "Must roll back".into();
        invalid.tax_code = 999999;
        assert!(save(&mut c, &invalid).is_err());
        assert_eq!(load(&c, id)?.name, original.name);
        assert_eq!(load(&c, id)?.profile, original.profile);
        assert_eq!(
            c.query_row("SELECT count(*) FROM audit_log", [], |r| r.get::<_, i64>(0))?,
            log_count
        );
        draft.profile.email = "invalid email".into();
        assert!(save(&mut c, &draft).is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM customers", [], |r| r.get::<_, i64>(0))?,
            1
        );
        assert!(c.query_row(
            "SELECT EXISTS(SELECT 1 FROM audit_log WHERE entity='customer_profiles')",
            [],
            |r| r.get::<_, bool>(0)
        )?);
        Ok(())
    }

    #[test]
    fn profile_defaults_are_snapshotted_and_balances_remain_separate_by_currency() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute(
            "INSERT INTO fx_rates(source,target,rate) VALUES('USD','CAD',135000000)",
            [],
        )?;
        let mut customer = Customer {
            name: "Buyer".into(),
            ..Default::default()
        };
        customer.profile.payment_terms = "Net 30".into();
        customer.profile.billing_address = "Old <street>".into();
        customer.id = save(&mut c, &customer)?;
        let mut invoices = Vec::new();
        for currency in ["CAD", "USD"] {
            let draft = crate::billing::DirectInvoice {
                request_key: format!("customer-{currency}"),
                customer: customer.id,
                date: crate::today().to_string(),
                currency: currency.into(),
                title: "Service".into(),
                lines: vec![crate::billing::DirectLine {
                    price_id: 0,
                    description: "Service".into(),
                    unit: "ea".into(),
                    quantity: 1,
                    unit_price: Some(10000),
                    tax_code: Some(0),
                }],
            };
            let preview = crate::billing::direct_preview(&c, &draft)?;
            assert_eq!(preview["payment_terms"], "Net 30");
            assert_eq!(preview["customer_address"], "Old <street>");
            invoices.push(crate::billing::direct_confirm(&mut c, &draft, &preview)?);
        }
        let before: String = c.query_row(
            "SELECT snapshot FROM invoices WHERE id=?",
            [invoices[0]],
            |r| r.get(0),
        )?;
        customer.name = "Renamed buyer".into();
        customer.profile.billing_address = "New street".into();
        customer.profile.payment_terms = "Net 60".into();
        customer.active = false;
        save(&mut c, &customer)?;
        assert_eq!(
            c.query_row(
                "SELECT snapshot FROM invoices WHERE id=?",
                [invoices[0]],
                |r| r.get::<_, String>(0)
            )?,
            before
        );
        assert!(crate::invoice::document(&c, invoices[0])?.contains("Old &lt;street&gt;"));
        crate::billing::add_payment(
            &mut c,
            invoices[0],
            &crate::today().to_string(),
            "25.00",
            "Cash",
            "",
        )?;
        assert_eq!(
            crate::party_balances::customers(&c)?
                .get(&customer.id)
                .cloned()
                .unwrap_or_default(),
            vec![("CAD".into(), 7500), ("USD".into(), 10000)]
        );
        assert!(crate::add_task(&c, "New task", customer.id, "CAD").is_err());
        crate::storage::validate_projections(&c)?;
        Ok(())
    }
}
