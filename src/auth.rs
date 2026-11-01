//! Company-local application sign-in. SQLite files are not encrypted.
use anyhow::{bail, ensure, Context, Result};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::cell::RefCell;
use zeroize::Zeroizing;

#[derive(Clone)]
struct Session {
    company: String,
    user: i64,
    revision: i64,
    policy: i64,
    name: String,
}
thread_local! { static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) }; }
#[derive(Clone, Debug)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub admin: bool,
    pub active: bool,
    pub has_password: bool,
}
pub struct UserChange<'a> {
    pub id: i64,
    pub username: &'a str,
    pub display_name: &'a str,
    pub admin: bool,
    pub active: bool,
    pub password: Option<&'a str>,
    pub remove_password: bool,
    pub require_login: bool,
}

pub fn register(c: &Connection) -> Result<()> {
    let company = crate::storage::company_id(c)?;
    c.create_scalar_function(
        "axibiz_actor",
        0,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8,
        move |_| Ok(actor_for(company.as_deref())),
    )?;
    Ok(())
}
pub fn init(c: &Connection) -> Result<()> {
    let existing = exists(c)?;
    c.execute_batch("CREATE TABLE IF NOT EXISTS company_users(id INTEGER PRIMARY KEY,username TEXT NOT NULL COLLATE NOCASE UNIQUE,display_name TEXT NOT NULL,admin INTEGER NOT NULL CHECK(admin IN(0,1)),active INTEGER NOT NULL CHECK(active IN(0,1)),password_hash TEXT,revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0));
        CREATE TABLE IF NOT EXISTS company_login(id INTEGER PRIMARY KEY CHECK(id=1),enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN(0,1)),revision INTEGER NOT NULL DEFAULT 1,failures INTEGER NOT NULL DEFAULT 0,locked_until INTEGER NOT NULL DEFAULT 0);
        INSERT OR IGNORE INTO company_login(id) VALUES(1);
        CREATE TRIGGER IF NOT EXISTS company_users_no_delete BEFORE DELETE ON company_users BEGIN SELECT RAISE(ABORT,'Deactivate users instead of deleting history'); END;
        CREATE TRIGGER IF NOT EXISTS company_login_no_delete BEFORE DELETE ON company_login BEGIN SELECT RAISE(ABORT,'Company login configuration cannot be deleted'); END;")?;
    if !existing {
        c.execute("INSERT INTO company_users(username,display_name,admin,active) VALUES('admin','Administrator',1,1)",[])?;
    }
    required(c)?;
    Ok(())
}
fn exists(c: &Connection) -> Result<bool> {
    let count: i64 = c.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN('company_users','company_login')", [], |r|r.get(0))?;
    let version: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    ensure!(
        version <= crate::SCHEMA_VERSION,
        "This database requires the same or a newer version of Axibiz Desktop Beta."
    );
    ensure!(
        count == 2 || (count == 0 && version < 21),
        "Company user configuration is incomplete; opening stopped."
    );
    Ok(count == 2)
}
pub fn required(c: &Connection) -> Result<bool> {
    if !exists(c)? {
        return Ok(false);
    }
    let enabled: bool = c
        .query_row("SELECT enabled FROM company_login WHERE id=1", [], |r| {
            r.get(0)
        })
        .context("Company login configuration is missing")?;
    let admins: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM company_users WHERE admin=1 AND active=1)",
        [],
        |r| r.get(0),
    )?;
    ensure!(
        admins,
        "Company has no active administrator; opening stopped."
    );
    if enabled {
        let missing: Option<String> = c.query_row(
            "SELECT username FROM company_users WHERE active=1 AND password_hash IS NULL ORDER BY id LIMIT 1",
            [],
            |r| r.get(0),
        ).optional()?;
        ensure!(
            missing.is_none(),
            "Set a password for active user '{}' before enabling sign-in.",
            missing.unwrap_or_default()
        );
    }
    Ok(enabled)
}
pub fn clear() {
    SESSION.with(|s| *s.borrow_mut() = None);
}
fn actor_for(company: Option<&str>) -> String {
    SESSION.with(|s| {
        s.borrow()
            .as_ref()
            .filter(|s| Some(s.company.as_str()) == company)
            .map_or_else(|| "local".into(), |s| format!("user:{}:{}", s.user, s.name))
    })
}
pub fn require(c: &Connection) -> Result<()> {
    if !required(c)? {
        return Ok(());
    }
    let session = SESSION
        .with(|s| s.borrow().clone())
        .context("Sign in to this company first.")?;
    let company = crate::storage::company_id(c)?.context("Company identity is missing")?;
    let valid: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM company_users u JOIN company_login p ON p.id=1 WHERE u.id=? AND u.active=1 AND u.revision=? AND p.revision=?)", params![session.user,session.revision,session.policy], |r|r.get(0))?;
    ensure!(
        session.company == company && valid,
        "Sign in to this company again."
    );
    Ok(())
}
fn bind(c: &Connection, user: i64) -> Result<()> {
    let (revision, name, policy) = c.query_row("SELECT u.revision,u.username,p.revision FROM company_users u JOIN company_login p ON p.id=1 WHERE u.id=? AND u.active=1", [user], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    let company = crate::storage::company_id(c)?.context("Company identity is missing")?;
    SESSION.with(|s| {
        *s.borrow_mut() = Some(Session {
            company,
            user,
            revision,
            policy,
            name,
        })
    });
    Ok(())
}
pub fn users(c: &Connection) -> Result<Vec<User>> {
    let mut q=c.prepare("SELECT id,username,display_name,admin,active,password_hash IS NOT NULL FROM company_users ORDER BY id")?;
    let rows = q
        .query_map([], |r| {
            Ok(User {
                id: r.get(0)?,
                username: r.get(1)?,
                display_name: r.get(2)?,
                admin: r.get(3)?,
                active: r.get(4)?,
                has_password: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}
pub fn current(c: &Connection) -> Result<User> {
    require(c)?;
    let session = SESSION.with(|s| s.borrow().clone());
    let company = crate::storage::company_id(c)?;
    let all = users(c)?;
    if let Some(s) = session.filter(|s| Some(&s.company) == company.as_ref()) {
        if let Some(user) = all.iter().find(|u| u.id == s.user && u.active) {
            return Ok(user.clone());
        }
    }
    all.into_iter()
        .find(|u| u.admin && u.active)
        .context("No active administrator")
}
fn engine() -> Result<Argon2<'static>> {
    Ok(Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(19456, 2, 1, Some(32)).map_err(|e| anyhow::anyhow!("{e}"))?,
    ))
}
fn hash(password: &str) -> Result<String> {
    ensure!(
        (12..=128).contains(&password.chars().count()) && password.len() <= 512,
        "Use a password of 12 to 128 characters."
    );
    let salt = SaltString::generate(&mut rand_core::OsRng);
    Ok(engine()?
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Password hashing failed: {e}"))?
        .to_string())
}
fn matches(password: &str, stored: Option<&str>) -> Result<bool> {
    ensure!(password.len() <= 512, "Password is too long.");
    let Some(stored) = stored else {
        return Ok(password.is_empty());
    };
    let parsed = PasswordHash::new(stored)
        .map_err(|_| anyhow::anyhow!("Stored password is invalid; sign-in stopped."))?;
    ensure!(
        parsed.algorithm.as_str() == "argon2id"
            && parsed.version == Some(19)
            && parsed.params.get_decimal("m") == Some(19456)
            && parsed.params.get_decimal("t") == Some(2)
            && parsed.params.get_decimal("p") == Some(1),
        "Unsupported password hash parameters; sign-in stopped."
    );
    Ok(engine()?
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}
fn audit(c: &Connection, user: &str, action: &str, data: serde_json::Value) -> Result<()> {
    c.execute("INSERT INTO audit_log(entity,record_id,action,after_json,actor) VALUES('company_users',?,?,?,?)",params![user,action,data.to_string(),actor_for(crate::storage::company_id(c)?.as_deref())])?;
    Ok(())
}
pub fn sign_in(c: &Connection, username: &str, password: &str) -> Result<()> {
    ensure!(exists(c)?, "Company users are not initialized");
    required(c)?;
    ensure!(password.len() <= 512, "Password is too long.");
    let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
    let now = chrono::Utc::now().timestamp();
    let (failures, until): (i64, i64) = tx.query_row(
        "SELECT failures,locked_until FROM company_login WHERE id=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    ensure!(
        until <= now,
        "Too many attempts. Try again in {} seconds.",
        until - now
    );
    let row:Option<(i64,Option<String>)>=tx.query_row("SELECT id,password_hash FROM company_users WHERE username=? COLLATE NOCASE AND active=1",[username.trim()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let valid = match &row {
        Some((_, stored)) => matches(password, stored.as_deref())?,
        None => {
            let salt = SaltString::encode_b64(&[0; 16]).map_err(|e| anyhow::anyhow!("{e}"))?;
            let _ = engine()?.hash_password(password.as_bytes(), &salt);
            false
        }
    };
    if !valid {
        let failures = if until > 0 { 1 } else { failures + 1 };
        let locked = if failures >= 5 { now + 30 } else { 0 };
        tx.execute(
            "UPDATE company_login SET failures=?,locked_until=? WHERE id=1",
            params![failures, locked],
        )?;
        audit(
            &tx,
            "company",
            "LOGIN_FAILED",
            serde_json::json!({"locked":locked>0}),
        )?;
        tx.commit()?;
        bail!("Incorrect username or password.");
    }
    let id = row.context("User not found")?.0;
    tx.execute(
        "UPDATE company_login SET failures=0,locked_until=0 WHERE id=1",
        [],
    )?;
    audit(
        &tx,
        &id.to_string(),
        "LOGIN",
        serde_json::json!({"user_id":id}),
    )?;
    tx.commit()?;
    bind(c, id)?;
    register(c)
}
pub fn save_user(c: &Connection, current_password: &str, change: UserChange<'_>) -> Result<()> {
    let who = current(c)?;
    sign_in(c, &who.username, current_password)?;
    let username = change.username.trim().to_ascii_lowercase();
    ensure!(
        (2..=64).contains(&username.len())
            && username
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
        "Username must use 2–64 letters, digits, dots, underscores or hyphens."
    );
    ensure!(
        !change.display_name.trim().is_empty() && change.display_name.len() <= 200,
        "Enter a display name (up to 200 bytes)."
    );
    let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
    let who = current(&tx)?;
    let enabled = required(&tx)?;
    if !who.admin {
        ensure!(
            change.id == who.id
                && username.eq_ignore_ascii_case(&who.username)
                && !change.admin
                && change.active
                && change.require_login == enabled
                && !change.remove_password,
            "Only administrators can manage other users or company sign-in."
        );
    }
    let password = if let Some(password) = change.password {
        Some(hash(password)?)
    } else {
        None
    };
    if change.id == 0 {
        ensure!(who.admin, "Administrator access is required.");
        tx.execute("INSERT INTO company_users(username,display_name,admin,active,password_hash) VALUES(?,?,?,?,?)",params![username,change.display_name.trim(),change.admin,change.active,password])?;
    } else {
        ensure!(tx.execute("UPDATE company_users SET username=?,display_name=?,admin=?,active=?,password_hash=CASE WHEN ? THEN NULL WHEN ? IS NOT NULL THEN ? ELSE password_hash END,revision=revision+1 WHERE id=?",params![username,change.display_name.trim(),change.admin,change.active,change.remove_password,password,password,change.id])?==1,"User not found.");
    }
    let id = if change.id == 0 {
        tx.last_insert_rowid()
    } else {
        change.id
    };
    let admins: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM company_users WHERE admin=1 AND active=1)",
        [],
        |r| r.get(0),
    )?;
    ensure!(admins, "Keep at least one active administrator.");
    tx.execute("UPDATE company_login SET enabled=?,revision=revision+1,failures=0,locked_until=0 WHERE id=1",[change.require_login])?;
    required(&tx)?;
    audit(
        &tx,
        &id.to_string(),
        "USER_SAVED",
        serde_json::json!({"username":username,"display_name":change.display_name.trim(),"admin":change.admin,"active":change.active,"password_changed":change.password.is_some() || change.remove_password,"require_login":change.require_login}),
    )?;
    tx.commit()?;
    if change.require_login {
        if who.id == id && !change.active {
            clear();
        } else {
            bind(c, who.id)?;
        }
    } else {
        clear();
    }
    Ok(())
}
pub fn cli_sign_in(c: &Connection) -> Result<()> {
    if !required(c)? {
        clear();
        return Ok(());
    }
    use std::io::{IsTerminal, Write};
    ensure!(std::io::stdin().is_terminal(),"This company requires interactive sign-in. Open the desktop app or run this command in a terminal.");
    print!("Username: ");
    std::io::stdout().flush()?;
    let mut username = String::new();
    std::io::stdin().read_line(&mut username)?;
    let password = Zeroizing::new(rpassword::prompt_password("Password: ")?);
    sign_in(c, &username, &password)
}

#[cfg(test)]
mod tests {
    use super::*;
    const PASSWORD: &str = "a private passphrase 123";
    const OTHER: &str = "another private phrase 456";
    fn company() -> Result<Connection> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        clear();
        Ok(c)
    }
    fn owner(
        c: &Connection,
        name: &str,
        password: Option<&str>,
        enabled: bool,
        current: &str,
    ) -> Result<()> {
        save_user(
            c,
            current,
            UserChange {
                id: 1,
                username: name,
                display_name: "Company Owner",
                admin: true,
                active: true,
                password,
                remove_password: false,
                require_login: enabled,
            },
        )
    }
    #[test]
    fn optional_login_and_company_credentials_are_independent() -> Result<()> {
        let a = company()?;
        let b = company()?;
        assert!(!required(&a)?);
        assert!(!required(&b)?);
        owner(&a, "alice", Some(PASSWORD), true, "")?;
        clear();
        assert!(require(&a).is_err());
        assert!(require(&b).is_ok());
        owner(&b, "alice", Some(OTHER), true, "")?;
        assert!(require(&b).is_ok());
        assert!(require(&a).is_err());
        assert!(sign_in(&a, "alice", OTHER).is_err());
        sign_in(&a, "ALICE", PASSWORD)?;
        assert!(require(&a).is_ok());
        assert!(require(&b).is_err());
        assert!(sign_in(&b, "alice", PASSWORD).is_err());
        sign_in(&b, "alice", OTHER)?;
        assert!(require(&b).is_ok());
        assert!(require(&a).is_err());
        Ok(())
    }
    #[test]
    fn password_changes_admin_rules_and_login_policy_validate_atomically() -> Result<()> {
        let mut c = company()?;
        owner(&c, "owner", Some(PASSWORD), true, "")?;
        save_user(
            &c,
            PASSWORD,
            UserChange {
                id: 0,
                username: "clerk",
                display_name: "Clerk",
                admin: false,
                active: true,
                password: Some(OTHER),
                remove_password: false,
                require_login: true,
            },
        )?;
        clear();
        sign_in(&c, "clerk", OTHER)?;
        let escalation = UserChange {
            id: 2,
            username: "clerk",
            display_name: "Clerk",
            admin: true,
            active: true,
            password: None,
            remove_password: false,
            require_login: true,
        };
        assert!(save_user(&c, OTHER, escalation).is_err());
        assert!(!users(&c)?[1].admin);
        save_user(
            &c,
            OTHER,
            UserChange {
                id: 2,
                username: "clerk",
                display_name: "Clerk",
                admin: false,
                active: true,
                password: Some(PASSWORD),
                remove_password: false,
                require_login: true,
            },
        )?;
        clear();
        assert!(sign_in(&c, "clerk", OTHER).is_err());
        sign_in(&c, "clerk", PASSWORD)?;
        assert!(crate::backup::restore(
            &mut c,
            std::path::Path::new("not-a-backup.db"),
            std::path::Path::new("not-created.db")
        )
        .unwrap_err()
        .to_string()
        .contains("Administrator access"));
        clear();
        sign_in(&c, "owner", PASSWORD)?;
        let before:String=c.query_row("SELECT group_concat(username||display_name||admin||active||revision||COALESCE(password_hash,'')) FROM company_users",[],|r|r.get(0))?;
        assert!(save_user(
            &c,
            PASSWORD,
            UserChange {
                id: 1,
                username: "owner",
                display_name: "Changed",
                admin: false,
                active: true,
                password: None,
                remove_password: false,
                require_login: true
            }
        )
        .is_err());
        let after:String=c.query_row("SELECT group_concat(username||display_name||admin||active||revision||COALESCE(password_hash,'')) FROM company_users",[],|r|r.get(0))?;
        assert_eq!(before, after);
        assert!(save_user(
            &c,
            PASSWORD,
            UserChange {
                id: 0,
                username: "missing",
                display_name: "Missing",
                admin: false,
                active: true,
                password: None,
                remove_password: false,
                require_login: true
            }
        )
        .is_err());
        assert_eq!(users(&c)?.len(), 2);
        owner(&c, "owner", None, false, PASSWORD)?;
        assert!(!required(&c)?);
        assert!(require(&c).is_ok());
        assert!(owner(&c, "owner", Some("short"), true, PASSWORD).is_err());
        assert!(!required(&c)?);
        owner(&c, "owner", Some(OTHER), true, PASSWORD)?;
        clear();
        assert!(sign_in(&c, "owner", PASSWORD).is_err());
        sign_in(&c, "owner", OTHER)?;
        Ok(())
    }
    #[test]
    fn throttling_survives_reopen_and_audit_never_contains_passwords_or_hashes() -> Result<()> {
        let path = std::env::temp_dir().join(format!(
            "axibiz-auth-{}-{}.db",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let c = Connection::open(&path)?;
        crate::init(&c)?;
        clear();
        owner(&c, "owner", Some(PASSWORD), true, "")?;
        let hash: String = c.query_row(
            "SELECT password_hash FROM company_users WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        assert!(hash.starts_with("$argon2id$"));
        assert!(!hash.contains(PASSWORD));
        clear();
        for _ in 0..5 {
            assert!(sign_in(&c, "owner", "wrong password").is_err());
        }
        drop(c);
        let c = Connection::open(&path)?;
        register(&c)?;
        assert!(sign_in(&c, "owner", PASSWORD)
            .unwrap_err()
            .to_string()
            .contains("Too many attempts"));
        c.execute(
            "UPDATE company_login SET locked_until=? WHERE id=1",
            [chrono::Utc::now().timestamp() - 1],
        )?;
        sign_in(&c, "owner", PASSWORD)?;
        c.execute(
            "INSERT INTO customers(name,contact) VALUES('Audited','')",
            [],
        )?;
        let actor: String = c.query_row(
            "SELECT actor FROM audit_log WHERE entity='customers' ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(actor, "user:1:owner");
        let logs: String = c.query_row(
            "SELECT group_concat(COALESCE(before_json,'')||COALESCE(after_json,'')) FROM audit_log",
            [],
            |r| r.get(0),
        )?;
        assert!(!logs.contains(PASSWORD));
        assert!(!logs.contains("wrong password"));
        assert!(!logs.contains("$argon2"));
        assert_ne!(hash, super::hash(PASSWORD)?);
        drop(c);
        std::fs::remove_file(path)?;
        clear();
        Ok(())
    }
    #[test]
    fn restore_follows_backup_users_and_cannot_reuse_another_company_session() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "axibiz-auth-restore-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir(&root)?;
        let mut a = company()?;
        owner(&a, "alice", Some(PASSWORD), true, "")?;
        let b = company()?;
        owner(&b, "bob", Some(OTHER), true, "")?;
        let source = root.join("company-b.db");
        crate::backup::save(&b, &source)?;
        sign_in(&a, "alice", PASSWORD)?;
        crate::backup::restore(&mut a, &source, &root.join("company-a-safety.db"))?;
        assert!(require(&a).is_err());
        assert!(crate::execute(&mut a, crate::Command::AccountingCheck).is_err());
        assert!(sign_in(&a, "alice", PASSWORD).is_err());
        sign_in(&a, "bob", OTHER)?;
        require(&a)?;
        assert_eq!(current(&a)?.username, "bob");
        let old = Connection::open(root.join("company-a-safety.db"))?;
        assert_eq!(users(&old)?[0].username, "alice");
        drop(old);
        drop(a);
        drop(b);
        clear();
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
    #[test]
    fn incomplete_credentials_fail_closed_without_reseeding_admin() -> Result<()> {
        let c = company()?;
        owner(&c, "owner", Some(PASSWORD), true, "")?;
        clear();
        c.execute_batch("DROP TABLE company_users;")?;
        assert!(required(&c).is_err());
        assert!(crate::init(&c).is_err());
        let count: i64 = c.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='company_users'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(count, 0);
        let future = company()?;
        future.pragma_update(None, "user_version", crate::SCHEMA_VERSION + 1)?;
        let changes = future.total_changes();
        assert!(sign_in(&future, "admin", "").is_err());
        assert_eq!(
            future.total_changes(),
            changes,
            "Do not update authentication state in unsupported future databases"
        );
        Ok(())
    }
    #[test]
    fn failed_user_audit_rolls_back_password_and_policy_and_cli_requires_login() -> Result<()> {
        let mut c = company()?;
        owner(&c, "owner", Some(PASSWORD), true, "")?;
        let before:(String,i64,i64)=c.query_row("SELECT u.password_hash,u.revision,p.revision FROM company_users u JOIN company_login p ON p.id=1 WHERE u.id=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        c.execute_batch("CREATE TRIGGER reject_user_audit BEFORE INSERT ON audit_log WHEN NEW.action='USER_SAVED' BEGIN SELECT RAISE(ABORT,'simulated user-save failure'); END;")?;
        assert!(owner(&c, "owner", Some(OTHER), true, PASSWORD).is_err());
        let after:(String,i64,i64)=c.query_row("SELECT u.password_hash,u.revision,p.revision FROM company_users u JOIN company_login p ON p.id=1 WHERE u.id=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        assert_eq!(before, after);
        clear();
        let changes = c.total_changes();
        assert!(crate::execute(&mut c, crate::Command::AccountingCheck).is_err());
        assert_eq!(changes, c.total_changes());
        sign_in(&c, "owner", PASSWORD)?;
        require(&c)?;
        assert!(sign_in(&c, "owner", OTHER).is_err());
        clear();
        Ok(())
    }
}
