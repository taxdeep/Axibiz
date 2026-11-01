use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
pub fn default_database() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let parent = exe.parent().context("程序目录无效")?;
    let root = if parent.file_name().is_some_and(|n| n == "dist") {
        parent.parent().context("程序目录无效")?.to_path_buf()
    } else if let Some(project) = exe
        .ancestors()
        .find(|path| path.join("Cargo.toml").is_file())
    {
        project.to_path_buf()
    } else {
        parent.to_path_buf()
    };
    in_directory(&root)
}
fn in_directory(root: &Path) -> Result<PathBuf> {
    let folder = root.join("database");
    std::fs::create_dir_all(&folder)?;
    crate::security::restrict(&folder, true)?;
    let destination = folder.join("tasks.db");
    let legacy = root.join("tasks.db");
    if legacy.exists() {
        crate::security::database(&legacy)?;
    }
    if !destination.exists() && legacy.exists() {
        // Claim both paths before opening/copying. Keep the original as a recovery copy.
        let _session = crate::instance::start(&destination)?;
        let _legacy = crate::instance::guard_existing_path(&legacy)?;
        let source = Connection::open_with_flags(&legacy, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        // A protected legacy company must be unlocked before any copy/migration.
        // Open it in place so the normal desktop/CLI sign-in gate can run first.
        if crate::auth::required(&source)? {
            return Ok(legacy);
        }
        let company = crate::instance::existing_company(&source)?;
        crate::instance::with_company(&company, || Ok(()))?;
        if let Some(uuid) = crate::storage::company_id(&source)? {
            crate::instance::with_identity(&uuid, || Ok(()))?;
        }
        crate::backup::save(&source, &destination).context("旧数据库迁移失败，原文件保留")?;
        if let Err(error) = crate::backup::verify(&destination) {
            // Only this newly created destination may be removed on failure.
            std::fs::remove_file(&destination)?;
            return Err(error.context("旧数据库迁移验证失败，原文件保留"));
        }
    }
    Ok(destination)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protected_legacy_company_is_opened_in_place_without_unauthenticated_copy() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "axibiz-protected-path-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir(&root)?;
        let old = root.join("tasks.db");
        let c = Connection::open(&old)?;
        crate::init(&c)?;
        crate::auth::clear();
        crate::auth::save_user(
            &c,
            "",
            crate::auth::UserChange {
                id: 1,
                username: "owner",
                display_name: "Owner",
                admin: true,
                active: true,
                password: Some("test-only legacy phrase"),
                remove_password: false,
                require_login: true,
            },
        )?;
        drop(c);
        crate::auth::clear();
        let original = std::fs::read(&old)?;
        assert_eq!(in_directory(&root)?, old);
        assert!(!root.join("database/tasks.db").exists());
        assert_eq!(std::fs::read(&old)?, original);
        std::fs::remove_file(old)?;
        std::fs::remove_dir(root.join("database"))?;
        std::fs::remove_dir(root)?;
        Ok(())
    }
    #[test]
    fn relocation_preserves_data_and_refuses_a_running_source() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "axibiz-path-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir(&root)?;
        let old = root.join("tasks.db");
        let c = Connection::open(&old)?;
        crate::init(&c)?;
        c.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Original','')",
            [],
        )?;
        let uuid = crate::storage::company_id(&c)?;
        drop(c);
        let guard = crate::instance::guard_existing_path(&old)?;
        assert!(in_directory(&root).is_err());
        assert!(!root.join("database/tasks.db").exists());
        drop(guard);
        let path = in_directory(&root)?;
        assert!(old.exists());
        let c = Connection::open(&path)?;
        assert_eq!(crate::storage::company_id(&c)?, uuid);
        assert_eq!(
            c.query_row("SELECT name FROM customers WHERE id=1", [], |r| r
                .get::<_, String>(0))?,
            "Original"
        );
        drop(c);
        assert_eq!(in_directory(&root)?, path);
        std::fs::remove_file(&path)?;
        std::fs::remove_file(&old)?;
        std::fs::remove_dir(root.join("database"))?;
        std::fs::remove_dir(root)?;
        Ok(())
    }
}
