//! Read models are separate from rendering and never infer balances after a query failure.
use super::{party_ui::Party, *};
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) struct Record {
    pub id: i64,
    pub name: String,
    pub currency: String,
    pub active: bool,
    pub search: Vec<String>,
}
#[derive(Clone, Default)]
pub(super) struct Data {
    pub records: Vec<Record>,
    pub amounts: BTreeMap<i64, Vec<(String, i64)>>,
}
#[derive(Default)]
pub(super) struct Cache {
    key: Option<(u64, i64, i64)>,
    customer: Option<Data>,
    vendor: Option<Data>,
    events: BTreeMap<(Party, i64), Vec<Event>>,
}
impl Cache {
    pub fn events(&mut self, db: &Connection, party: Party, owner: i64) -> Result<Vec<Event>> {
        self.load(db, party)?;
        if let Some(rows) = self.events.get(&(party, owner)) {
            return Ok(rows.clone());
        }
        let rows = events(db, party, owner)?;
        self.events.insert((party, owner), rows.clone());
        Ok(rows)
    }

    pub fn load(&mut self, db: &Connection, party: Party) -> Result<Data> {
        let key = (
            db.total_changes(),
            db.query_row("PRAGMA data_version", [], |r| r.get(0))?,
            db.query_row("PRAGMA schema_version", [], |r| r.get(0))?,
        );
        if self.key != Some(key) {
            self.customer = None;
            self.vendor = None;
            self.events.clear();
            self.key = Some(key)
        }
        let slot = match party {
            Party::Customer => &mut self.customer,
            Party::Vendor => &mut self.vendor,
        };
        if let Some(data) = slot {
            return Ok(data.clone());
        }
        let tx = db.unchecked_transaction()?;
        let table = party.plural().to_lowercase();
        let profiles = format!("{}_profiles", party.scope());
        let mut stmt=tx.prepare(&format!("SELECT p.id,p.name,p.currency,p.active,p.contact,COALESCE(json_extract(x.data,'$.company'),''),COALESCE(json_extract(x.data,'$.contact_name'),''),COALESCE(json_extract(x.data,'$.email'),'') FROM {table} p LEFT JOIN {profiles} x ON x.{}=p.id ORDER BY p.name COLLATE NOCASE,p.id",party.scope()))?;
        let records = stmt
            .query_map([], |r| {
                Ok(Record {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    currency: r.get(2)?,
                    active: r.get(3)?,
                    search: (1..=7)
                        .filter(|i| *i != 3)
                        .map(|i| r.get::<_, String>(i))
                        .collect::<rusqlite::Result<_>>()?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        let amounts = match party {
            Party::Customer => crate::party_balances::customers(&tx)?,
            Party::Vendor => crate::party_balances::vendors(&tx)?,
        };
        tx.commit()?;
        let data = Data { records, amounts };
        *slot = Some(data.clone());
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_cache_invalidates_local_external_and_schema_changes_without_writes() -> Result<()> {
        let path = std::env::temp_dir().join(format!(
            "axibiz-party-cache-{}-{}.db",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let c = Connection::open(&path)?;
        crate::init(&c)?;
        c.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Original','')",
            [],
        )?;
        let mut cache = Cache::default();
        let before = c.total_changes();
        assert_eq!(cache.load(&c, Party::Customer)?.records[0].name, "Original");
        assert_eq!(cache.load(&c, Party::Customer)?.records[0].name, "Original");
        assert_eq!(c.total_changes(), before);
        c.execute("UPDATE customers SET name='Local' WHERE id=1", [])?;
        assert_eq!(cache.load(&c, Party::Customer)?.records[0].name, "Local");
        let other = Connection::open(&path)?;
        crate::auth::register(&other)?;
        other.execute("UPDATE customers SET name='External' WHERE id=1", [])?;
        assert_eq!(cache.load(&c, Party::Customer)?.records[0].name, "External");
        assert!(cache.events(&c, Party::Customer, 1)?.is_empty());
        c.execute_batch("ALTER TABLE customer_profiles RENAME TO missing_profiles;")?;
        assert!(
            cache.load(&c, Party::Customer).is_err(),
            "A failed query must not reuse cached balances or pretend the list is empty"
        );
        drop(other);
        drop(c);
        std::fs::remove_file(path)?;
        Ok(())
    }
    #[test]
    fn transaction_filter_keeps_unknown_balances_out_of_open_documents() {
        let row = Event {
            kind: "Bill".into(),
            record: 1,
            document: 1,
            number: "EXP001".into(),
            date: "2026-10-10".into(),
            owner: 1,
            party: "München Services".into(),
            currency: "CAD".into(),
            amount: 10000,
            remaining: None,
            reference: "Reference".into(),
            memo: "Annual service".into(),
        };
        assert!(row.matches(1, "2026-10", " MÜNCHEN "));
        assert!(row.matches(0, "", "reference"));
        assert!(!row.matches(3, "", ""));
        assert!(!row.matches(2, "", ""));
        assert!(!row.matches(0, "2025", ""));
        let known = Event {
            remaining: Some(100),
            ..row
        };
        assert!(known.matches(3, "2026", "annual"));
    }
}

pub(super) fn summary(amounts: Option<&Vec<(String, i64)>>, currency: &str) -> String {
    match amounts {
        Some(values) if !values.is_empty() => values
            .iter()
            .map(|(c, a)| format!("{c} {}", crate::format_money(*a)))
            .collect::<Vec<_>>()
            .join(" / "),
        _ => format!("{currency} 0.00"),
    }
}

#[derive(Clone)]
pub(super) struct Event {
    pub kind: String,
    pub record: i64,
    pub document: i64,
    pub number: String,
    pub date: String,
    pub owner: i64,
    pub party: String,
    pub currency: String,
    pub amount: i64,
    pub remaining: Option<i64>,
    pub reference: String,
    pub memo: String,
}
impl Event {
    pub fn is_document(&self) -> bool {
        self.kind == "Invoice" || self.kind == "Bill"
    }
    pub fn matches(&self, filter: usize, prefix: &str, query: &str) -> bool {
        self.date.starts_with(prefix)
            && match filter {
                1 => self.is_document(),
                2 => !self.is_document(),
                3 => self.is_document() && self.remaining.is_some_and(|a| a > 0),
                _ => true,
            }
            && super::party_ui::matches(
                &[
                    self.kind.clone(),
                    self.number.clone(),
                    self.date.clone(),
                    self.party.clone(),
                    self.reference.clone(),
                    self.memo.clone(),
                ],
                query,
            )
    }
    pub fn memo(&self) -> String {
        if self.reference.is_empty() {
            self.memo.clone()
        } else if self.memo.is_empty() {
            self.reference.clone()
        } else {
            format!("{} | {}", self.reference, self.memo)
        }
    }
}
pub(super) fn events(db: &Connection, party: Party, owner: i64) -> Result<Vec<Event>> {
    let tx = db.unchecked_transaction()?;
    let sql=match party {
        Party::Customer=>"SELECT 'Invoice',i.id,i.id,COALESCE(i.number,''),COALESCE(json_extract(i.snapshot,'$.invoice_date'),substr(i.created,1,10)),h.customer_id,h.customer_name,h.currency,h.total,h.total-COALESCE(p.received,0),'',h.title FROM invoice_headers h JOIN invoices i ON i.id=h.invoice LEFT JOIN (SELECT invoice,sum(amount) AS received FROM payments WHERE voided=0 GROUP BY invoice) p ON p.invoice=i.id WHERE h.customer_id=?1 OR ?1=0 UNION ALL SELECT CASE p.voided WHEN 1 THEN 'Voided receipt' ELSE 'Receipt' END,p.id,p.invoice,COALESCE(p.number,''),COALESCE(p.date,'Unknown'),h.customer_id,h.customer_name,h.currency,p.amount,NULL,p.reference,'' FROM payments p JOIN invoice_headers h ON h.invoice=p.invoice WHERE h.customer_id=?1 OR ?1=0 ORDER BY 5 DESC,4 DESC",
        Party::Vendor=>"SELECT 'Bill',d.id,d.id,COALESCE(d.number,''),COALESCE(d.date,'Unknown'),d.vendor,v.name,d.currency,sum(COALESCE(t.gross,e.amount)),NULL,d.reference,CASE WHEN length(trim(d.memo))>0 THEN d.memo ELSE group_concat(e.description,'; ') END FROM expense_documents d JOIN vendors v ON v.id=d.vendor JOIN expense_document_lines x ON x.document=d.id JOIN ledger_expenses e ON e.id=x.expense LEFT JOIN expense_tax t ON t.expense=e.id WHERE d.vendor=?1 OR ?1=0 GROUP BY d.id UNION ALL SELECT CASE WHEN r.id IS NULL THEN 'Payment' ELSE 'Voided payment' END,p.id,0,p.number,p.date,p.vendor,v.name,p.currency,p.amount,NULL,p.reference,p.memo FROM vendor_payments p JOIN vendors v ON v.id=p.vendor LEFT JOIN ap_voids r ON r.payment=p.id WHERE p.vendor=?1 OR ?1=0 ORDER BY 5 DESC,4 DESC",
    };
    let mut stmt = tx.prepare(sql)?;
    let mut rows = stmt
        .query_map([owner], |r| {
            Ok(Event {
                kind: r.get(0)?,
                record: r.get(1)?,
                document: r.get(2)?,
                number: r.get(3)?,
                date: r.get(4)?,
                owner: r.get::<_, Option<i64>>(5)?.unwrap_or(0),
                party: r.get(6)?,
                currency: r.get(7)?,
                amount: r.get(8)?,
                remaining: r.get(9)?,
                reference: r.get(10)?,
                memo: r.get(11)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    if party == Party::Vendor {
        let mut groups = std::collections::BTreeSet::new();
        for row in &rows {
            if row.is_document() {
                groups.insert((row.owner, row.currency.clone()));
            }
        }
        for (vendor, currency) in groups {
            let today = crate::today().to_string();
            let r = crate::ap::reconciliation(&tx, vendor, &currency, &today)?;
            if r.ready() {
                for b in crate::ap::bills(&tx, vendor, &currency, &today)? {
                    if let Some(row) = rows
                        .iter_mut()
                        .find(|r| r.is_document() && r.document == b.document)
                    {
                        row.remaining = Some(b.remaining)
                    }
                }
            }
        }
    }
    tx.commit()?;
    Ok(rows)
}
