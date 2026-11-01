use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub const PROVIDER: &str = "Bank of Canada";
pub fn dated(c: &Connection, source: &str, target: &str, date: &str) -> Result<i64> {
    crate::currency(source).map_err(anyhow::Error::msg)?;
    crate::currency(target).map_err(anyhow::Error::msg)?;
    crate::task_date(date).map_err(anyhow::Error::msg)?;
    if source == target {
        return Ok(crate::FX_SCALE);
    }
    if let Some(rate)=c.query_row("SELECT rate FROM fx_rates WHERE source=? AND target=? AND provider='Manual' AND rate_date IS NULL",params![source,target],|r|r.get::<_,i64>(0)).optional()?{return Ok(rate);}
    let since = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")? - chrono::Duration::days(7);
    let history=c.query_row("SELECT rate FROM fx_history WHERE source=? AND target=? AND rate_date BETWEEN ? AND ? ORDER BY rate_date DESC,fetched_at DESC,provider LIMIT 1",params![source,target,since.to_string(),date],|r|r.get::<_,i64>(0)).optional()?;
    let rate = history.or(c
        .query_row(
            "SELECT rate FROM fx_rates WHERE source=? AND target=? AND rate_date BETWEEN ? AND ?",
            params![source, target, since.to_string(), date],
            |r| r.get::<_, i64>(0),
        )
        .optional()?);
    rate.with_context(|| {
        format!("缺少 {date} 的本地汇率：{source} → {target}，请更新或补充手工汇率")
    })
}
const CODES: [&str; 8] = ["CAD", "USD", "CNY", "EUR", "GBP", "HKD", "AUD", "SGD"];
#[derive(Debug)]
pub struct DailyRates {
    pub date: String,
    pub rates: Vec<(String, String, i64)>,
}
pub fn init(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS fx_history(source TEXT NOT NULL,target TEXT NOT NULL,rate_date TEXT NOT NULL,rate INTEGER NOT NULL CHECK(rate>0),provider TEXT NOT NULL,fetched_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,PRIMARY KEY(source,target,rate_date,provider));
        CREATE TABLE IF NOT EXISTS fx_sync(id INTEGER PRIMARY KEY CHECK(id=1),last_success TEXT,rate_date TEXT);
        INSERT OR IGNORE INTO fx_sync(id) VALUES(1);")?;
    for (table, column, definition) in [
        ("fx_rates", "provider", "TEXT NOT NULL DEFAULT 'Manual'"),
        ("fx_rates", "rate_date", "TEXT"),
        ("items", "fx_provider", "TEXT"),
        ("items", "fx_date", "TEXT"),
    ] {
        let exists: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?) WHERE name=?)",
            params![table, column],
            |r| r.get(0),
        )?;
        if !exists {
            c.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN {column} {definition};"
            ))?;
        }
    }
    Ok(())
}
pub fn parse(value: &Value) -> Result<Vec<DailyRates>> {
    let observations = value["observations"]
        .as_array()
        .context("接口缺少 observations 数据")?;
    let mut days = Vec::new();
    for row in observations {
        let date = row["d"].as_str().context("接口缺少汇率日期")?;
        crate::task_date(date).map_err(anyhow::Error::msg)?;
        ensure!(
            date <= crate::today().to_string().as_str(),
            "接口返回未来日期"
        );
        let mut bases = vec![crate::FX_SCALE];
        let mut complete = true;
        for code in &CODES[1..] {
            match row[format!("FX{code}CAD")]["v"].as_str() {
                Some(v) => bases.push(crate::rate(v).with_context(|| format!("{code} 汇率无效"))?),
                None => {
                    complete = false;
                    break;
                }
            }
        }
        // 不将不同日期的汇率混在一起；未完整发布的日期留待下次获取。
        if !complete {
            continue;
        }
        let mut rates = Vec::new();
        for (i, source) in CODES.iter().enumerate() {
            for (j, target) in CODES.iter().enumerate() {
                if i == j {
                    continue;
                }
                let n = i128::from(bases[i]) * i128::from(crate::FX_SCALE);
                let d = i128::from(bases[j]);
                let rate = i64::try_from((n + d / 2) / d).context("交叉汇率过大")?;
                ensure!(rate > 0, "交叉汇率过小");
                rates.push((source.to_string(), target.to_string(), rate));
            }
        }
        days.push(DailyRates {
            date: date.into(),
            rates,
        });
    }
    ensure!(
        !days.is_empty(),
        "没有获取到完整的八币种汇率，保留原本地数据"
    );
    days.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(days)
}
pub fn fetch() -> Result<Vec<DailyRates>> {
    fetch_query(&[("recent", "10".to_owned())])
}

pub fn history_window(date: &str) -> Result<(String, String)> {
    let end = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")?;
    ensure!(
        end <= crate::today(),
        "Exchange rates are not available for a future date."
    );
    let start = end
        .checked_sub_signed(chrono::Duration::days(7))
        .context("Invalid historical rate date.")?;
    Ok((start.to_string(), end.to_string()))
}

pub fn fetch_dated(date: &str) -> Result<Vec<DailyRates>> {
    let (start, end) = history_window(date)?;
    let days = fetch_query(&[("start_date", start.clone()), ("end_date", end.clone())])?;
    ensure!(
        days.iter().all(|day| day.date >= start && day.date <= end),
        "The provider returned rates outside the requested date range."
    );
    Ok(days)
}

fn fetch_query(parameters: &[(&str, String)]) -> Result<Vec<DailyRates>> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(25))
        .user_agent(concat!("Axibiz/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let data: Value = client
        .get("https://www.bankofcanada.ca/valet/observations/group/FX_RATES_DAILY/json")
        .query(parameters)
        .send()
        .context("无法连接加拿大央行")?
        .error_for_status()
        .context("汇率接口返回错误")?
        .json()
        .context("无法解析汇率响应")?;
    parse(&data)
}
pub fn cached(c: &Connection, source: &str, target: &str) -> Result<Option<i64>> {
    Ok(c.query_row(
        "SELECT rate FROM fx_rates WHERE source=? AND target=?",
        params![source, target],
        |r| r.get(0),
    )
    .optional()?)
}
pub fn resolve(
    c: &Connection,
    source: &str,
    target: &str,
    fetch: impl FnOnce() -> Result<Vec<DailyRates>>,
) -> Result<i64> {
    if let Some(value) = cached(c, source, target)? {
        return Ok(value);
    }
    crate::currency(source).map_err(anyhow::Error::msg)?;
    crate::currency(target).map_err(anyhow::Error::msg)?;
    let days =
        fetch().with_context(|| format!("本地缺少 {source} → {target} 汇率，在线查询失败"))?;
    store(c, &days)?;
    cached(c, source, target)?.with_context(|| format!("查询结果没有 {source} → {target} 汇率"))
}
pub fn store(c: &Connection, days: &[DailyRates]) -> Result<String> {
    // Savepoint works both alone and inside an existing Task transaction.
    c.execute_batch("SAVEPOINT fx_cache_store")?;
    match store_inner(c, days) {
        Ok(date) => {
            c.execute_batch("RELEASE fx_cache_store")?;
            Ok(date)
        }
        Err(error) => {
            c.execute_batch("ROLLBACK TO fx_cache_store; RELEASE fx_cache_store")?;
            Err(error)
        }
    }
}
pub fn store_dated(c: &Connection, date: &str, days: &[DailyRates]) -> Result<String> {
    let (start, end) = history_window(date)?;
    ensure!(
        !days.is_empty() && days.iter().all(|day| day.date >= start && day.date <= end),
        "Historical rates do not match the requested date range."
    );
    c.execute_batch("SAVEPOINT fx_history_store")?;
    match write_history(c, days) {
        Ok(()) => {
            c.execute_batch("RELEASE fx_history_store")?;
            Ok(days.iter().map(|day| day.date.clone()).max().unwrap())
        }
        Err(error) => {
            c.execute_batch("ROLLBACK TO fx_history_store; RELEASE fx_history_store")?;
            Err(error)
        }
    }
}
fn write_history(c: &Connection, days: &[DailyRates]) -> Result<()> {
    ensure!(!days.is_empty(), "汇率结果为空");
    for day in days {
        crate::task_date(&day.date).map_err(anyhow::Error::msg)?;
        ensure!(
            day.date <= crate::today().to_string(),
            "Cannot store a future exchange rate."
        );
        ensure!(day.rates.len() == 56, "汇率数据不完整");
        for (source, target, rate) in &day.rates {
            c.execute("INSERT INTO fx_history(source,target,rate_date,rate,provider) VALUES(?,?,?,?,?) ON CONFLICT(source,target,rate_date,provider) DO UPDATE SET rate=excluded.rate,fetched_at=CURRENT_TIMESTAMP",params![source,target,day.date,rate,PROVIDER])?;
        }
    }
    Ok(())
}
fn store_inner(c: &Connection, days: &[DailyRates]) -> Result<String> {
    write_history(c, days)?;
    let latest = days.iter().max_by_key(|d| &d.date).unwrap();
    for (source, target, rate) in &latest.rates {
        c.execute("INSERT INTO fx_rates(source,target,rate,provider,rate_date) VALUES(?,?,?,?,?) ON CONFLICT(source,target) DO UPDATE SET rate=excluded.rate,provider=excluded.provider,rate_date=excluded.rate_date,updated=CURRENT_TIMESTAMP WHERE fx_rates.rate_date IS NULL OR fx_rates.rate_date<=excluded.rate_date",params![source,target,rate,PROVIDER,latest.date])?;
    }
    c.execute(
        "UPDATE fx_sync SET last_success=CURRENT_TIMESTAMP,rate_date=? WHERE id=1",
        [&latest.date],
    )?;
    Ok(latest.date.clone())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn historical_rates_persist_without_overwriting_current_or_manual_rates() -> Result<()> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        let original = parse(&payload())?;
        let mut newer = payload();
        newer["observations"][0]["d"] = serde_json::json!("2026-01-09");
        newer["observations"][0]["FXUSDCAD"]["v"] = serde_json::json!("1.50");
        store(&c, &parse(&newer)?)?;
        let sync_date: String = c.query_row("SELECT rate_date FROM fx_sync", [], |r| r.get(0))?;
        assert!(dated(&c, "USD", "CAD", "2026-01-04").is_err());
        store_dated(&c, "2026-01-04", &original)?;
        assert_eq!(dated(&c, "USD", "CAD", "2026-01-04")?, 135000000);
        assert_eq!(cached(&c, "USD", "CAD")?, Some(150000000));
        assert_eq!(
            c.query_row("SELECT rate_date FROM fx_sync", [], |r| r
                .get::<_, String>(0))?,
            sync_date
        );
        c.execute("UPDATE fx_rates SET rate=160000000,provider='Manual',rate_date=NULL WHERE source='USD' AND target='CAD'", [])?;
        store_dated(&c, "2026-01-04", &original)?;
        assert_eq!(dated(&c, "USD", "CAD", "2026-01-04")?, 160000000);
        assert_eq!(c.query_row("SELECT count(*) FROM fx_history WHERE rate_date='2026-01-02' AND provider='Bank of Canada' AND fetched_at IS NOT NULL", [], |r| r.get::<_, i64>(0))?, 56);
        Ok(())
    }
    #[test]
    fn historical_response_bounds_and_partial_failures_preserve_cache() -> Result<()> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        let mut days = parse(&payload())?;
        assert!(store_dated(&c, "2025-12-31", &days).is_err());
        days.push(DailyRates {
            date: "2026-01-03".into(),
            rates: vec![],
        });
        assert!(store_dated(&c, "2026-01-04", &days).is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM fx_history", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            history_window("2026-01-04")?,
            ("2025-12-28".into(), "2026-01-04".into())
        );
        assert!(history_window(&(crate::today() + chrono::Duration::days(1)).to_string()).is_err());
        Ok(())
    }
    #[test]
    #[ignore = "requires access to the Bank of Canada API"]
    fn real_historical_lookup_is_saved_for_offline_reuse() -> Result<()> {
        let days = fetch_dated("2025-12-31")?;
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        store_dated(&c, "2025-12-31", &days)?;
        assert!(dated(&c, "USD", "CAD", "2025-12-31")? > 0);
        assert!(dated(&c, "EUR", "USD", "2025-12-31")? > 0);
        Ok(())
    }
    fn payload() -> Value {
        let mut day = serde_json::json!({"d":"2026-01-02"});
        for code in &CODES[1..] {
            day[format!("FX{code}CAD")] =
                serde_json::json!({"v":if *code=="USD" {"1.35"} else {"0.20"}});
        }
        serde_json::json!({"observations":[day]})
    }
    #[test]
    fn sync_is_atomic_and_keeps_history_and_task_snapshots() -> Result<()> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        crate::init(&c)?;
        let days = parse(&payload())?;
        assert_eq!(days[0].rates.len(), 56);
        store(&c, &days)?;
        store(&c, &days)?;
        assert_eq!(crate::exchange(&c, "USD", "CAD")?, 135000000);
        assert_eq!(crate::exchange(&c, "CAD", "USD")?, 74074074);
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM fx_history", [], |r| r
                .get::<_, i64>(0))?,
            56
        );
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer','');INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','hour',100,200,'USD');")?;
        let task = crate::add_task(&c, "Task", 1, "CAD")?;
        crate::add_item(&c, task, 1, 1, None, None)?;
        let saved = crate::snapshot(&c, task)?;
        let mut next = payload();
        next["observations"][0]["d"] = serde_json::json!("2026-01-05");
        next["observations"][0]["FXUSDCAD"]["v"] = serde_json::json!("1.50");
        store(&c, &parse(&next)?)?;
        assert_eq!(crate::exchange(&c, "USD", "CAD")?, 150000000);
        assert_eq!(crate::snapshot(&c, task)?, saved);
        assert_eq!(saved["items"][0]["fx_provider"], PROVIDER);
        assert_eq!(saved["items"][0]["fx_date"], "2026-01-02");
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM fx_history", [], |r| r
                .get::<_, i64>(0))?,
            112
        );
        assert!(parse(&serde_json::json!({"observations":[]})).is_err());
        let mut bad = payload();
        bad["observations"][0]["FXUSDCAD"]["v"] = serde_json::json!("-1");
        assert!(parse(&bad).is_err());
        assert_eq!(crate::exchange(&c, "USD", "CAD")?, 150000000);
        Ok(())
    }
    #[test]
    fn cache_first_queries_only_missing_pairs_and_supports_task_transactions() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        let days = parse(&payload())?;
        {
            let tx = c.transaction()?;
            assert_eq!(resolve(&tx, "USD", "CAD", || Ok(days))?, 135000000);
            tx.commit()?;
        }
        assert_eq!(
            resolve(&c, "USD", "CAD", || panic!(
                "cache hit must not use network"
            ))?,
            135000000
        );
        assert!(resolve(&c, "USD", "BAD", || panic!(
            "invalid currency must not use network"
        ))
        .is_err());
        c.execute(
            "DELETE FROM fx_rates WHERE source='EUR' AND target='CAD'",
            [],
        )?;
        assert!(resolve(&c, "EUR", "CAD", || anyhow::bail!("offline")).is_err());
        assert_eq!(cached(&c, "USD", "CAD")?, Some(135000000));
        let mut broken = parse(&payload())?;
        broken.push(DailyRates {
            date: "2026-01-05".into(),
            rates: Vec::new(),
        });
        assert!(store(&c, &broken).is_err());
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM fx_history", [], |r| r
                .get::<_, i64>(0))?,
            56
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Quote {
    pub rate: i64,
    pub target: String,
    pub provider: String,
    pub effective_date: String,
    pub fetched_at: String,
}

pub fn posting_quote(c: &Connection, currency: &str, date: &str) -> Result<Quote> {
    let rate = crate::gl::quote(c, currency, date)?;
    let target = crate::gl::base(c)?;
    if currency == target {
        return Ok(Quote {
            rate,
            target,
            provider: "Base currency".into(),
            effective_date: date.into(),
            fetched_at: String::new(),
        });
    }
    let row=c.query_row("SELECT provider,COALESCE(rate_date,''),updated FROM fx_rates WHERE source=? AND target=? AND provider='Manual' AND rate_date IS NULL",params![currency,target],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).optional()?;
    let since = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")? - chrono::Duration::days(7);
    let row=match row {Some(row)=>Some(row),None=>c.query_row("SELECT provider,rate_date,fetched_at FROM fx_history WHERE source=? AND target=? AND rate_date BETWEEN ? AND ? ORDER BY rate_date DESC,fetched_at DESC,provider LIMIT 1",params![currency,target,since.to_string(),date],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?};
    let (provider,effective_date,fetched_at)=match row {Some(row)=>row,None=>c.query_row("SELECT provider,rate_date,updated FROM fx_rates WHERE source=? AND target=? AND rate_date BETWEEN ? AND ?",params![currency,target,since.to_string(),date],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?};
    Ok(Quote {
        rate,
        target,
        provider,
        effective_date,
        fetched_at,
    })
}
