pub(crate) const APP_NAME: &str = "Axibiz Desktop Beta";
mod amount;
mod app_settings;
mod auth;
use amount::{format_money, parse_money};
mod accounting;
mod ap;
mod autocomplete;
mod backup;
mod billing;
mod compliance;
mod csv;
mod customer;
mod data_path;
mod expense_doc;
mod fx;
mod gl;
mod gui;
mod instance;
mod invoice;
mod invoice_template;
mod numbering;
mod party_balances;
mod pdf;
mod posting;
mod receipts;
mod reconciliation;
mod security;
mod settings;
mod storage;
mod tax;
mod vendor;
use anyhow::{ensure, Context, Result};
use clap::{Parser, Subcommand};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use settings::Settings;
use std::path::PathBuf;

/// 供应商、价格、任务成本与发票管理。金额最多两位小数，内部以分保存。
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[arg(long, global = true)]
    db: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    /// 指定截止日期的应收和税务控制科目对账，只读
    Reconcile {
        date: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// 只读账务与单据资料检查，不构成合规认证
    AccountingCheck,
    /// 税务期间汇总（CAD）
    TaxSummary {
        from: String,
        to: String,
    },
    /// 分配未来业务默认税码（price/customer/vendor）
    TaxAssign {
        entity: String,
        record: i64,
        code: i64,
    },
    /// 设置既有税务机构注册税号；--registered 表示已注册
    TaxAgencySet {
        id: i64,
        registration: String,
        #[arg(long)]
        registered: bool,
    },
    /// 登记已发生的 CAD 税款缴纳、退款或调整
    TaxRecord {
        agency: i64,
        date: String,
        #[arg(value_parser=["payment","refund","increase","decrease"])]
        kind: String,
        amount: String,
        #[arg(long, default_value = "")]
        reference: String,
        #[arg(long)]
        reason: String,
    },
    /// 查看最近的数据库操作记录（UTC）
    AuditList,
    /// 打开终端交互式公司设置界面
    Settings,
    /// 查看当前公司设置
    SettingsShow,
    /// 直接更新指定设置字段，省略的字段保留
    SettingsSet {
        #[arg(long)]
        company_name: Option<String>,
        #[arg(long)]
        address: Option<String>,
        #[arg(long)]
        contact: Option<String>,
        #[arg(long)]
        tax_number: Option<String>,
        #[arg(long, value_parser = currency)]
        currency: Option<String>,
    },
    /// 添加供应商
    VendorAdd {
        name: String,
        #[arg(long, default_value = "")]
        contact: String,
        #[arg(long, default_value = "CAD", value_parser = currency)]
        currency: String,
    },
    VendorList,
    VendorUpdate {
        id: i64,
        name: String,
        #[arg(long, default_value = "")]
        contact: String,
        #[arg(long, value_parser = currency)]
        currency: Option<String>,
    },
    /// 添加价格单条目（成本价、销售价）
    PriceAdd {
        vendor: i64,
        name: String,
        cost: String,
        price: String,
        #[arg(long, default_value = "项")]
        unit: String,
        #[arg(long, value_parser = currency)]
        currency: Option<String>,
    },
    PriceList,
    PriceUpdate {
        id: i64,
        cost: String,
        price: String,
        #[arg(long, value_parser = currency)]
        currency: Option<String>,
    },
    /// 支持的币种（均使用两位小数）
    CurrencyList,
    /// 指定旧版未标币种数据的原有币种；仅补标签，不换算
    LegacyCurrency {
        #[arg(value_parser = currency)]
        currency: String,
    },
    /// 设置汇率：1 source = rate target，最多八位小数
    FxSet {
        #[arg(value_parser = currency)]
        source: String,
        #[arg(value_parser = currency)]
        target: String,
        rate: String,
    },
    FxList,
    /// 从加拿大央行获取汇率并存入本地
    FxSync,
    /// 添加客户
    CustomerAdd {
        name: String,
        #[arg(long, default_value = "")]
        contact: String,
        #[arg(long, default_value = "CAD", value_parser = currency)]
        currency: String,
    },
    CustomerList,
    CustomerUpdate {
        id: i64,
        name: String,
        #[arg(long, default_value = "")]
        contact: String,
        #[arg(long, value_parser = currency)]
        currency: Option<String>,
    },
    /// 添加任务，选择 customer-list 中的客户 ID
    TaskAdd {
        title: String,
        /// 任务日期 YYYY-MM-DD，默认温哥华当天日期
        #[arg(long, value_parser = task_date)]
        date: Option<String>,
        customer: i64,
        #[arg(long, value_delimiter = ',')]
        items: Vec<i64>,
        #[arg(long, value_parser = currency)]
        currency: Option<String>,
    },
    TaskList,
    TaskShow {
        id: i64,
    },
    /// 从价格单添加任务明细，价格保存为快照
    TaskItem {
        task: i64,
        price_id: i64,
        #[arg(default_value_t = 1)]
        quantity: i64,
        #[arg(long)]
        price: Option<String>,
        #[arg(long)]
        cost: Option<String>,
    },
    /// 批量从价格单添加业务，每项数量默认为 1
    TaskItems {
        task: i64,
        #[arg(required = true, value_delimiter = ',')]
        price_ids: Vec<i64>,
    },
    /// 修改当前任务明细的价格、成本或数量（不修改价格单）
    TaskItemUpdate {
        task: i64,
        item: i64,
        #[arg(long)]
        price: Option<String>,
        #[arg(long)]
        cost: Option<String>,
        #[arg(long)]
        quantity: Option<i64>,
    },
    /// 添加额外成本，不增加销售金额
    TaskExpense {
        task: i64,
        description: String,
        amount: String,
        #[arg(long)]
        vendor: i64,
        #[arg(long, default_value_t = 0)]
        tax_code: i64,
        #[arg(long)]
        tax_included: bool,
        #[arg(long, default_value_t = 100)]
        recovery: u8,
    },
    /// 更新进度（0 到 100），100 表示完成
    TaskProgress {
        task: i64,
        progress: i64,
    },
    /// 标记任务完成
    TaskComplete {
        task: i64,
    },
    /// 取消未开票任务
    TaskCancel {
        task: i64,
    },
    /// 将任务转换成发票，重复操作返回原发票
    InvoiceCreate {
        task: i64,
    },
    InvoiceList,
    /// 合并同一客户且同币种的 completed Task
    InvoiceMerge {
        #[arg(required = true, value_delimiter = ',')]
        tasks: Vec<i64>,
    },
    /// 记录发票付款（发票币种）
    PaymentAdd {
        invoice: i64,
        date: String,
        amount: String,
        #[arg(long, default_value = "Other")]
        method: String,
        #[arg(long, default_value = "")]
        reference: String,
    },
    PaymentVoid {
        id: i64,
        reason: String,
    },
    NumberingList,
    NumberingSet {
        #[arg(value_parser=["task","invoice","expense","payment","journal"])]
        kind: String,
        prefix: String,
        #[arg(long, default_value_t = 4)]
        width: usize,
        #[arg(long, default_value_t = 1)]
        next: i64,
    },
    /// 导出自包含 HTML 发票，可在浏览器打印成 PDF
    InvoiceExport {
        id: i64,
        output: PathBuf,
    },
    /// 标记发票已支付
    InvoicePay {
        id: i64,
    },
}
const TASK_STATUS_SQL:&str="CASE WHEN i.id IS NOT NULL THEN 'invoiced' WHEN t.cancelled=1 THEN 'cancel' WHEN t.progress=100 THEN 'completed' ELSE 'pending' END";
const TASK_ORDER_SQL: &str =
    "CASE WHEN i.id IS NOT NULL THEN 2 WHEN t.cancelled=1 THEN 3 WHEN t.progress=100 THEN 1 ELSE 0 END,t.id DESC";
const FX_SCALE: i64 = 100_000_000;
fn currency(s: &str) -> std::result::Result<String, String> {
    let s = s.trim().to_ascii_uppercase();
    if ["CAD", "USD", "CNY", "EUR", "GBP", "HKD", "AUD", "SGD"].contains(&s.as_str()) {
        Ok(s)
    } else {
        Err("支持币种：CAD USD CNY EUR GBP HKD AUD SGD".into())
    }
}
fn rate(s: &str) -> Result<i64> {
    let parts: Vec<_> = s.split('.').collect();
    ensure!(
        parts.len() <= 2
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit())),
        "汇率必须为正十进制数"
    );
    let whole: i64 = parts[0].parse()?;
    let fraction = if parts.len() == 2 {
        ensure!(parts[1].len() <= 8, "汇率最多八位小数");
        parts[1].parse::<i64>()? * 10i64.pow(8 - parts[1].len() as u32)
    } else {
        0
    };
    let value = whole
        .checked_mul(FX_SCALE)
        .and_then(|n| n.checked_add(fraction))
        .context("汇率过大")?;
    ensure!(value > 0, "汇率必须大于零");
    Ok(value)
}
fn convert(amount: i64, fx: i64) -> Result<i64> {
    ensure!(amount >= 0 && fx > 0, "金额或汇率无效");
    i64::try_from(
        (i128::from(amount) * i128::from(fx) + i128::from(FX_SCALE / 2)) / i128::from(FX_SCALE),
    )
    .context("换算金额过大")
}
fn exchange(c: &Connection, source: &str, target: &str) -> Result<i64> {
    ensure!(
        source != "UNSET" && target != "UNSET",
        "旧数据尚未指定币种，请先运行 legacy-currency"
    );
    if source == target {
        return Ok(FX_SCALE);
    }
    fx::resolve(c, source, target, fx::fetch)
}
fn required(s: &str) -> Result<()> {
    ensure!(!s.trim().is_empty(), "名称或描述不能为空");
    Ok(())
}
const SCHEMA_VERSION: i64 = 21;

fn init(c: &Connection) -> Result<()> {
    auth::register(c)?;
    ensure!(c.is_autocommit(), "数据库初始化须在业务事务之外执行");
    let version: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    ensure!(
        version <= SCHEMA_VERSION,
        "This database requires the same or a newer version of Axibiz Desktop Beta."
    );
    c.execute_batch("PRAGMA foreign_keys=ON")?;
    let rebuild:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('invoices') WHERE name='task' AND [notnull]=1)",[],|r|r.get(0))?;
    if rebuild {
        c.execute_batch("PRAGMA foreign_keys=OFF")?;
    }
    let result = init_inner(c, rebuild);
    c.execute_batch("PRAGMA foreign_keys=ON")?;
    if result.is_ok() {
        auth::register(c)?;
    }
    result
}
fn init_inner(c: &Connection, rebuild: bool) -> Result<()> {
    let version: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    ensure!(
        version <= SCHEMA_VERSION,
        "This database requires the same or a newer version of Axibiz Desktop Beta."
    );
    c.execute_batch("
    CREATE TABLE IF NOT EXISTS customers(id INTEGER PRIMARY KEY,name TEXT NOT NULL,contact TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS vendors(id INTEGER PRIMARY KEY,name TEXT NOT NULL,contact TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS prices(id INTEGER PRIMARY KEY,vendor INTEGER NOT NULL REFERENCES vendors(id),name TEXT NOT NULL,unit TEXT NOT NULL,cost INTEGER NOT NULL CHECK(cost>=0),price INTEGER NOT NULL CHECK(price>=0));
    CREATE TABLE IF NOT EXISTS tasks(id INTEGER PRIMARY KEY,title TEXT NOT NULL,customer TEXT NOT NULL,progress INTEGER NOT NULL DEFAULT 0 CHECK(progress BETWEEN 0 AND 100));
    CREATE TABLE IF NOT EXISTS items(id INTEGER PRIMARY KEY,task INTEGER NOT NULL REFERENCES tasks(id),description TEXT NOT NULL,unit TEXT NOT NULL,quantity INTEGER NOT NULL CHECK(quantity>0),cost INTEGER NOT NULL,price INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS expenses(id INTEGER PRIMARY KEY,task INTEGER NOT NULL REFERENCES tasks(id),description TEXT NOT NULL,amount INTEGER NOT NULL CHECK(amount>=0));
    CREATE TABLE IF NOT EXISTS invoices(id INTEGER PRIMARY KEY,task INTEGER UNIQUE REFERENCES tasks(id),created TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,snapshot TEXT NOT NULL,paid INTEGER NOT NULL DEFAULT 0);")?;
    // Migrate legacy names atomically.
    let tx = c.unchecked_transaction()?;
    if rebuild {
        storage::rebuild_invoice_anchor(&tx)?;
    }
    let has_customer_id: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tasks') WHERE name='customer_id')",
        [],
        |r| r.get(0),
    )?;
    if !has_customer_id {
        tx.execute_batch("ALTER TABLE tasks ADD COLUMN customer_id INTEGER REFERENCES customers(id);
        INSERT INTO customers(name,contact) SELECT DISTINCT customer,'' FROM tasks;
        UPDATE tasks SET customer_id=(SELECT id FROM customers WHERE name=tasks.customer ORDER BY id LIMIT 1);")?;
    }
    tx.execute_batch(
        "CREATE INDEX IF NOT EXISTS tasks_customer_idx ON tasks(customer_id);
    CREATE TRIGGER IF NOT EXISTS tasks_require_customer_insert BEFORE INSERT ON tasks
    WHEN NEW.customer_id IS NULL BEGIN SELECT RAISE(ABORT,'customer_id required'); END;
    CREATE TRIGGER IF NOT EXISTS tasks_require_customer_update BEFORE UPDATE OF customer_id ON tasks
    WHEN NEW.customer_id IS NULL BEGIN SELECT RAISE(ABORT,'customer_id required'); END;",
    )?;
    for (table, column, definition) in [
        ("customers", "currency", "TEXT NOT NULL DEFAULT 'CAD' CHECK(currency IN ('CAD','USD','CNY','EUR','GBP','HKD','AUD','SGD'))"),
        ("vendors", "currency", "TEXT NOT NULL DEFAULT 'CAD' CHECK(currency IN ('CAD','USD','CNY','EUR','GBP','HKD','AUD','SGD'))"),
        ("prices", "currency", "TEXT NOT NULL DEFAULT 'UNSET'"),
        ("tasks", "currency", "TEXT NOT NULL DEFAULT 'UNSET'"),
        ("tasks", "task_date", "TEXT"),
        ("tasks", "created_at", "TEXT"),
        (
            "tasks",
            "cancelled",
            "INTEGER NOT NULL DEFAULT 0 CHECK(cancelled IN (0,1))",
        ),
        ("items", "source_currency", "TEXT NOT NULL DEFAULT 'UNSET'"),
        ("items", "source_cost", "INTEGER"),
        ("items", "source_price", "INTEGER"),
        ("items", "vendor_id", "INTEGER REFERENCES vendors(id)"),
        ("items", "vendor_name", "TEXT"),
        ("items", "fx_rate", "INTEGER NOT NULL DEFAULT 100000000"),
        ("items", "converted_at", "TEXT"),
    ] {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?) WHERE name=?)",
            params![table, column],
            |r| r.get(0),
        )?;
        if !exists {
            tx.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN {column} {definition};"
            ))?;
        }
    }
    tx.execute_batch("CREATE TABLE IF NOT EXISTS fx_rates(source TEXT NOT NULL,target TEXT NOT NULL,rate INTEGER NOT NULL CHECK(rate>0),updated TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,PRIMARY KEY(source,target));")?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS settings(id INTEGER PRIMARY KEY CHECK(id=1),company_name TEXT NOT NULL DEFAULT '',address TEXT NOT NULL DEFAULT '',contact TEXT NOT NULL DEFAULT '',tax_number TEXT NOT NULL DEFAULT '',currency TEXT NOT NULL DEFAULT 'CAD');
        INSERT OR IGNORE INTO settings(id) VALUES(1);")?;
    settings::init(&tx)?;
    fx::init(&tx)?;
    storage::prepare_legacy(&tx)?;
    accounting::init(&tx)?;
    expense_doc::init(&tx)?;
    tax::init(&tx)?;
    accounting::seed_template(&tx)?;
    gl::init(&tx)?;
    ap::init(&tx)?;
    billing::init(&tx)?;
    receipts::init(&tx)?;
    invoice_template::init(&tx)?;
    customer::init(&tx)?;
    vendor::init(&tx)?;
    auth::init(&tx)?;
    storage::init(&tx)?;
    if version < SCHEMA_VERSION {
        storage::validate_projections(&tx)?;
    }
    let invalid: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
        [],
        |r| r.get(0),
    )?;
    ensure!(!invalid, "数据库外键检查失败，升级已回滚");
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}
fn today() -> chrono::NaiveDate {
    chrono::Utc::now()
        .with_timezone(&chrono_tz::America::Vancouver)
        .date_naive()
}
fn task_age_days(task_date_text: Option<&str>, as_of: chrono::NaiveDate) -> Option<i64> {
    let text = task_date_text?;
    task_date(text).ok()?;
    let date = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()?;
    Some(as_of.signed_duration_since(date).num_days().max(0))
}
fn task_date(s: &str) -> std::result::Result<String, String> {
    let date = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| "日期无效，请使用 YYYY-MM-DD".to_owned())?;
    if date.format("%Y-%m-%d").to_string() != s || s.len() != 10 {
        return Err("请使用 YYYY-MM-DD 格式".into());
    }
    Ok(s.to_owned())
}
#[cfg(test)]
fn add_task(c: &Connection, title: &str, customer: i64, currency: &str) -> Result<i64> {
    add_task_on(c, title, customer, currency, &today().to_string())
}
fn add_task_on(
    c: &Connection,
    title: &str,
    customer: i64,
    currency: &str,
    date: &str,
) -> Result<i64> {
    task_date(date).map_err(anyhow::Error::msg)?;
    required(title)?;
    let count = c.execute("INSERT INTO tasks(title,customer,customer_id,currency,task_date,created_at) SELECT ?,name,id,?,?,? FROM customers WHERE id=? AND active=1", params![title,currency,date,chrono::Utc::now().to_rfc3339(),customer])?;
    ensure!(
        count == 1,
        "客户不存在，请使用 customer-list 选择客户或 customer-add 添加客户"
    );
    let id = c.last_insert_rowid();
    numbering::assign(c, "task", id)?;
    Ok(id)
}
fn add_item(
    c: &Connection,
    task: i64,
    price_id: i64,
    quantity: i64,
    price: Option<i64>,
    cost: Option<i64>,
) -> Result<i64> {
    editable(c, task)?;
    ensure!(quantity > 0, "数量必须大于零");
    let vendor: i64 = c.query_row("SELECT vendor FROM prices WHERE id=?", [price_id], |r| {
        r.get(0)
    })?;
    vendor::allow_usage(c, vendor, None)?;
    let (name, unit, base_cost, base_price): (String, String, i64, i64) = c
        .query_row(
            "SELECT name,unit,cost,price FROM prices WHERE id=?",
            [price_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .context("价格条目不存在")?;
    let source: String =
        c.query_row("SELECT currency FROM prices WHERE id=?", [price_id], |r| {
            r.get(0)
        })?;
    let target: String = c.query_row("SELECT currency FROM tasks WHERE id=?", [task], |r| {
        r.get(0)
    })?;
    let fx = exchange(c, &source, &target)?;
    let (cost, price) = (
        cost.unwrap_or(convert(base_cost, fx)?),
        price.unwrap_or(convert(base_price, fx)?),
    );
    ensure!(cost >= 0 && price >= 0, "价格和成本不能为负数");
    c.execute("INSERT INTO items(task,description,unit,quantity,cost,price,source_currency,source_cost,source_price,fx_rate,converted_at) VALUES(?,?,?,?,?,?,?,?,?,?,CURRENT_TIMESTAMP)",params![task,name,unit,quantity,cost,price,source,base_cost,base_price,fx])?;
    let item = c.last_insert_rowid();
    c.execute("UPDATE items SET vendor_id=(SELECT vendor FROM prices WHERE id=?1),vendor_name=(SELECT v.name FROM prices p JOIN vendors v ON v.id=p.vendor WHERE p.id=?1) WHERE id=?2",params![price_id,item])?;
    c.execute("UPDATE items SET tax_code=COALESCE(NULLIF((SELECT tax_code FROM prices WHERE id=?1),0),(SELECT code FROM tax_assignments WHERE entity='customer' AND record=(SELECT customer_id FROM tasks WHERE id=?2)),0) WHERE id=?3",params![price_id,task,item])?;
    if source == target {
        c.execute(
            "UPDATE items SET fx_provider='Same currency' WHERE id=?",
            [item],
        )?;
    } else {
        c.execute("UPDATE items SET fx_provider=(SELECT provider FROM fx_rates WHERE source=? AND target=?),fx_date=(SELECT rate_date FROM fx_rates WHERE source=? AND target=?) WHERE id=?",params![source,target,source,target,item])?;
    }
    snapshot(c, task)?;
    Ok(item)
}
fn update_item(
    c: &Connection,
    task: i64,
    item: i64,
    price: Option<i64>,
    cost: Option<i64>,
    quantity: Option<i64>,
) -> Result<()> {
    editable(c, task)?;
    ensure!(
        price.is_some() || cost.is_some() || quantity.is_some(),
        "请提供 --price、--cost 或 --quantity"
    );
    ensure!(quantity.is_none_or(|q| q > 0), "数量必须大于零");
    ensure!(
        price.is_none_or(|p| p >= 0) && cost.is_none_or(|p| p >= 0),
        "价格和成本不能为负数"
    );
    let count=c.execute("UPDATE items SET price=COALESCE(?,price),cost=COALESCE(?,cost),quantity=COALESCE(?,quantity) WHERE id=? AND task=?",params![price,cost,quantity,item,task])?;
    ensure!(count == 1, "该任务中不存在此明细");
    snapshot(c, task)?;
    Ok(())
}
fn editable(c: &Connection, task: i64) -> Result<()> {
    ensure!(
        c.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?)",
            [task],
            |r| r.get::<_, bool>(0)
        )?,
        "任务不存在"
    );
    ensure!(
        !c.query_row(
            "SELECT EXISTS(SELECT 1 FROM invoice_tasks WHERE task=?)",
            [task],
            |r| r.get::<_, bool>(0)
        )?,
        "任务已开票，明细和费用已锁定"
    );
    ensure!(
        !c.query_row("SELECT cancelled FROM tasks WHERE id=?", [task], |r| r
            .get::<_, bool>(0))?,
        "任务已 cancel，不能修改"
    );
    Ok(())
}
fn changed(n: usize) -> Result<()> {
    ensure!(n > 0, "记录不存在");
    println!("已保存");
    Ok(())
}
fn list(c: &Connection, sql: &str) -> Result<()> {
    let mut stmt = c.prepare(sql)?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut rows = stmt.query([])?;
    println!("{}", names.join(" | "));
    while let Some(r) = rows.next()? {
        let values: Vec<String> = (0..names.len())
            .map(|i| match r.get_ref(i) {
                Ok(rusqlite::types::ValueRef::Integer(v)) => v.to_string(),
                Ok(rusqlite::types::ValueRef::Text(v)) => String::from_utf8_lossy(v).to_string(),
                _ => String::new(),
            })
            .collect();
        println!("{}", values.join(" | "));
    }
    Ok(())
}
fn snapshot(c: &Connection, task: i64) -> Result<Value> {
    let (title, customer, customer_id): (String,String,i64) = c.query_row("SELECT t.title,c.name,c.id FROM tasks t JOIN customers c ON c.id=t.customer_id WHERE t.id=?", [task], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).context("任务不存在")?;
    let status:String=c.query_row(&format!("SELECT {TASK_STATUS_SQL} FROM tasks t LEFT JOIN invoice_tasks it ON it.task=t.id LEFT JOIN invoices i ON i.id=it.invoice WHERE t.id=?"),[task],|r|r.get(0))?;
    let number: Option<String> =
        c.query_row("SELECT number FROM tasks WHERE id=?", [task], |r| r.get(0))?;
    let currency: String = c.query_row("SELECT currency FROM tasks WHERE id=?", [task], |r| {
        r.get(0)
    })?;
    let date: Option<String> =
        c.query_row("SELECT task_date FROM tasks WHERE id=?", [task], |r| {
            r.get(0)
        })?;
    let mut stmt = c.prepare(
        "SELECT description,unit,quantity,cost,price,id FROM items WHERE task=? ORDER BY id",
    )?;
    let rows = stmt.query_map([task], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
        ))
    })?;
    let mut items = Vec::new();
    let mut cost = 0i64;
    let mut total = 0i64;
    for row in rows {
        let (name, unit, qty, unit_cost, p, id) = row?;
        cost = cost
            .checked_add(unit_cost.checked_mul(qty).context("成本过大")?)
            .context("成本过大")?;
        let subtotal = p.checked_mul(qty).context("金额过大")?;
        total = total.checked_add(subtotal).context("金额过大")?;
        let (source_currency,source_cost,source_price,fx_rate,converted_at):(String,Option<i64>,Option<i64>,i64,Option<String>)=c.query_row(
            "SELECT source_currency,source_cost,source_price,fx_rate,converted_at FROM items WHERE id=?",[id],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
        let (fx_provider, fx_date): (Option<String>, Option<String>) = c.query_row(
            "SELECT fx_provider,fx_date FROM items WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let tax_code: i64 =
            c.query_row("SELECT tax_code FROM items WHERE id=?", [id], |r| r.get(0))?;
        let (vendor_id, vendor_name): (Option<i64>, Option<String>) = c.query_row(
            "SELECT vendor_id,vendor_name FROM items WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        items.push(json!({"vendor_id":vendor_id,"vendor_name":vendor_name,"tax_code":tax_code,"fx_provider":fx_provider,"fx_date":fx_date,"source_currency":source_currency,"source_cost":source_cost,"source_price":source_price,"fx_rate":format!("{}.{:08}",fx_rate/FX_SCALE,fx_rate%FX_SCALE),"converted_at":converted_at,"id":id,"description":name,"unit":unit,"quantity":qty,"unit_cost":unit_cost,"unit_price":p,"subtotal":subtotal}));
    }
    // Ledger is canonical; retain a fallback for pre-migration legacy task expenses.
    let mut stmt = c.prepare("SELECT description,task_amount AS amount,id FROM ledger_expenses WHERE task=?1 UNION ALL SELECT description,amount,id FROM expenses e WHERE task=?1 AND NOT EXISTS(SELECT 1 FROM ledger_expenses l WHERE l.task_expense=e.id) ORDER BY id")?;
    let mut expenses = Vec::new();
    for row in stmt.query_map([task], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })? {
        let (description, amount) = row?;
        cost = cost.checked_add(amount).context("成本过大")?;
        expenses.push(json!({"description":description,"amount":amount}));
    }
    Ok(
        json!({"number":number,"status":status,"task_date":date,"currency":currency,"task":task,"title":title,"customer":customer,"customer_id":customer_id,"items":items,"expenses":expenses,"total":total,"cost":cost,"profit":total-cost}),
    )
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn set_task_status(c: &mut Connection, task: i64, cancel: bool) -> Result<()> {
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let (cancelled,invoiced):(bool,bool)=tx.query_row("SELECT cancelled,EXISTS(SELECT 1 FROM invoice_tasks WHERE task=tasks.id) FROM tasks WHERE id=?",[task],|r|Ok((r.get(0)?,r.get(1)?))).context("任务不存在")?;
    ensure!(!invoiced, "任务已 invoiced，不能更改任务状态");
    if cancel {
        tx.execute("UPDATE tasks SET cancelled=1 WHERE id=?", [task])?;
    } else {
        ensure!(!cancelled, "任务已 cancel，不能标记 completed");
        tx.execute("UPDATE tasks SET progress=100 WHERE id=?", [task])?;
    }
    tx.commit()?;
    Ok(())
}
fn create_invoice(c: &mut Connection, task: i64) -> Result<(i64, bool)> {
    if let Some(id) = billing::for_task(c, task)? {
        return Ok((id, false));
    }
    Ok((billing::merge(c, &[task])?, true))
}
fn run() -> Result<()> {
    let cli = Cli::parse();
    let graphical = cli.command.is_none();
    let result = (|| -> Result<()> {
        let db = match cli.db {
            Some(path) => path,
            None => data_path::default_database()?,
        };
        // Claim the database before opening it or running schema/data migrations.
        let _session = instance::start(&db)?;
        security::database(&db)?;
        let mut c = Connection::open(&db).context("无法打开数据库")?;
        auth::register(&c)?;
        c.busy_timeout(std::time::Duration::from_secs(5))?;
        let company = instance::existing_company(&c)?;
        instance::with_company(&company, || Ok(()))?;
        if let Some(id) = storage::company_id(&c)? {
            instance::with_identity(&id, || Ok(()))?;
        }
        if let Some(command) = cli.command {
            auth::cli_sign_in(&c)?;
            prepare_company(&c, &db)?;
            execute(&mut c, command)
        } else {
            gui::launch(c, db)
        }
    })();
    if graphical {
        if let Err(error) = &result {
            rfd::MessageDialog::new()
                .set_title(APP_NAME)
                .set_description(format!("{error:#}"))
                .set_level(rfd::MessageLevel::Warning)
                .set_buttons(rfd::MessageButtons::Ok)
                .show();
        }
    }
    result
}
fn prepare_company(c: &Connection, path: &std::path::Path) -> Result<()> {
    auth::require(c)?;
    let preferences = app_settings::Preferences::load(&app_settings::path()?)?;
    backup::automatic(c, path, &preferences)?;
    init(c)?;
    instance::with_identity(
        &storage::company_id(c)?.context("Company identity is missing")?,
        || Ok(()),
    )?;
    Ok(())
}
fn execute(c: &mut Connection, command: Command) -> Result<()> {
    auth::require(c)?;
    match command {
        Command::Reconcile{date,output}=>{
            let report=reconciliation::build(c,&date)?;
            let rows=report.csv_rows();
            if let Some(path)=output {tax::admin::export(&rows,&reconciliation::CSV_HEADERS,&path)?;}
            for row in rows {println!("{}",row.join(" | "));}
        }
        Command::AccountingCheck => {
            let findings=compliance::review(c)?;
            for finding in &findings {println!("{} {}: {}",if finding.error {"ERROR"} else {"REVIEW"},finding.code,finding.message);}
            println!("检查结束；供应地点、ITC 资格、期初对账及长期归档须另行核对。");
            ensure!(!findings.iter().any(|finding|finding.error),"账务一致性检查发现错误");
        }
        Command::Settings => settings::edit(c)?,
        Command::SettingsShow => Settings::load(c)?.show(),
        Command::SettingsSet { company_name,address,contact,tax_number,currency } => {
            ensure!(company_name.is_some() || address.is_some() || contact.is_some() || tax_number.is_some() || currency.is_some(),"请提供需要修改的设置字段，或运行 settings 打开设置界面");
            let mut s=Settings::load(c)?;
            if let Some(v)=company_name {s.company_name=v;}
            if let Some(v)=address {s.address=v;}
            if let Some(v)=contact {s.contact=v;}
            if let Some(v)=tax_number {s.tax_number=v;}
            if let Some(v)=currency {s.currency=v;}
            s.save(c)?; s.show();
        }
        Command::VendorAdd { name, contact, currency: selected } => { required(&name)?; let selected=currency(&selected).map_err(anyhow::Error::msg)?; c.execute("INSERT INTO vendors(name,contact,currency) VALUES(?,?,?)",params![name,contact,selected])?; println!("Vendor #{}",c.last_insert_rowid()); }
        Command::VendorList => list(c,"SELECT id,name,contact,currency FROM vendors ORDER BY id")?,
        Command::VendorUpdate { id,name,contact,currency:selected } => { required(&name)?; let selected=selected.map(|v|currency(&v).map_err(anyhow::Error::msg)).transpose()?; changed(c.execute("UPDATE vendors SET name=?,contact=?,currency=COALESCE(?,currency) WHERE id=?",params![name,contact,selected,id])?)?; }
        Command::PriceAdd { vendor,name,cost,price,unit,currency } => { vendor::allow_usage(c,vendor,None)?; let currency=currency.unwrap_or(Settings::load(c)?.currency); required(&name)?; required(&unit)?; c.execute("INSERT INTO prices(vendor,name,unit,cost,price,currency) VALUES(?,?,?,?,?,?)",params![vendor,name,unit,parse_money(&cost)?,parse_money(&price)?,currency])?; println!("价格 #{}",c.last_insert_rowid()); }
        Command::PriceList => list(c,"SELECT p.id,v.name AS vendor,p.name,p.unit,p.currency,printf('%d.%02d',p.cost/100,p.cost%100) AS cost,printf('%d.%02d',p.price/100,p.price%100) AS price FROM prices p JOIN vendors v ON v.id=p.vendor ORDER BY p.id")?,
        Command::PriceUpdate { id,cost,price,currency } => changed(c.execute("UPDATE prices SET cost=?,price=?,currency=COALESCE(?,currency) WHERE id=?",params![parse_money(&cost)?,parse_money(&price)?,currency,id])?)?,
        Command::CurrencyList => println!("CAD USD CNY EUR GBP HKD AUD SGD"),
        Command::LegacyCurrency { currency } => {
            let tx=c.transaction()?;
            tx.execute("UPDATE prices SET currency=? WHERE currency='UNSET'",[&currency])?;
            tx.execute("UPDATE tasks SET currency=? WHERE currency='UNSET'",[&currency])?;
            tx.execute("UPDATE items SET source_currency=?,source_cost=cost,source_price=price WHERE source_currency='UNSET'",[&currency])?;
            tx.execute("UPDATE ledger_expenses SET currency=? WHERE currency='UNSET'",[&currency])?;
            tx.execute("UPDATE expense_documents SET currency=? WHERE currency='UNSET'",[&currency])?;
            tx.execute("UPDATE ledger_expenses SET task_currency=? WHERE task_currency='UNSET'",[&currency])?;
            tx.execute("UPDATE invoices SET snapshot=json_set(snapshot,'$.currency',?) WHERE json_extract(snapshot,'$.currency') IS NULL OR json_extract(snapshot,'$.currency')='UNSET'",[&currency])?;
            tx.commit()?; println!("旧数据已标记为 {currency}，金额未换算");
        }
        Command::FxSet { source,target,rate:input } => {
            let fx=rate(&input)?; ensure!(source!=target || fx==FX_SCALE,"同币种汇率必须为 1");
            c.execute("INSERT INTO fx_rates(source,target,rate,provider,rate_date) VALUES(?,?,?,'Manual',NULL) ON CONFLICT(source,target) DO UPDATE SET rate=excluded.rate,provider='Manual',rate_date=NULL,updated=CURRENT_TIMESTAMP",params![source,target,fx])?;
            println!("1 {source} = {input} {target}");
        }
        Command::FxSync => {let date=fx::store(c,&fx::fetch()?)?;println!("已同步加拿大央行汇率：{date}");}
        Command::FxList => list(c,"SELECT source,target,printf('%d.%08d',rate/100000000,rate%100000000) AS rate,provider,rate_date,updated FROM fx_rates ORDER BY source,target")?,
        Command::CustomerAdd { name,contact,currency:selected } => { required(&name)?; let selected=currency(&selected).map_err(anyhow::Error::msg)?; c.execute("INSERT INTO customers(name,contact,currency) VALUES(?,?,?)",params![name,contact,selected])?; println!("Customer #{}",c.last_insert_rowid()); }
        Command::CustomerList => list(c,"SELECT id,name,contact,currency FROM customers ORDER BY id")?,
        Command::CustomerUpdate { id,name,contact,currency:selected } => { required(&name)?; let selected=selected.map(|v|currency(&v).map_err(anyhow::Error::msg)).transpose()?; changed(c.execute("UPDATE customers SET name=?,contact=?,currency=COALESCE(?,currency) WHERE id=?",params![name,contact,selected,id])?)?; }
        Command::TaskAdd { title,date,customer,items,currency } => {
            let currency=currency.unwrap_or(Settings::load(c)?.currency);
            let tx=c.transaction()?; let date=date.unwrap_or_else(||today().to_string());
            let id=add_task_on(&tx,&title,customer,&currency,&date)?;
            for price_id in items { add_item(&tx,id,price_id,1,None,None)?; }
            tx.commit()?; println!("Task #{id}");
        }
        Command::TaskList => list(c,&format!("SELECT t.id,t.number,t.title,t.task_date,t.currency,t.customer_id,c.name AS customer,t.progress,{TASK_STATUS_SQL} AS status,i.id AS invoice FROM tasks t JOIN customers c ON c.id=t.customer_id LEFT JOIN invoice_tasks it ON it.task=t.id LEFT JOIN invoices i ON i.id=it.invoice ORDER BY {TASK_ORDER_SQL}"))?,
        Command::TaskShow { id } => println!("{}",serde_json::to_string_pretty(&snapshot(c,id)?)?),
        Command::TaskItem { task,price_id,quantity,price,cost } => {
            let price=price.as_deref().map(parse_money).transpose()?;
            let cost=cost.as_deref().map(parse_money).transpose()?;
            let tx=c.transaction()?; let id=add_item(&tx,task,price_id,quantity,price,cost)?;
            tx.commit()?; println!("明细 #{id} 已添加");
        }
        Command::TaskItems { task,price_ids } => {
            let tx=c.transaction()?;
            for id in price_ids { add_item(&tx,task,id,1,None,None)?; }
            tx.commit()?; println!("业务已添加");
        }
        Command::TaskItemUpdate { task,item,price,cost,quantity } => {
            let price=price.as_deref().map(parse_money).transpose()?;
            let cost=cost.as_deref().map(parse_money).transpose()?;
            let tx=c.transaction()?; update_item(&tx,task,item,price,cost,quantity)?;
            tx.commit()?; println!("任务明细已更新");
        }
        Command::TaskExpense { task,description,amount,vendor,tax_code,tax_included,recovery } => {
            let currency=c.query_row("SELECT currency FROM tasks WHERE id=?",[task],|r|r.get::<_,String>(0))?;
            let account=accounting::default_account(c)?;
            accounting::save_expense(c,&accounting::Expense{ tax:tax::Input{code:tax_code,inclusive:tax_included,recovery},id:0,date:today().to_string(),description,account,vendor:Some(vendor),currency,amount,task:Some(task)})?;println!("费用已添加");
        }
        Command::TaskProgress { task,progress } => { ensure!((0..=100).contains(&progress),"进度必须为 0 到 100");let tx=c.transaction()?;editable(&tx,task)?;tx.execute("UPDATE tasks SET progress=? WHERE id=?",params![progress,task])?;tx.commit()?;println!("已保存"); }
        Command::TaskComplete {task} => {set_task_status(c,task,false)?;println!("Task #{task} completed");}
        Command::TaskCancel {task} => {set_task_status(c,task,true)?;println!("Task #{task} cancel");}
        Command::InvoiceCreate { task } => {let (id,created)=create_invoice(c,task)?;println!("Invoice #{id}{}",if created {""} else {"（已存在）"});}
        Command::InvoiceMerge{tasks}=>{let id=billing::merge(c,&tasks)?;println!("Invoice #{id}");}
        Command::PaymentAdd{invoice,date,amount,method,reference}=>{let id=billing::add_payment(c,invoice,&date,&amount,&method,&reference)?;println!("Payment #{id}");}
        Command::PaymentVoid{id,reason}=>billing::void_payment(c,id,&reason)?,
        Command::TaxRecord{agency,date,kind,amount,reference,reason}=>tax::cash(c,agency,&date,&kind,&amount,&reference,&reason)?,
        Command::TaxSummary{from,to}=>{println!("{}",serde_json::to_string_pretty(&tax::admin::summary(c,&from,&to)?)?);},
        Command::TaxAssign{entity,record,code}=>tax::admin::assign(c,&entity,record,code)?,
        Command::TaxAgencySet{id,registration,registered}=>{let (name,payable,recoverable):(String,i64,Option<i64>)=c.query_row("SELECT name,payable_account,recoverable_account FROM tax_agencies WHERE id=?",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;tax::admin::agency(c,id,&name,&registration,registered,payable,recoverable)?;},
        Command::AuditList=>list(c,"SELECT id,occurred_at,entity,record_id,action,actor,before_json,after_json FROM audit_log ORDER BY id DESC LIMIT 200")?,
        Command::NumberingList=>list(c,"SELECT kind,prefix,width,next_value FROM number_sequences ORDER BY kind")?,
        Command::NumberingSet{kind,prefix,width,next}=>numbering::configure(c,&[numbering::Rule{kind,prefix,width,next}])?,
        Command::InvoiceList => {
            let mut stmt=c.prepare("SELECT id,number FROM invoices ORDER BY id")?;
            for row in stmt.query_map([], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?)))? {
                let (id,number)=row?;
                let balance=billing::balance(c,id)?;
                let tasks:String=c.query_row("SELECT group_concat(number, ', ') FROM tasks WHERE id IN (SELECT task FROM invoice_tasks WHERE invoice=?)",[id],|r|r.get(0))?;
                println!("{id}\t{number}\t{tasks}\t{}\t{}\t{}\t{}\t{}",balance.currency,format_money(balance.total),format_money(balance.received),format_money(balance.remaining),balance.status());
            }
        }
        Command::InvoicePay { id } => {let balance=billing::balance(c,id)?;if balance.remaining>0 {billing::add_payment(c,id,&today().to_string(),&format_money(balance.remaining),"Other","CLI 标记全额支付")?;}println!("已支付");}
        Command::InvoiceExport { id,output } => {invoice::export(c,id,&output)?;println!("已导出 {}",output.display());}
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("错误：{e:#}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn customer_vendor_currency_migrates_defaults_and_preserves_invoice_history() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        c.execute_batch("CREATE TABLE customers(id INTEGER PRIMARY KEY,name TEXT NOT NULL,contact TEXT NOT NULL); INSERT INTO customers VALUES(1,'Legacy customer','Contact'); CREATE TABLE vendors(id INTEGER PRIMARY KEY,name TEXT NOT NULL,contact TEXT NOT NULL); INSERT INTO vendors VALUES(1,'Legacy vendor','Contact'); PRAGMA user_version=8;")?;
        init(&c)?;
        for table in ["customers", "vendors"] {
            assert_eq!(
                c.query_row(
                    &format!("SELECT currency FROM {table} WHERE id=1"),
                    [],
                    |r| r.get::<_, String>(0)
                )?,
                "CAD"
            );
            assert_eq!(
                c.query_row(
                    &format!("SELECT contact FROM {table} WHERE id=1"),
                    [],
                    |r| r.get::<_, String>(0)
                )?,
                "Contact"
            );
            assert!(c
                .execute(&format!("UPDATE {table} SET currency='' WHERE id=1"), [])
                .is_err());
            assert!(c
                .execute(
                    &format!("UPDATE {table} SET currency='INVALID' WHERE id=1"),
                    []
                )
                .is_err());
        }
        c.execute("INSERT INTO prices(vendor,name,unit,cost,price,currency) VALUES(1,'Service','item',0,10000,'CAD')",[])?;
        let task = add_task(&c, "Historical task", 1, "CAD")?;
        add_item(&c, task, 1, 1, None, None)?;
        set_task_status(&mut c, task, false)?;
        let invoice = billing::merge(&mut c, &[task])?;
        let historical: String =
            c.query_row("SELECT snapshot FROM invoices WHERE id=?", [invoice], |r| {
                r.get(0)
            })?;
        execute(
            &mut c,
            Command::CustomerUpdate {
                id: 1,
                name: "Legacy customer".into(),
                contact: "Contact".into(),
                currency: Some("usd".into()),
            },
        )?;
        execute(
            &mut c,
            Command::VendorUpdate {
                id: 1,
                name: "Legacy vendor".into(),
                contact: "Contact".into(),
                currency: Some("eur".into()),
            },
        )?;
        execute(
            &mut c,
            Command::CustomerUpdate {
                id: 1,
                name: "Legacy customer".into(),
                contact: "Contact".into(),
                currency: None,
            },
        )?;
        execute(
            &mut c,
            Command::VendorUpdate {
                id: 1,
                name: "Legacy vendor".into(),
                contact: "Contact".into(),
                currency: None,
            },
        )?;
        assert_eq!(
            c.query_row("SELECT currency FROM customers WHERE id=1", [], |r| r
                .get::<_, String>(0))?,
            "USD"
        );
        assert_eq!(
            c.query_row("SELECT currency FROM vendors WHERE id=1", [], |r| r
                .get::<_, String>(0))?,
            "EUR"
        );
        assert_eq!(
            c.query_row("SELECT snapshot FROM invoices WHERE id=?", [invoice], |r| r
                .get::<_, String>(0))?,
            historical
        );
        assert_eq!(snapshot(&c, task)?["currency"], "CAD");
        assert!(execute(
            &mut c,
            Command::CustomerAdd {
                name: "Invalid".into(),
                contact: "".into(),
                currency: "".into()
            }
        )
        .is_err());
        assert!(execute(
            &mut c,
            Command::VendorAdd {
                name: "Invalid".into(),
                contact: "".into(),
                currency: "JPY".into()
            }
        )
        .is_err());
        let cli = Cli::try_parse_from(["axibiz", "customer-add", "Default CAD"])?;
        execute(&mut c, cli.command.unwrap())?;
        let cli = Cli::try_parse_from(["axibiz", "vendor-add", "Default CAD"])?;
        execute(&mut c, cli.command.unwrap())?;
        for table in ["customers", "vendors"] {
            assert_eq!(
                c.query_row(
                    &format!("SELECT currency FROM {table} WHERE name='Default CAD'"),
                    [],
                    |r| r.get::<_, String>(0)
                )?,
                "CAD"
            );
        }
        init(&c)?;
        assert_eq!(
            c.query_row("SELECT currency FROM customers WHERE id=1", [], |r| r
                .get::<_, String>(0))?,
            "USD"
        );
        Ok(())
    }
    #[test]
    fn amounts_are_exact() {
        assert_eq!(parse_money("12.30").unwrap(), 1230);
        assert_eq!(parse_money("0.1").unwrap(), 10);
        for s in ["-1", "1.001", "1e3", "", "1."] {
            assert!(parse_money(s).is_err());
        }
    }
    #[test]
    fn snapshots_and_invoice_lock() -> Result<()> {
        let c = Connection::open_in_memory()?;
        init(&c)?;
        c.execute(
            "INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','')",
            [],
        )?;
        c.execute(
            "INSERT INTO prices(id,vendor,name,unit,cost,price) VALUES(1,1,'服务','项',100,300)",
            [],
        )?;
        c.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Customer','')",
            [],
        )?;
        add_task(&c, "Task", 1, "CAD")?;
        c.execute("INSERT INTO items(id,task,description,unit,quantity,cost,price) VALUES(1,1,'服务','项',2,100,300)",[])?;
        c.execute("INSERT INTO expenses VALUES(1,1,'运输',50)", [])?;
        let data = snapshot(&c, 1)?;
        assert_eq!(data["total"], 600);
        assert_eq!(data["cost"], 250);
        assert_eq!(data["profit"], 350);
        c.execute("UPDATE prices SET price=999 WHERE id=1", [])?;
        assert_eq!(snapshot(&c, 1)?["total"], 600);
        c.execute(
            "INSERT INTO invoices(task,snapshot) VALUES(1,?)",
            [data.to_string()],
        )?;
        assert!(editable(&c, 1).is_err());
        assert!(c
            .execute("INSERT INTO invoices(task,snapshot) VALUES(1,'{}')", [])
            .is_err());
        Ok(())
    }
    #[test]
    fn customer_selection_and_snapshot() -> Result<()> {
        let c = Connection::open_in_memory()?;
        init(&c)?;
        c.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Customer A','a@example.com')",
            [],
        )?;
        assert!(add_task(&c, "Missing customer", 99, "CAD").is_err());
        assert!(add_task(&c, "  ", 1, "CAD").is_err());
        let id = add_task(&c, "Test", 1, "CAD")?;
        c.execute_batch("INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor',''); INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','item',0,100,'CAD');")?;
        add_item(&c, id, 1, 1, None, None)?;
        let saved = snapshot(&c, id)?;
        assert_eq!(saved["customer_id"], 1);
        assert_eq!(saved["customer"], "Customer A");
        c.execute(
            "INSERT INTO invoices(task,snapshot) VALUES(?,?)",
            params![id, saved.to_string()],
        )?;
        c.execute(
            "UPDATE customers SET name='Renamed customer' WHERE id=1",
            [],
        )?;
        assert_eq!(snapshot(&c, id)?["customer"], "Renamed customer");
        let raw: String = c.query_row("SELECT snapshot FROM invoices WHERE task=?", [id], |r| {
            r.get(0)
        })?;
        assert_eq!(
            serde_json::from_str::<Value>(&raw)?["customer"],
            "Customer A"
        );
        assert!(c.execute("DELETE FROM customers WHERE id=1", []).is_err());
        Ok(())
    }
    #[test]
    fn legacy_customers_migrate_once() -> Result<()> {
        let c = Connection::open_in_memory()?;
        c.execute_batch("CREATE TABLE tasks(id INTEGER PRIMARY KEY,title TEXT NOT NULL,customer TEXT NOT NULL,progress INTEGER NOT NULL DEFAULT 0);
            INSERT INTO tasks VALUES(1,'Test','Customer A',50),(2,'Test','Customer A',0),(3,'Test','Customer B',100);")?;
        init(&c)?;
        init(&c)?;
        let count: i64 = c.query_row("SELECT COUNT(*) FROM customers", [], |r| r.get(0))?;
        assert_eq!(count, 2);
        assert_eq!(
            snapshot(&c, 1)?["customer_id"],
            snapshot(&c, 2)?["customer_id"]
        );
        assert_eq!(snapshot(&c, 3)?["customer"], "Customer B");
        assert!(c
            .execute(
                "INSERT INTO tasks(title,customer,customer_id) VALUES('bad','bad',999)",
                []
            )
            .is_err());
        assert!(c
            .execute("INSERT INTO tasks(title,customer) VALUES('bad','bad')", [])
            .is_err());
        Ok(())
    }

    #[test]
    fn task_prices_are_independent_and_locked_after_invoice() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        init(&c)?;
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer',''); INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');
            INSERT INTO prices(id,vendor,name,unit,cost,price) VALUES(1,1,'Service','hour',100,300);")?;
        c.execute("UPDATE prices SET currency='CAD'", [])?;
        let task = add_task(&c, "Task", 1, "CAD")?;
        let item = add_item(&c, task, 1, 2, Some(250), None)?;
        assert_eq!(snapshot(&c, task)?["total"], 500);
        update_item(&c, task, item, Some(400), Some(150), Some(3))?;
        let data = snapshot(&c, task)?;
        assert_eq!(data["total"], 1200);
        assert_eq!(data["cost"], 450);
        assert_eq!(data["profit"], 750);
        let base: i64 = c.query_row("SELECT price FROM prices WHERE id=1", [], |r| r.get(0))?;
        assert_eq!(base, 300);
        assert!(update_item(&c, task, item, None, None, Some(0)).is_err());
        let other = add_task(&c, "Other", 1, "CAD")?;
        assert!(update_item(&c, other, item, Some(1), None, None).is_err());
        {
            let tx = c.transaction()?;
            add_item(&tx, task, 1, 1, None, None)?;
            assert!(add_item(&tx, task, 999, 1, None, None).is_err());
        }
        assert_eq!(snapshot(&c, task)?["items"].as_array().unwrap().len(), 1);
        c.execute(
            "INSERT INTO invoices(task,snapshot) VALUES(?,?)",
            params![task, data.to_string()],
        )?;
        assert!(update_item(&c, task, item, Some(1), None, None).is_err());
        assert!(add_item(&c, task, 1, 1, None, None).is_err());
        Ok(())
    }

    #[test]
    fn exchange_rates_are_exact_and_validated() -> Result<()> {
        assert_eq!(rate("1.35000001")?, 135000001);
        for invalid in ["0", "-1", "NaN", "1e3", "1.123456789", "1.", ".5"] {
            assert!(rate(invalid).is_err());
        }
        assert_eq!(convert(101, rate("1.5")?)?, 152);
        assert_eq!(convert(10000, rate("1.35")?)?, 13500);
        assert!(convert(i64::MAX, rate("2")?).is_err());
        assert_eq!(format_money(i64::MAX), "92233720368547758.07");
        assert_eq!(currency("usd").unwrap(), "USD");
        assert!(currency("JPY").is_err());
        Ok(())
    }
    #[test]
    fn cross_currency_snapshot_survives_rate_changes() -> Result<()> {
        let c = Connection::open_in_memory()?;
        init(&c)?;
        init(&c)?;
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer',''); INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');
            INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','hour',8000,10000,'USD');")?;
        let task = add_task(&c, "Canadian task", 1, "CAD")?;
        assert!(add_item(&c, task, 999, 1, None, None).is_err());
        c.execute(
            "INSERT INTO fx_rates(source,target,rate) VALUES('USD','CAD',?)",
            [rate("1.35")?],
        )?;
        let item = add_item(&c, task, 1, 2, None, None)?;
        let data = snapshot(&c, task)?;
        assert_eq!(data["currency"], "CAD");
        assert_eq!(data["total"], 27000);
        assert_eq!(data["cost"], 21600);
        assert_eq!(data["items"][0]["source_currency"], "USD");
        assert_eq!(data["items"][0]["source_price"], 10000);
        assert_eq!(data["items"][0]["fx_rate"], "1.35000000");
        c.execute("UPDATE fx_rates SET rate=?", [rate("1.50")?])?;
        assert_eq!(snapshot(&c, task)?, data);
        update_item(&c, task, item, Some(14000), None, None)?;
        assert_eq!(snapshot(&c, task)?["total"], 28000);
        assert_eq!(snapshot(&c, task)?["items"][0]["fx_rate"], "1.35000000");
        assert!(fx::cached(&c, "CAD", "USD")?.is_none());
        assert!(exchange(&c, "UNSET", "CAD").is_err());
        Ok(())
    }
    #[test]
    fn task_age_uses_displayed_task_date() -> Result<()> {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        assert_eq!(task_age_days(Some("2026-10-23"), date), Some(3));
        assert_eq!(task_age_days(Some("2026-10-25"), date), Some(1));
        assert_eq!(task_age_days(Some("2026-10-26"), date), Some(0));
        assert_eq!(task_age_days(Some("2026-10-28"), date), Some(0));
        assert_eq!(task_age_days(None, date), None);
        assert_eq!(task_age_days(Some("invalid"), date), None);
        Ok(())
    }
    #[test]
    fn task_creation_migration_uses_only_insert_audit_and_keeps_business_date_separate(
    ) -> Result<()> {
        let c = Connection::open_in_memory()?;
        c.execute_batch("CREATE TABLE tasks(id INTEGER PRIMARY KEY,title TEXT NOT NULL,customer TEXT NOT NULL,progress INTEGER NOT NULL DEFAULT 0,task_date TEXT);
            INSERT INTO tasks VALUES(1,'Known','Customer',0,'2026-01-01'),(2,'Unknown','Customer',0,'2026-01-02');
            CREATE TABLE audit_log(id INTEGER PRIMARY KEY,occurred_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),entity TEXT NOT NULL,record_id TEXT NOT NULL,action TEXT NOT NULL,before_json TEXT,after_json TEXT,actor TEXT NOT NULL DEFAULT 'local');
            INSERT INTO audit_log(occurred_at,entity,record_id,action) VALUES('2026-10-24T02:00:00Z','tasks','1','INSERT'),('2026-10-25T02:00:00Z','tasks','2','UPDATE'); PRAGMA user_version=6;")?;
        init(&c)?;
        let created: Option<String> =
            c.query_row("SELECT created_at FROM tasks WHERE id=1", [], |r| r.get(0))?;
        assert_eq!(created.as_deref(), Some("2026-10-24T02:00:00Z"));
        assert_eq!(
            c.query_row("SELECT created_at FROM tasks WHERE id=2", [], |r| r
                .get::<_, Option<String>>(0))?,
            None
        );
        let id = add_task_on(&c, "Future business date", 1, "CAD", "2030-01-01")?;
        let stamp: String = c.query_row("SELECT created_at FROM tasks WHERE id=?", [id], |r| {
            r.get(0)
        })?;
        assert!(chrono::DateTime::parse_from_rfc3339(&stamp).is_ok());
        c.execute("UPDATE tasks SET task_date='2026-01-01' WHERE id=?", [id])?;
        assert_eq!(
            c.query_row("SELECT created_at FROM tasks WHERE id=?", [id], |r| r
                .get::<_, String>(0))?,
            stamp
        );
        assert!(c
            .execute("UPDATE tasks SET created_at='2000-01-01' WHERE id=?", [id])
            .is_err());
        init(&c)?;
        assert_eq!(
            c.query_row("SELECT created_at FROM tasks WHERE id=1", [], |r| r
                .get::<_, Option<String>>(0))?,
            created
        );
        Ok(())
    }
    #[test]
    fn task_dates_validate_persist_and_survive_migration() -> Result<()> {
        for invalid in ["2026-02-29", "2026-13-01", "2026-1-01", "", "2026-04-31"] {
            assert!(task_date(invalid).is_err());
        }
        assert!(task_date("2028-02-29").is_ok());
        let c = Connection::open_in_memory()?;
        init(&c)?;
        c.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Customer','')",
            [],
        )?;
        let id = add_task_on(&c, "Dated task", 1, "CAD", "2026-11-05")?;
        init(&c)?;
        assert_eq!(snapshot(&c, id)?["task_date"], "2026-11-05");
        assert!(add_task_on(&c, "Invalid", 1, "CAD", "2026-02-29").is_err());
        let count: i64 = c.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))?;
        assert_eq!(count, 1);
        Ok(())
    }
    #[test]
    fn task_status_and_order_prioritize_pending_completed_invoiced() -> Result<()> {
        let c = Connection::open_in_memory()?;
        init(&c)?;
        c.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Customer','')",
            [],
        )?;
        let invoiced = add_task(&c, "Invoiced", 1, "CAD")?;
        let completed = add_task(&c, "Completed", 1, "CAD")?;
        let pending = add_task(&c, "Pending", 1, "CAD")?;
        let partial = add_task(&c, "Partial", 1, "CAD")?;
        c.execute("UPDATE tasks SET progress=100 WHERE id=?", [completed])?;
        c.execute("UPDATE tasks SET progress=50 WHERE id=?", [partial])?;
        c.execute(
            r#"INSERT INTO invoices(task,snapshot,paid) VALUES(?,'{"title":"Invoiced","customer":"Customer","currency":"CAD","total":0,"items":[{"description":"Service","unit":"item","quantity":1,"unit_price":0,"subtotal":0}]}',1)"#,
            [invoiced],
        )?;
        let mut stmt=c.prepare(&format!("SELECT t.id,{TASK_STATUS_SQL} FROM tasks t LEFT JOIN invoice_tasks it ON it.task=t.id LEFT JOIN invoices i ON i.id=it.invoice ORDER BY {TASK_ORDER_SQL}"))?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        assert_eq!(
            rows,
            vec![
                (partial, "pending".into()),
                (pending, "pending".into()),
                (completed, "completed".into()),
                (invoiced, "invoiced".into())
            ]
        );
        c.execute("UPDATE tasks SET progress=100 WHERE id=?", [invoiced])?;
        let status:String=c.query_row(&format!("SELECT {TASK_STATUS_SQL} FROM tasks t LEFT JOIN invoice_tasks it ON it.task=t.id LEFT JOIN invoices i ON i.id=it.invoice WHERE t.id=?"),[invoiced],|r|r.get(0))?;
        assert_eq!(status, "invoiced");
        Ok(())
    }
    #[test]
    fn invoicing_requires_completed_task_even_via_cli() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        init(&c)?;
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer','');INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','item',0,5000,'CAD');")?;
        let task = add_task(&c, "Task", 1, "CAD")?;
        add_item(&c, task, 1, 1, None, None)?;
        for progress in [0, 50, 99] {
            c.execute(
                "UPDATE tasks SET progress=? WHERE id=?",
                params![progress, task],
            )?;
            let error = execute(&mut c, Command::InvoiceCreate { task }).unwrap_err();
            assert!(error.to_string().contains("completed"));
        }
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM invoices", [], |r| r.get::<_, i64>(0))?,
            0
        );
        c.execute("UPDATE tasks SET progress=100 WHERE id=?", [task])?;
        let (id, created) = create_invoice(&mut c, task)?;
        assert!(created);
        assert_eq!(create_invoice(&mut c, task)?, (id, false));
        Ok(())
    }
    #[test]
    fn cancelled_tasks_cannot_be_completed_edited_or_invoiced() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        init(&c)?;
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer','');INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','item',0,5000,'CAD');")?;
        let id = add_task(&c, "Cancel task", 1, "CAD")?;
        add_item(&c, id, 1, 1, None, None)?;
        execute(&mut c, Command::TaskComplete { task: id })?;
        assert_eq!(snapshot(&c, id)?["status"], "completed");
        execute(&mut c, Command::TaskCancel { task: id })?;
        init(&c)?;
        assert_eq!(snapshot(&c, id)?["status"], "cancel");
        assert!(create_invoice(&mut c, id).is_err());
        assert!(execute(&mut c, Command::TaskComplete { task: id }).is_err());
        assert!(execute(
            &mut c,
            Command::TaskProgress {
                task: id,
                progress: 100
            }
        )
        .is_err());
        assert!(add_item(&c, id, 1, 1, None, None).is_err());
        let other = add_task(&c, "Invoice task", 1, "CAD")?;
        add_item(&c, other, 1, 1, None, None)?;
        set_task_status(&mut c, other, false)?;
        create_invoice(&mut c, other)?;
        assert!(set_task_status(&mut c, other, true).is_err());
        let mut stmt=c.prepare(&format!("SELECT {TASK_STATUS_SQL} FROM tasks t LEFT JOIN invoice_tasks it ON it.task=t.id LEFT JOIN invoices i ON i.id=it.invoice ORDER BY {TASK_ORDER_SQL}"))?;
        let statuses = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        assert_eq!(statuses, vec!["invoiced", "cancel"]);
        Ok(())
    }
    #[test]
    fn legacy_currency_updates_linked_expenses_and_rejects_future_schema() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        init(&c)?;
        c.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Customer','')",
            [],
        )?;
        let task = add_task(&c, "Legacy", 1, "UNSET")?;
        c.execute(
            "INSERT INTO expenses(task,description,amount) VALUES(?,'Old expense',100)",
            [task],
        )?;
        init(&c)?;
        execute(
            &mut c,
            Command::LegacyCurrency {
                currency: "CAD".into(),
            },
        )?;
        let values: (String, String) = c.query_row(
            "SELECT currency,task_currency FROM ledger_expenses",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        assert_eq!(values, ("CAD".into(), "CAD".into()));
        c.execute_batch("PRAGMA user_version=99")?;
        assert!(init(&c).is_err());
        assert_eq!(
            c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
            99
        );
        Ok(())
    }
}
