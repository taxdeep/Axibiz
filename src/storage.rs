//! Additive migrations, structured invoice reporting and database-level safeguards.
use anyhow::{ensure, Context, Result};
use rusqlite::Connection;

pub fn rebuild_invoice_anchor(c: &Connection) -> Result<()> {
    // Foreign keys are disabled only by init's outer guard. Everything below,
    // including all other migrations, is committed as one transaction.
    let objects = {
        let mut stmt=c.prepare("SELECT type,name,sql FROM sqlite_master WHERE type IN('view','trigger') AND sql IS NOT NULL ORDER BY type,name")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let indexes = {
        let mut stmt=c.prepare("SELECT sql FROM sqlite_master WHERE type='index' AND tbl_name='invoices' AND sql IS NOT NULL")?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for (kind, name, _) in &objects {
        c.execute_batch(&format!(
            "DROP {} \"{}\"",
            kind.to_uppercase(),
            name.replace('"', "\"\"")
        ))?;
    }
    let number: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('invoices') WHERE name='number')",
        [],
        |r| r.get(0),
    )?;
    c.execute_batch("CREATE TABLE invoices_v12(id INTEGER PRIMARY KEY,task INTEGER UNIQUE REFERENCES tasks(id),created TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,snapshot TEXT NOT NULL,paid INTEGER NOT NULL DEFAULT 0,number TEXT);")?;
    c.execute_batch(&format!("INSERT INTO invoices_v12(id,task,created,snapshot,paid,number) SELECT id,task,created,snapshot,paid,{} FROM invoices;DROP TABLE invoices;ALTER TABLE invoices_v12 RENAME TO invoices;",if number{"number"}else{"NULL"}))?;
    for sql in indexes {
        c.execute_batch(&sql)?;
    }
    for (_, _, sql) in objects {
        c.execute_batch(&sql)?;
    }
    Ok(())
}

pub fn company_id(c: &Connection) -> Result<Option<String>> {
    let exists: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='company_identity')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    Ok(Some(c.query_row(
        "SELECT uuid FROM company_identity WHERE id=1",
        [],
        |r| r.get(0),
    )?))
}

// Temporarily suspend only the new-entry expense rules while importing legacy rows.
// The caller holds the migration transaction; these are reinstated before commit.
pub fn prepare_legacy(c: &Connection) -> Result<()> {
    c.execute_batch("DROP TRIGGER IF EXISTS expense_required_insert; DROP TRIGGER IF EXISTS expense_required_update; DROP TRIGGER IF EXISTS expense_document_required_insert; DROP TRIGGER IF EXISTS expense_document_required_update;")?;
    Ok(())
}

pub fn init(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
        DROP TRIGGER IF EXISTS invoice_valid_insert;
        CREATE TRIGGER invoice_valid_insert BEFORE INSERT ON invoices BEGIN
            SELECT CASE WHEN json_valid(NEW.snapshot)=0 THEN RAISE(ABORT,'Invalid invoice JSON') END;
            SELECT CASE WHEN NEW.task IS NULL AND (json_extract(NEW.snapshot,'$.origin') IS NOT 'direct' OR json_type(NEW.snapshot,'$.customer_id') IS NOT 'integer' OR json_extract(NEW.snapshot,'$.task') IS NOT NULL OR json_type(NEW.snapshot,'$.tasks') IS NOT 'array' OR json_array_length(NEW.snapshot,'$.tasks')!=0) THEN RAISE(ABORT,'Direct invoice requires a customer and no task sources') END;
            SELECT CASE WHEN json_type(NEW.snapshot,'$.items') IS NOT 'array' OR json_array_length(NEW.snapshot,'$.items')=0 OR json_type(NEW.snapshot,'$.total') IS NOT 'integer' OR json_extract(NEW.snapshot,'$.total')-COALESCE(json_extract(NEW.snapshot,'$.tax_total'),0)!=(SELECT sum(json_extract(value,'$.subtotal')) FROM json_each(NEW.snapshot,'$.items')) THEN RAISE(ABORT,'Invalid invoice lines or total') END;
        END;
        CREATE TABLE IF NOT EXISTS company_identity(id INTEGER PRIMARY KEY CHECK(id=1),uuid TEXT NOT NULL UNIQUE CHECK(length(uuid)=36),created TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        INSERT OR IGNORE INTO company_identity(id,uuid) SELECT 1,lower(hex(randomblob(4))||'-'||hex(randomblob(2))||'-4'||substr(hex(randomblob(2)),2)||'-'||substr('89ab',abs(random()%4)+1,1)||substr(hex(randomblob(2)),2)||'-'||hex(randomblob(6)));
        CREATE TABLE IF NOT EXISTS invoice_headers(
            invoice INTEGER PRIMARY KEY REFERENCES invoices(id),
            customer_id INTEGER REFERENCES customers(id),customer_name TEXT NOT NULL,
            title TEXT NOT NULL,currency TEXT NOT NULL,total INTEGER NOT NULL CHECK(typeof(total)='integer' AND total>=0),
            issued_at TEXT NOT NULL,company_json TEXT NOT NULL CHECK(json_valid(company_json)));
        CREATE INDEX IF NOT EXISTS invoice_headers_customer_date_idx ON invoice_headers(customer_id,issued_at);
        CREATE TABLE IF NOT EXISTS invoice_lines(
            invoice INTEGER NOT NULL REFERENCES invoice_headers(invoice),line_no INTEGER NOT NULL CHECK(line_no>0),
            source_task INTEGER REFERENCES tasks(id),description TEXT NOT NULL,unit TEXT NOT NULL,
            quantity INTEGER NOT NULL CHECK(typeof(quantity)='integer' AND quantity>0),
            unit_price INTEGER NOT NULL CHECK(typeof(unit_price)='integer' AND unit_price>=0),
            subtotal INTEGER NOT NULL CHECK(typeof(subtotal)='integer' AND subtotal>=0 AND subtotal=quantity*unit_price),
            PRIMARY KEY(invoice,line_no));
        CREATE INDEX IF NOT EXISTS items_task_idx ON items(task);
        CREATE INDEX IF NOT EXISTS expenses_task_idx ON expenses(task);
        CREATE INDEX IF NOT EXISTS ledger_expenses_date_idx ON ledger_expenses(date);
        CREATE TABLE IF NOT EXISTS audit_log(
            id INTEGER PRIMARY KEY,occurred_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            entity TEXT NOT NULL,record_id TEXT NOT NULL,action TEXT NOT NULL,
            before_json TEXT,after_json TEXT,actor TEXT NOT NULL DEFAULT 'local',
            CHECK(before_json IS NULL OR json_valid(before_json)),CHECK(after_json IS NULL OR json_valid(after_json)));
        CREATE INDEX IF NOT EXISTS audit_log_entity_idx ON audit_log(entity,record_id,id);
        CREATE TRIGGER IF NOT EXISTS audit_no_update BEFORE UPDATE ON audit_log BEGIN SELECT RAISE(ABORT,'Audit records are append-only'); END;
        CREATE TRIGGER IF NOT EXISTS audit_no_delete BEFORE DELETE ON audit_log BEGIN SELECT RAISE(ABORT,'Audit records are append-only'); END;
    "#)?;
    // Restore only genuine creation events; task_date is a user-selected business date.
    c.execute_batch("UPDATE tasks SET created_at=(SELECT min(occurred_at) FROM audit_log WHERE entity='tasks' AND action='INSERT' AND record_id=CAST(tasks.id AS TEXT)) WHERE created_at IS NULL AND EXISTS(SELECT 1 FROM audit_log WHERE entity='tasks' AND action='INSERT' AND record_id=CAST(tasks.id AS TEXT));
        CREATE TRIGGER IF NOT EXISTS task_creation_stamp AFTER INSERT ON tasks WHEN NEW.created_at IS NULL BEGIN UPDATE tasks SET created_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=NEW.id; END;
        CREATE TRIGGER IF NOT EXISTS task_creation_fixed BEFORE UPDATE OF created_at ON tasks WHEN OLD.created_at IS NOT NULL AND NEW.created_at IS NOT OLD.created_at BEGIN SELECT RAISE(ABORT,'Task creation timestamp is immutable'); END;")?;
    // Validate and backfill without rewriting immutable historical JSON snapshots.
    let invoices = {
        let mut stmt = c.prepare("SELECT id,snapshot FROM invoices WHERE id NOT IN (SELECT invoice FROM invoice_headers)")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for (id, raw) in invoices {
        let data = serde_json::from_str(&raw).context("历史发票 JSON 损坏，升级已停止")?;
        crate::invoice::validate(&data)
            .with_context(|| format!("历史发票 #{id} 校验失败，升级已停止"))?;
        c.execute(
            &format!("INSERT INTO invoice_headers {}", header_select("id=?")),
            [id],
        )?;
        c.execute(
            &format!("INSERT INTO invoice_lines {}", lines_select("i.id=?")),
            [id],
        )?;
    }
    // Materialize each newly created invoice in its existing transaction.
    c.execute_batch(&format!(r#"
        CREATE TRIGGER IF NOT EXISTS invoice_projection AFTER INSERT ON invoices BEGIN
            INSERT INTO invoice_headers {};
            INSERT INTO invoice_lines {};
        END;
        CREATE TRIGGER IF NOT EXISTS invoice_currency_projection AFTER UPDATE OF snapshot ON invoices BEGIN
            UPDATE invoice_headers SET currency=COALESCE(json_extract(NEW.snapshot,'$.currency'),'UNSET') WHERE invoice=NEW.id;
        END;
        CREATE VIEW IF NOT EXISTS invoice_balances AS
            SELECT h.*,i.number,COALESCE(p.received,0) AS received,h.total-COALESCE(p.received,0) AS remaining
            FROM invoice_headers h JOIN invoices i ON i.id=h.invoice
            LEFT JOIN (SELECT invoice,sum(amount) AS received FROM payments WHERE voided=0 GROUP BY invoice) p ON p.invoice=h.invoice;
        DROP VIEW IF EXISTS expense_register;
        CREATE VIEW expense_register AS
            SELECT l.*,a.code AS account_code,a.name AS account_name,v.name AS vendor_name,t.number AS task_number,d.id AS document_id,d.number AS document_number,d.reference,d.due_date,d.memo AS document_memo
            FROM ledger_expenses l JOIN accounts a ON a.id=l.account LEFT JOIN vendors v ON v.id=l.vendor LEFT JOIN tasks t ON t.id=l.task LEFT JOIN expense_document_lines x ON x.expense=l.id LEFT JOIN expense_documents d ON d.id=x.document;
    "#,header_select("id=NEW.id"),lines_select("i.id=NEW.id")))?;
    constraints(c)?;
    for table in [
        "tasks",
        "items",
        "expenses",
        "ledger_expenses",
        "expense_documents",
        "expense_document_lines",
        "invoices",
        "invoice_tasks",
        "payments",
        "receipt_requests",
        "prices",
        "customers",
        "customer_profiles",
        "vendors",
        "vendor_profiles",
        "ap_sources",
        "ap_reviews",
        "ap_banks",
        "vendor_payments",
        "ap_allocations",
        "ap_voids",
        "accounts",
        "number_sequences",
        "settings",
        "company_profile",
        "invoice_template",
        "tax_agencies",
        "tax_items",
        "tax_rates",
        "tax_codes",
        "tax_code_items",
        "tax_assignments",
        "expense_tax",
        "tax_postings",
        "tax_cash",
        "gl_settings",
        "gl_accounts",
        "gl_entries",
        "gl_lines",
        "gl_reversals",
        "gl_sources",
    ] {
        audit_table(c, table)?;
    }
    Ok(())
}
fn header_select(condition: &str) -> String {
    format!("SELECT id,COALESCE(json_extract(snapshot,'$.customer_id'),(SELECT customer_id FROM tasks WHERE tasks.id=invoices.task)),json_extract(snapshot,'$.customer'),json_extract(snapshot,'$.title'),COALESCE(json_extract(snapshot,'$.currency'),'UNSET'),json_extract(snapshot,'$.total'),created,COALESCE(json_extract(snapshot,'$.company'),'{{}}') FROM invoices WHERE {condition}")
}
fn lines_select(condition: &str) -> String {
    format!("SELECT i.id,CAST(j.key AS INTEGER)+1,COALESCE(json_extract(j.value,'$.source_task'),i.task),json_extract(j.value,'$.description'),json_extract(j.value,'$.unit'),json_extract(j.value,'$.quantity'),json_extract(j.value,'$.unit_price'),json_extract(j.value,'$.subtotal') FROM invoices i,json_each(i.snapshot,'$.items') j WHERE {condition}")
}
fn constraints(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
        CREATE TRIGGER IF NOT EXISTS expense_document_required_insert BEFORE INSERT ON expense_documents WHEN NEW.vendor IS NULL OR length(trim(NEW.date))=0 OR NEW.date IS NULL BEGIN SELECT RAISE(ABORT,'Expense document requires vendor and date'); END;
        CREATE TRIGGER IF NOT EXISTS expense_document_required_update BEFORE UPDATE OF vendor,date ON expense_documents WHEN NEW.vendor IS NULL OR length(trim(NEW.date))=0 OR NEW.date IS NULL BEGIN SELECT RAISE(ABORT,'Expense document requires vendor and date'); END;
        CREATE TRIGGER IF NOT EXISTS company_uuid_fixed BEFORE UPDATE OF uuid ON company_identity BEGIN SELECT RAISE(ABORT,'Company identity cannot change'); END;
        CREATE TRIGGER IF NOT EXISTS company_uuid_no_delete BEFORE DELETE ON company_identity BEGIN SELECT RAISE(ABORT,'Company identity cannot be deleted'); END;
        CREATE TRIGGER IF NOT EXISTS expense_required_insert BEFORE INSERT ON ledger_expenses WHEN NEW.vendor IS NULL OR typeof(NEW.amount)!='integer' OR NEW.amount<=0 BEGIN SELECT RAISE(ABORT,'Expense requires a vendor and positive integer amount'); END;
        CREATE TRIGGER IF NOT EXISTS expense_required_update BEFORE UPDATE OF vendor,amount ON ledger_expenses WHEN NEW.vendor IS NULL OR typeof(NEW.amount)!='integer' OR NEW.amount<=0 BEGIN SELECT RAISE(ABORT,'Expense requires a vendor and positive integer amount'); END;
        CREATE TRIGGER IF NOT EXISTS payment_amount_guard BEFORE INSERT ON payments WHEN typeof(NEW.amount)!='integer' OR NEW.amount<=0 OR NEW.voided!=0 OR NEW.voided_on IS NOT NULL OR NEW.amount>(SELECT total-COALESCE((SELECT sum(amount) FROM payments WHERE invoice=NEW.invoice AND voided=0),0) FROM invoice_headers WHERE invoice=NEW.invoice) BEGIN SELECT RAISE(ABORT,'Invalid payment or payment exceeds invoice balance'); END;
        CREATE TRIGGER IF NOT EXISTS payment_fixed BEFORE UPDATE OF invoice,date,amount,method,reference ON payments BEGIN SELECT RAISE(ABORT,'Void incorrect payments instead of editing'); END;
        CREATE TRIGGER IF NOT EXISTS payment_void_guard BEFORE UPDATE OF voided,void_reason,voided_on ON payments WHEN OLD.voided=1 OR NEW.voided!=1 OR length(trim(NEW.void_reason))=0 OR NEW.voided_on IS NULL OR NEW.voided_on<OLD.date BEGIN SELECT RAISE(ABORT,'Payment void requires a dated reason and cannot be reversed'); END;
        CREATE TRIGGER IF NOT EXISTS payment_no_delete BEFORE DELETE ON payments BEGIN SELECT RAISE(ABORT,'Void payments instead of deleting'); END;
        CREATE TRIGGER IF NOT EXISTS payment_sync_insert AFTER INSERT ON payments WHEN EXISTS(SELECT 1 FROM invoice_headers WHERE invoice=NEW.invoice) BEGIN UPDATE invoices SET paid=((SELECT total FROM invoice_headers WHERE invoice=NEW.invoice)=(SELECT COALESCE(sum(amount),0) FROM payments WHERE invoice=NEW.invoice AND voided=0)) WHERE id=NEW.invoice; END;
        CREATE TRIGGER IF NOT EXISTS payment_sync_void AFTER UPDATE OF voided ON payments BEGIN UPDATE invoices SET paid=((SELECT total FROM invoice_headers WHERE invoice=NEW.invoice)=(SELECT COALESCE(sum(amount),0) FROM payments WHERE invoice=NEW.invoice AND voided=0)) WHERE id=NEW.invoice; END;
        CREATE TRIGGER IF NOT EXISTS invoice_snapshot_fixed BEFORE UPDATE OF snapshot ON invoices
            WHEN NOT (COALESCE(json_extract(OLD.snapshot,'$.currency'),'UNSET')='UNSET' AND NEW.snapshot=json_set(OLD.snapshot,'$.currency',json_extract(NEW.snapshot,'$.currency')))
            BEGIN SELECT RAISE(ABORT,'Invoice snapshot is immutable'); END;
        CREATE TRIGGER IF NOT EXISTS invoice_no_delete BEFORE DELETE ON invoices BEGIN SELECT RAISE(ABORT,'Invoices cannot be deleted'); END;
    "#)?;
    for table in [
        "tasks",
        "invoices",
        "ledger_expenses",
        "payments",
        "expense_documents",
    ] {
        c.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS {table}_number_fixed BEFORE UPDATE OF number ON {table} WHEN OLD.number IS NOT NULL AND NEW.number IS NOT OLD.number BEGIN SELECT RAISE(ABORT,'Document number cannot change'); END;"))?;
    }
    c.execute_batch("CREATE TRIGGER IF NOT EXISTS gl_entries_number_fixed BEFORE UPDATE OF number ON gl_entries WHEN OLD.number IS NOT NULL AND NEW.number IS NOT OLD.number BEGIN SELECT RAISE(ABORT,'Document number cannot change'); END;")?;
    c.execute_batch("CREATE TRIGGER IF NOT EXISTS invoice_task_fixed BEFORE UPDATE OF task ON invoices WHEN NEW.task IS NOT OLD.task BEGIN SELECT RAISE(ABORT,'Invoice task source is fixed'); END;")?;
    for table in ["prices", "items"] {
        for operation in ["INSERT", "UPDATE"] {
            c.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS {table}_money_{operation} BEFORE {operation} ON {table} WHEN typeof(NEW.price)!='integer' OR NEW.price<0 OR typeof(NEW.cost)!='integer' OR NEW.cost<0 BEGIN SELECT RAISE(ABORT,'Money must be nonnegative integer cents'); END;"))?;
        }
    }
    Ok(())
}

fn audit_table(c: &Connection, table: &str) -> Result<()> {
    let columns = {
        let mut stmt = c.prepare(&format!("PRAGMA table_info({table})"))?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let object = |prefix: &str| {
        format!(
            "json_object({})",
            columns
                .iter()
                .map(|col| format!("'{col}',{prefix}.\"{col}\""))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let old = object("OLD");
    let new = object("NEW");
    let key = if table == "customer_profiles" {
        "customer"
    } else if table == "vendor_profiles" {
        "vendor"
    } else if table == "expense_tax" {
        "expense"
    } else if table == "tax_code_items" {
        "code"
    } else if table == "tax_assignments" {
        "record"
    } else if table == "expense_document_lines" {
        "expense"
    } else if table == "invoice_tasks" {
        "task"
    } else if table == "number_sequences" {
        "kind"
    } else {
        "id"
    };
    for (operation, before, after, record, when) in [
        (
            "INSERT",
            "NULL".to_owned(),
            new.clone(),
            format!("NEW.{key}"),
            String::new(),
        ),
        (
            "UPDATE",
            old.clone(),
            new,
            format!("NEW.{key}"),
            format!(
                "WHEN {}",
                columns
                    .iter()
                    .map(|col| format!("OLD.\"{col}\" IS NOT NEW.\"{col}\""))
                    .collect::<Vec<_>>()
                    .join(" OR ")
            ),
        ),
        (
            "DELETE",
            old,
            "NULL".to_owned(),
            format!("OLD.{key}"),
            String::new(),
        ),
    ] {
        c.execute_batch(&format!("DROP TRIGGER IF EXISTS audit_{table}_{operation}; CREATE TRIGGER audit_{table}_{operation} AFTER {operation} ON {table} {when} BEGIN INSERT INTO audit_log(entity,record_id,action,before_json,after_json,actor) VALUES('{table}',CAST({record} AS TEXT),'{operation}',{before},{after},axibiz_actor()); END;"))?;
    }
    Ok(())
}

pub fn validate_projections(c: &Connection) -> Result<()> {
    crate::billing::validate_payment_dates(c)?;
    crate::ap::validate(c)?;
    crate::receipts::validate(c)?;
    crate::gl::validate(c)?;
    crate::tax::validate(c)?;
    let direct_invalid:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM invoices i WHERE i.task IS NULL AND (json_extract(i.snapshot,'$.origin') IS NOT 'direct' OR json_array_length(i.snapshot,'$.tasks') IS NOT 0 OR EXISTS(SELECT 1 FROM invoice_tasks t WHERE t.invoice=i.id) OR EXISTS(SELECT 1 FROM invoice_lines l WHERE l.invoice=i.id AND l.source_task IS NOT NULL))) OR EXISTS(SELECT 1 FROM invoice_requests r JOIN invoices i ON i.id=r.invoice WHERE i.task IS NOT NULL OR json_extract(i.snapshot,'$.origin') IS NOT 'direct' OR json_extract(r.request_json,'$.request_key') IS NOT r.request_key)",[],|r|r.get(0))?;
    ensure!(!direct_invalid, "独立发票来源或请求关联不一致");
    let invalid_expense:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM ledger_expenses l LEFT JOIN expense_document_lines x ON x.expense=l.id LEFT JOIN expense_documents d ON d.id=x.document WHERE d.id IS NULL OR d.vendor IS NOT l.vendor OR d.currency IS NOT l.currency OR d.date IS NOT l.date) OR EXISTS(SELECT 1 FROM expense_documents d WHERE NOT EXISTS(SELECT 1 FROM expense_document_lines WHERE document=d.id))",[],|r|r.get(0))?;
    ensure!(!invalid_expense, "费用单与费用明细不一致");
    let invalid:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM invoices i LEFT JOIN invoice_headers h ON h.invoice=i.id WHERE h.invoice IS NULL OR h.total IS NOT json_extract(i.snapshot,'$.total') OR h.customer_name IS NOT json_extract(i.snapshot,'$.customer') OR h.currency IS NOT COALESCE(json_extract(i.snapshot,'$.currency'),'UNSET') OR (SELECT count(*) FROM invoice_lines WHERE invoice=i.id)!=json_array_length(i.snapshot,'$.items') OR h.total-COALESCE(json_extract(i.snapshot,'$.tax_total'),0)!=(SELECT sum(subtotal) FROM invoice_lines WHERE invoice=i.id))",[],|r|r.get(0))?;
    ensure!(!invalid, "发票结构化数据与历史快照不一致");
    let inconsistent:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM ledger_expenses l LEFT JOIN expenses e ON e.id=l.task_expense WHERE l.task_expense IS NOT NULL AND (e.id IS NULL OR e.task IS NOT l.task OR e.amount IS NOT l.task_amount OR e.description IS NOT l.description))",[],|r|r.get(0))?;
    ensure!(!inconsistent, "费用登记与旧版兼容记录不一致");
    for (table, expected) in [
        ("invoice_headers", header_select("1")),
        ("invoice_lines", lines_select("1")),
    ] {
        let different: bool = c.query_row(
            &format!("SELECT EXISTS(SELECT * FROM {table} EXCEPT {expected})"),
            [],
            |r| r.get(0),
        )?;
        ensure!(!different, "{table} 与发票快照不一致");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn legacy_invoice_db(task: i64) -> Result<(Connection, String)> {
        let c = Connection::open_in_memory()?;
        c.execute_batch("PRAGMA foreign_keys=OFF;")?;
        c.execute_batch("CREATE TABLE customers(id INTEGER PRIMARY KEY,name TEXT NOT NULL,contact TEXT NOT NULL);INSERT INTO customers VALUES(1,'Customer','');CREATE TABLE tasks(id INTEGER PRIMARY KEY,title TEXT NOT NULL,customer TEXT NOT NULL,progress INTEGER NOT NULL DEFAULT 0);INSERT INTO tasks VALUES(1,'Legacy','Customer',100);CREATE TABLE invoices(id INTEGER PRIMARY KEY,task INTEGER NOT NULL UNIQUE REFERENCES tasks(id),created TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,snapshot TEXT NOT NULL,paid INTEGER NOT NULL DEFAULT 0,number TEXT);CREATE UNIQUE INDEX invoices_number_idx ON invoices(number);CREATE VIEW legacy_invoice_view AS SELECT id,number FROM invoices;CREATE TRIGGER legacy_paid AFTER UPDATE OF paid ON invoices BEGIN SELECT NEW.id;END;PRAGMA user_version=11;")?;
        let raw=serde_json::json!({"task":1,"title":"Legacy","customer":"Customer","customer_id":1,"currency":"CAD","items":[{"description":"Service","unit":"item","quantity":1,"unit_price":10000,"subtotal":10000}],"total":10000}).to_string();
        c.execute("INSERT INTO invoices(id,task,created,snapshot,number) VALUES(7,?,'2025-01-01 12:00:00',?,'OLD0007')",rusqlite::params![task,raw])?;
        Ok((c, raw))
    }
    #[test]
    fn nullable_invoice_migration_preserves_history_views_indexes_and_fk_state() -> Result<()> {
        let (c, raw) = legacy_invoice_db(1)?;
        crate::init(&c)?;
        assert_eq!(
            c.query_row("SELECT snapshot FROM invoices WHERE id=7", [], |r| r
                .get::<_, String>(0))?,
            raw
        );
        assert_eq!(
            c.query_row(
                "SELECT number FROM legacy_invoice_view WHERE id=7",
                [],
                |r| r.get::<_, String>(0)
            )?,
            "OLD0007"
        );
        assert_eq!(
            c.query_row(
                "SELECT [notnull] FROM pragma_table_info('invoices') WHERE name='task'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        assert_eq!(
            c.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            c.query_row("SELECT invoice FROM invoice_tasks WHERE task=1", [], |r| {
                r.get::<_, i64>(0)
            })?,
            7
        );
        assert!(c.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='legacy_paid' AND type='trigger')",[],|r|r.get::<_,bool>(0))?);
        assert!(!c.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
            [],
            |r| r.get::<_, bool>(0)
        )?);
        crate::init(&c)?;
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM invoice_tasks", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        Ok(())
    }
    #[test]
    fn failed_invoice_migration_rolls_back_schema_and_restores_foreign_keys() -> Result<()> {
        let (c, raw) = legacy_invoice_db(999)?;
        assert!(crate::init(&c).is_err());
        assert_eq!(
            c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
            11
        );
        assert_eq!(
            c.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            c.query_row(
                "SELECT [notnull] FROM pragma_table_info('invoices') WHERE name='task'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        assert_eq!(
            c.query_row("SELECT snapshot FROM invoices WHERE id=7", [], |r| r
                .get::<_, String>(0))?,
            raw
        );
        assert_eq!(
            c.query_row(
                "SELECT number FROM legacy_invoice_view WHERE id=7",
                [],
                |r| r.get::<_, String>(0)
            )?,
            "OLD0007"
        );
        c.execute("UPDATE invoices SET task=1 WHERE id=7", [])?;
        crate::init(&c)?;
        assert_eq!(
            c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
            crate::SCHEMA_VERSION
        );
        Ok(())
    }
    fn fixture() -> Result<Connection> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer',''); INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor',''); INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','item',0,10000,'CAD');")?;
        crate::add_task(&c, "Task", 1, "CAD")?;
        crate::add_item(&c, 1, 1, 1, None, None)?;
        crate::set_task_status(&mut c, 1, false)?;
        crate::billing::merge(&mut c, &[1])?;
        Ok(c)
    }
    #[test]
    fn projections_receipts_and_database_constraints() -> Result<()> {
        let mut c = fixture()?;
        validate_projections(&c)?;
        assert!(c.execute("INSERT INTO ledger_expenses(account,description,currency,amount) VALUES(1,'Invalid','CAD',1)",[]).is_err());
        assert!(c
            .execute("UPDATE prices SET price=1.5 WHERE id=1", [])
            .is_err());
        assert!(c
            .execute("UPDATE tasks SET number='Changed' WHERE id=1", [])
            .is_err());
        assert!(c
            .execute("UPDATE invoices SET snapshot='{}' WHERE id=1", [])
            .is_err());
        assert!(c.execute("INSERT INTO payments(invoice,date,amount,method) VALUES(1,'2026-10-25',10001,'Cash')",[]).is_err());
        let id =
            crate::billing::add_payment(&mut c, 1, &crate::today().to_string(), "25", "Cash", "")?;
        let (total, received, remaining): (i64, i64, i64) = c.query_row(
            "SELECT total,received,remaining FROM invoice_balances WHERE invoice=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        assert_eq!((total, received, remaining), (10000, 2500, 7500));
        assert!(c
            .execute("UPDATE payments SET amount=1 WHERE id=?", [id])
            .is_err());
        assert!(c.execute("DELETE FROM payments WHERE id=?", [id]).is_err());
        crate::billing::void_payment(&mut c, id, "Correction")?;
        assert_eq!(crate::billing::balance(&c, 1)?.remaining, 10000);
        c.execute(
            "UPDATE invoice_lines SET description='Corrupt' WHERE invoice=1",
            [],
        )?;
        assert!(validate_projections(&c).is_err());
        Ok(())
    }
    #[test]
    fn audit_is_transactional_and_append_only() -> Result<()> {
        let mut c = fixture()?;
        let count: i64 = c.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0))?;
        {
            let tx = c.transaction()?;
            tx.execute("UPDATE customers SET name='Rolled back' WHERE id=1", [])?;
        }
        assert_eq!(
            c.query_row("SELECT count(*) FROM audit_log", [], |r| r.get::<_, i64>(0))?,
            count
        );
        c.execute("UPDATE customers SET name='Changed' WHERE id=1", [])?;
        let (before,after):(String,String)=c.query_row("SELECT json_extract(before_json,'$.name'),json_extract(after_json,'$.name') FROM audit_log WHERE entity='customers' AND action='UPDATE' ORDER BY id DESC LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
        assert_eq!((before.as_str(), after.as_str()), ("Customer", "Changed"));
        assert!(c.execute("DELETE FROM audit_log", []).is_err());
        assert!(c.execute("UPDATE audit_log SET actor='Fake'", []).is_err());
        Ok(())
    }
    #[test]
    fn corrupt_backup_projection_cannot_replace_live_data() -> Result<()> {
        let mut c = fixture()?;
        let root = std::env::temp_dir().join(format!(
            "axibiz-projection-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir(&root)?;
        let path = root.join("bad.db");
        let safety = root.join("safety.db");
        crate::backup::save(&c, &path)?;
        let bad = Connection::open(&path)?;
        bad.execute("UPDATE invoice_lines SET description='Changed backup'", [])?;
        drop(bad);
        assert!(crate::backup::verify(&path).is_err());
        assert!(crate::backup::restore(&mut c, &path, &safety).is_err());
        assert!(!safety.exists());
        validate_projections(&c)?;
        drop(c);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
    #[test]
    fn old_paid_invoice_migrates_without_changing_history() -> Result<()> {
        let c = Connection::open_in_memory()?;
        c.execute_batch(r#"CREATE TABLE tasks(id INTEGER PRIMARY KEY,title TEXT NOT NULL,customer TEXT NOT NULL,progress INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE invoices(id INTEGER PRIMARY KEY,task INTEGER NOT NULL UNIQUE REFERENCES tasks(id),created TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,snapshot TEXT NOT NULL,paid INTEGER NOT NULL DEFAULT 0);
            INSERT INTO tasks VALUES(1,'Task','Current name',100);
            INSERT INTO invoices(task,snapshot,paid) VALUES(1,'{"title":"Original title","customer":"Historical name","currency":"CAD","total":100,"items":[{"description":"Historical service","unit":"item","quantity":1,"unit_price":100,"subtotal":100}]}',1);
            PRAGMA user_version=3;"#)?;
        let raw: String = c.query_row("SELECT snapshot FROM invoices", [], |r| r.get(0))?;
        crate::init(&c)?;
        let uuid = company_id(&c)?;
        assert_eq!(uuid.as_ref().unwrap().len(), 36);
        validate_projections(&c)?;
        assert_eq!(crate::billing::balance(&c, 1)?.received, 100);
        assert_eq!(
            c.query_row("SELECT customer_name FROM invoice_headers", [], |r| r
                .get::<_, String>(0))?,
            "Historical name"
        );
        assert_eq!(
            c.query_row("SELECT snapshot FROM invoices", [], |r| r
                .get::<_, String>(0))?,
            raw
        );
        crate::init(&c)?;
        assert_eq!(company_id(&c)?, uuid);
        assert_eq!(
            c.query_row("SELECT count(*) FROM payments", [], |r| r.get::<_, i64>(0))?,
            1
        );
        assert!(c
            .execute(
                "UPDATE company_identity SET uuid='00000000-0000-4000-8000-000000000000'",
                []
            )
            .is_err());
        Ok(())
    }
}
