use anyhow::{ensure, Context, Result};
use rusqlite::{
    backup::{Backup, StepResult},
    Connection, OpenFlags,
};
use std::{
    path::Path,
    time::{Duration, Instant},
};
/// Apply machine-local daily-backup preferences; always protect schema upgrades.
pub fn automatic(
    c: &Connection,
    path: &Path,
    preferences: &crate::app_settings::Preferences,
) -> Result<()> {
    preferences.validate()?;
    if path.to_string_lossy() == ":memory:" || !path.exists() || path.metadata()?.len() == 0 {
        return Ok(());
    }
    let existing: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='tasks')",
        [],
        |r| r.get(0),
    )?;
    if !existing {
        return Ok(());
    }
    let version: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if !preferences.automatic_backup && version >= crate::SCHEMA_VERSION {
        return Ok(());
    }
    use sha2::{Digest, Sha256};
    let absolute = std::fs::canonicalize(path)?;
    let key = format!(
        "{:x}",
        Sha256::digest(absolute.to_string_lossy().to_lowercase().as_bytes())
    );
    let folder = preferences.backup_root(&absolute).join(&key[..16]);
    std::fs::create_dir_all(&folder)
        .with_context(|| format!("Cannot create backup directory: {}", folder.display()))?;
    crate::security::restrict(&folder, true)?;
    let date = crate::today();
    let daily = folder.join(format!("Axibiz-auto-{date}.db"));
    if preferences.automatic_backup && !daily.exists() {
        save(c, &daily)
            .context("Startup backup failed; opening stopped to protect the original data")?;
        verify(&daily).context("Startup backup validation failed; opening stopped")?;
    }
    if version < crate::SCHEMA_VERSION {
        let upgrade = folder.join(format!(
            "Axibiz-upgrade-v{version}-to-v{}-{}.db",
            crate::SCHEMA_VERSION,
            chrono::Utc::now().format("%Y%m%d-%H%M%S-%f")
        ));
        save(c, &upgrade).context("Required pre-upgrade backup failed; migration stopped")?;
        verify(&upgrade).context("Pre-upgrade backup validation failed; migration stopped")?;
    }
    // Only files created by this routine are rotated; manual and restore backups stay.
    for (prefix, keep) in [
        ("Axibiz-auto-", usize::from(preferences.daily_backups_kept)),
        ("Axibiz-upgrade-", 5),
    ] {
        if prefix == "Axibiz-auto-" && !preferences.automatic_backup {
            continue;
        }
        let mut files = std::fs::read_dir(&folder)?.collect::<std::io::Result<Vec<_>>>()?;
        files.retain(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_file())
                && managed_stamp(&entry.file_name().to_string_lossy(), prefix).is_some()
        });
        files.sort_by_key(|entry| managed_stamp(&entry.file_name().to_string_lossy(), prefix));
        let excess = files.len().saturating_sub(keep);
        for entry in files.into_iter().take(excess) {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
fn managed_stamp(name: &str, prefix: &str) -> Option<chrono::NaiveDateTime> {
    let stem = name.strip_prefix(prefix)?.strip_suffix(".db")?;
    if prefix == "Axibiz-auto-" {
        let date = chrono::NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok()?;
        return (date.to_string() == stem)
            .then(|| date.and_hms_opt(0, 0, 0))
            .flatten();
    }
    let (from, rest) = stem.strip_prefix('v')?.split_once("-to-v")?;
    let (to, stamp) = rest.split_once('-')?;
    if from.is_empty()
        || to.is_empty()
        || !from.bytes().all(|b| b.is_ascii_digit())
        || !to.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%d-%H%M%S-%f").ok()
}
/// Exercise migration and validation in memory; never replace the live database.
pub fn verify(path: &Path) -> Result<()> {
    let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    integrity(&source)?;
    compatible_source(&source)?;
    let mut staged = Connection::open_in_memory()?;
    copy(&source, &mut staged)?;
    crate::init(&staged)?;
    integrity(&staged)?;
    crate::storage::validate_projections(&staged)?;
    let mut stmt = staged.prepare("SELECT id FROM invoices")?;
    for row in stmt.query_map([], |r| r.get::<_, i64>(0))? {
        crate::billing::balance(&staged, row?)?;
    }
    Ok(())
}
fn compatible_source(c: &Connection) -> Result<()> {
    for sql in [
        "SELECT id,name,contact FROM vendors LIMIT 0",
        "SELECT id,vendor,name,unit,cost,price FROM prices LIMIT 0",
        "SELECT id,title,customer,progress FROM tasks LIMIT 0",
        "SELECT id,task,description,unit,quantity,cost,price FROM items LIMIT 0",
        "SELECT id,task,description,amount FROM expenses LIMIT 0",
        "SELECT id,task,created,snapshot,paid FROM invoices LIMIT 0",
    ] {
        c.prepare(sql)
            .context("This is not a compatible Axibiz Desktop Beta backup.")?;
    }
    Ok(())
}
fn copy(source: &Connection, destination: &mut Connection) -> Result<()> {
    let backup = Backup::new(source, destination)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        ensure!(
            Instant::now() < deadline,
            "Database busy. Close other Axibiz Desktop Beta windows and retry the backup or restore."
        );
        match backup.step(128)? {
            StepResult::Done => break,
            StepResult::More => {}
            StepResult::Busy | StepResult::Locked => std::thread::sleep(Duration::from_millis(50)),
            _ => anyhow::bail!("备份返回未知状态"),
        }
    }
    Ok(())
}
fn integrity(c: &Connection) -> Result<()> {
    let status: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    ensure!(status == "ok", "数据库校验失败：{status}");
    let mut stmt = c.prepare("PRAGMA foreign_key_check")?;
    ensure!(stmt.query([])?.next()?.is_none(), "备份包含无效的数据关联");
    Ok(())
}
pub fn save(c: &Connection, path: &Path) -> Result<()> {
    // Refuse to overwrite any existing file, including the live database.
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .context("无法创建备份，请选择一个尚不存在的文件名")?;
    let result = (|| -> Result<()> {
        crate::security::restrict(path, false)?;
        let mut destination = Connection::open(path)?;
        copy(c, &mut destination)?;
        integrity(&destination)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}
pub fn restore(c: &mut Connection, source: &Path, safety: &Path) -> Result<()> {
    crate::auth::require(c)?;
    if crate::auth::required(c)? {
        ensure!(
            crate::auth::current(c)?.admin,
            "Administrator access is required to restore company data."
        );
    }
    let live: String = c.query_row(
        "SELECT file FROM pragma_database_list WHERE name='main'",
        [],
        |r| r.get(0),
    )?;
    if !live.is_empty() {
        ensure!(
            std::fs::canonicalize(&live)? != std::fs::canonicalize(source)?,
            "不能将当前正在使用的数据库作为恢复来源"
        );
    }
    let source = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .context("无法打开备份文件")?;
    integrity(&source)?;
    compatible_source(&source)?;
    // The earliest Task Ledger backups stored customer names directly.
    if source.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='customers')",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        source.prepare("SELECT id,name,contact FROM customers LIMIT 0")?;
    }
    let mut staged = Connection::open_in_memory()?;
    copy(&source, &mut staged)?;
    crate::init(&staged)?;
    integrity(&staged)?;
    crate::storage::validate_projections(&staged)?;
    let invalid: bool = staged.query_row(
        "SELECT EXISTS(SELECT 1 FROM invoices WHERE NOT json_valid(snapshot))",
        [],
        |r| r.get(0),
    )?;
    ensure!(!invalid, "备份中的发票数据无效");
    {
        let mut stmt = staged.prepare("SELECT id FROM invoices")?;
        for row in stmt.query_map([], |r| r.get::<_, i64>(0))? {
            crate::billing::balance(&staged, row?).context("备份中的发票或付款内容校验失败")?;
        }
    }
    let company = crate::Settings::load(&staged)?.company_name;
    let identity = crate::storage::company_id(&staged)?.context("备份缺少公司标识")?;
    staged.execute("INSERT INTO audit_log(entity,record_id,action,after_json) VALUES('database',?,'RESTORE',?)",rusqlite::params![identity,serde_json::json!({"source":source.path(),"safety":safety.to_string_lossy()}).to_string()])?;
    crate::instance::with_company(&company, || {
        crate::instance::with_identity(&identity, || {
            save(c, safety).context("无法创建恢复前备份，已停止恢复")?;
            copy(&staged, c).context("恢复失败，当前数据保留；恢复前备份已保存")?;
            crate::auth::clear();
            Ok(())
        })
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configured_backup_root_retention_and_disabled_daily_preserve_upgrade_protection(
    ) -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "axibiz-backup-preferences-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir(&root)?;
        let path = root.join("company.db");
        let c = Connection::open(&path)?;
        crate::init(&c)?;
        let mut preferences = crate::app_settings::Preferences {
            backup_directory: root.join("custom-backups").to_string_lossy().into_owned(),
            daily_backups_kept: 2,
            ..Default::default()
        };
        automatic(&c, &path, &preferences)?;
        let folder = std::fs::read_dir(&preferences.backup_directory)?
            .next()
            .unwrap()?
            .path();
        let daily = folder.join(format!("Axibiz-auto-{}.db", crate::today()));
        for day in 1..=4 {
            std::fs::copy(
                &daily,
                folder.join(format!("Axibiz-auto-2000-01-{day:02}.db")),
            )?;
        }
        let manual = folder.join("Axibiz-auto-manual.db");
        std::fs::copy(&daily, &manual)?;
        for day in 1..=6 {
            std::fs::copy(
                &daily,
                folder.join(format!(
                    "Axibiz-upgrade-v999-to-v1000-200001{day:02}-000000-000000000.db"
                )),
            )?;
        }
        automatic(&c, &path, &preferences)?;
        assert_eq!(
            std::fs::read_dir(&folder)?
                .filter_map(|e| e.ok())
                .filter(
                    |e| managed_stamp(&e.file_name().to_string_lossy(), "Axibiz-auto-").is_some()
                )
                .count(),
            2
        );
        assert!(manual.exists());
        preferences.automatic_backup = false;
        std::fs::remove_file(&daily)?;
        automatic(&c, &path, &preferences)?;
        assert!(!daily.exists());
        c.pragma_update(None, "user_version", 19)?;
        automatic(&c, &path, &preferences)?;
        assert!(
            !daily.exists(),
            "Disabling daily backup must be respected during upgrade too"
        );
        let upgrade = std::fs::read_dir(&folder)?
            .filter_map(|e| e.ok())
            .find(|e| {
                e.file_name().to_string_lossy().starts_with(&format!(
                    "Axibiz-upgrade-v19-to-v{}-",
                    crate::SCHEMA_VERSION
                ))
            })
            .context("Newest upgrade backup must survive rotation regardless of version sorting")?;
        assert_eq!(
            Connection::open(upgrade.path())?
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
            19
        );
        verify(&upgrade.path())?;
        assert!(manual.exists());
        drop(c);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
    #[test]
    fn automatic_backups_rotate_verify_and_restore() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "axibiz-auto-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir(&root)?;
        let db = root.join("test.db");
        let mut c = Connection::open(&db)?;
        crate::init(&c)?;
        c.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Original','')",
            [],
        )?;
        automatic(&c, &db, &Default::default())?;
        let folder = std::fs::read_dir(root.join("backups"))?
            .next()
            .unwrap()?
            .path();
        let daily = folder.join(format!("Axibiz-auto-{}.db", crate::today()));
        for day in 1..=20 {
            std::fs::copy(
                &daily,
                folder.join(format!("Axibiz-auto-2000-01-{day:02}.db")),
            )?;
        }
        let manual = folder.join("manual.db");
        std::fs::copy(&daily, &manual)?;
        automatic(&c, &db, &Default::default())?;
        let daily_count = std::fs::read_dir(&folder)?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("Axibiz-auto-"))
            .count();
        assert_eq!(daily_count, 14);
        assert!(manual.exists());
        let identity = crate::storage::company_id(&c)?;
        c.execute("UPDATE customers SET name='Changed' WHERE id=1", [])?;
        verify(&daily)?;
        restore(&mut c, &daily, &root.join("safety.db"))?;
        assert_eq!(
            c.query_row("SELECT name FROM customers WHERE id=1", [], |r| r
                .get::<_, String>(0))?,
            "Original"
        );
        assert_eq!(crate::storage::company_id(&c)?, identity);
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM audit_log WHERE action='RESTORE'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        drop(c);
        std::fs::remove_dir_all(&root)?;
        Ok(())
    }
    #[test]
    fn backup_restore_and_invalid_source_preserve_current_data() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "axibiz-backup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::create_dir(&root)?;
        let path = root.join("backup.db");
        let safety = root.join("before.db");
        let invalid = root.join("invalid.db");
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Original','')",
            [],
        )?;
        save(&c, &path)?;
        assert!(save(&c, &path).is_err());
        c.execute("UPDATE customers SET name='Current'", [])?;
        std::fs::write(&invalid, b"not sqlite")?;
        assert!(restore(&mut c, &invalid, &safety).is_err());
        assert!(!safety.exists());
        restore(&mut c, &path, &safety)?;
        assert_eq!(
            c.query_row("SELECT name FROM customers WHERE id=1", [], |r| r
                .get::<_, String>(0))?,
            "Original"
        );
        let before = Connection::open(&safety)?;
        assert_eq!(
            before.query_row("SELECT name FROM customers WHERE id=1", [], |r| r
                .get::<_, String>(0))?,
            "Current"
        );
        drop(before);
        for file in [&path, &safety, &invalid] {
            std::fs::remove_file(file)?;
        }
        std::fs::remove_dir(&root)?;
        Ok(())
    }
}
