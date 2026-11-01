//! Process lifetime guards for both the database and normalized company name.
use anyhow::{ensure, Context, Result};
use rusqlite::{Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, marker::PhantomData, path::Path, rc::Rc};

#[derive(Debug)]
pub struct AlreadyRunning(String);
impl std::fmt::Display for AlreadyRunning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is already open. Switch to the existing Axibiz Desktop Beta window.",
            self.0
        )
    }
}
impl std::error::Error for AlreadyRunning {}
#[derive(Debug)]
struct Token {
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
}
impl Token {
    fn acquire(identity: &str, label: &str) -> Result<Self> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::{
                Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS},
                System::Threading::CreateMutexW,
            };
            let digest = Sha256::digest(identity.as_bytes());
            let name: Vec<u16> = format!("Global\\Axibiz.Instance.v1.{digest:x}")
                .encode_utf16()
                .chain(Some(0))
                .collect();
            // No ownership or waiting: the open handle itself marks process presence.
            // Windows closes it even if the process crashes or is terminated.
            let (handle, error) = unsafe {
                let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
                (handle, GetLastError())
            };
            ensure!(
                !handle.is_null(),
                "无法建立运行锁（{}），为避免同时写入已停止打开",
                std::io::Error::from_raw_os_error(error as i32)
            );
            if error == ERROR_ALREADY_EXISTS {
                unsafe { CloseHandle(handle) };
                return Err(AlreadyRunning(label.into()).into());
            }
            Ok(Self { handle })
        }
        #[cfg(not(windows))]
        {
            let _ = (identity, label);
            anyhow::bail!("单实例保护目前仅支持 Windows");
        }
    }
}
impl Drop for Token {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}
fn normalized_company(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
struct Instance {
    _database: Token,
    company: Option<Token>,
    company_key: String,
}
impl Instance {
    fn transition<T>(&mut self, name: &str, operation: impl FnOnce() -> Result<T>) -> Result<T> {
        let key = normalized_company(name);
        if key == self.company_key {
            return operation();
        }
        let candidate = if key.is_empty() {
            None
        } else {
            Some(Token::acquire(
                &format!("company:{key}"),
                &format!("公司“{}”", name.trim()),
            )?)
        };
        let result = operation()?;
        self.company = candidate;
        self.company_key = key;
        Ok(result)
    }
}
thread_local! {static ACTIVE:RefCell<Option<Instance>>=const {RefCell::new(None)};}
thread_local! {static IDENTITY:RefCell<Option<(String,Token)>>=const {RefCell::new(None)};}
thread_local! {static SESSION_ACTIVE:std::cell::Cell<bool>=const {std::cell::Cell::new(false)};}
pub fn with_identity<T>(uuid: &str, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    // Unit tests may exercise storage without creating a process session.
    if !SESSION_ACTIVE.with(|active| active.get()) {
        return operation();
    }
    IDENTITY.with(|slot| {
        let mut current = slot.borrow_mut();
        if current.as_ref().is_some_and(|(key, _)| key == uuid) {
            return operation();
        }
        let token = Token::acquire(&format!("company-uuid:{uuid}"), "此公司（固定标识）")?;
        let result = operation()?;
        *current = Some((uuid.to_owned(), token));
        Ok(result)
    })
}
pub struct Session {
    _main_thread: PhantomData<Rc<()>>,
}
impl Drop for Session {
    fn drop(&mut self) {
        SESSION_ACTIVE.with(|active| active.set(false));
        IDENTITY.with(|slot| {
            slot.borrow_mut().take();
        });
        ACTIVE.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}
pub fn start(path: &Path) -> Result<Session> {
    let absolute = if path.to_string_lossy() == ":memory:" {
        format!("memory:{}", std::process::id())
    } else {
        let path = if path.exists() {
            std::fs::canonicalize(path)?
        } else {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            std::fs::canonicalize(parent)?.join(path.file_name().context("数据库路径无效")?)
        };
        path.to_string_lossy().to_lowercase()
    };
    let database = Token::acquire(&format!("database:{absolute}"), "当前数据库")?;
    ACTIVE.with(|slot| -> Result<()> {
        let mut slot = slot.borrow_mut();
        ensure!(slot.is_none(), "当前线程已建立运行会话");
        *slot = Some(Instance {
            _database: database,
            company: None,
            company_key: String::new(),
        });
        Ok(())
    })?;
    SESSION_ACTIVE.with(|active| active.set(true));
    Ok(Session {
        _main_thread: PhantomData,
    })
}
pub fn guard_existing_path(path: &Path) -> Result<impl Drop> {
    let absolute = std::fs::canonicalize(path)?
        .to_string_lossy()
        .to_lowercase();
    Token::acquire(&format!("database:{absolute}"), "待迁移的旧数据库")
}
pub fn existing_company(c: &Connection) -> Result<String> {
    let exists: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='settings')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(String::new());
    }
    Ok(
        c.query_row("SELECT company_name FROM settings WHERE id=1", [], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
        .unwrap_or_default(),
    )
}
pub fn with_company<T>(name: &str, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    ACTIVE.with(|slot| {
        let mut active = slot.borrow_mut();
        match active.as_mut() {
            Some(instance) => instance.transition(name, operation),
            None => operation(),
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn unique() -> String {
        format!(
            "Axibiz test {} {}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }
    #[test]
    fn company_identity_and_transition_are_exclusive_and_rollback_safe() -> Result<()> {
        let key = unique();
        let name = format!("  {key}  LTD  ");
        let normalized = normalized_company(&name);
        let mut first = Instance {
            _database: Token::acquire(&format!("database:{}", unique()), "test")?,
            company: None,
            company_key: String::new(),
        };
        first.transition(&name, || Ok(()))?;
        assert!(Token::acquire(&format!("company:{normalized}"), "same company").is_err());
        assert_eq!(
            normalized_company(&name),
            normalized_company(&format!("{key} ltd"))
        );
        let other = unique();
        assert!(first
            .transition(&other, || -> Result<()> { anyhow::bail!("save failed") })
            .is_err());
        let other_token = Token::acquire(
            &format!("company:{}", normalized_company(&other)),
            "released",
        )?;
        assert!(first.transition(&other, || Ok(())).is_err());
        drop(other_token);
        first.transition(&other, || Ok(()))?;
        let old = Token::acquire(&format!("company:{normalized}"), "old released")?;
        drop(old);
        Ok(())
    }
    #[test]
    fn fixed_identity_survives_rename_and_failed_switch() -> Result<()> {
        let session = start(Path::new(":memory:"))?;
        let first = unique();
        let second = unique();
        with_identity(&first, || Ok(()))?;
        with_company(&unique(), || Ok(()))?;
        assert!(Token::acquire(&format!("company-uuid:{first}"), "duplicate").is_err());
        assert!(with_identity(&second, || -> Result<()> {
            anyhow::bail!("restore failed")
        })
        .is_err());
        assert!(Token::acquire(&format!("company-uuid:{first}"), "old retained").is_err());
        drop(Token::acquire(
            &format!("company-uuid:{second}"),
            "candidate released",
        )?);
        drop(session);
        drop(Token::acquire(
            &format!("company-uuid:{first}"),
            "exit released",
        )?);
        Ok(())
    }
    #[test]
    fn child_lock_helper() {
        let Ok(key) = std::env::var("AXIBIZ_TEST_MUTEX") else {
            return;
        };
        let _token = Token::acquire(&key, "child").unwrap();
        std::fs::write(std::env::var("AXIBIZ_TEST_READY").unwrap(), b"ready").unwrap();
        std::thread::sleep(std::time::Duration::from_secs(20));
    }
    #[test]
    fn lock_is_cross_process_and_released_after_termination() -> Result<()> {
        let key = unique();
        let ready = std::env::temp_dir().join(format!("{}.ready", key.replace(' ', "-")));
        struct Child(std::process::Child);
        impl Drop for Child {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut child = Child(
            std::process::Command::new(std::env::current_exe()?)
                .args([
                    "--exact",
                    "instance::tests::child_lock_helper",
                    "--nocapture",
                ])
                .env("AXIBIZ_TEST_MUTEX", &key)
                .env("AXIBIZ_TEST_READY", &ready)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?,
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !ready.exists() {
            ensure!(
                std::time::Instant::now() < deadline,
                "child startup timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(Token::acquire(&key, "duplicate").is_err());
        child.0.kill()?;
        child.0.wait()?;
        let token = Token::acquire(&key, "after crash")?;
        drop(token);
        std::fs::remove_file(ready)?;
        Ok(())
    }
    #[test]
    fn settings_and_restore_cannot_switch_to_a_running_company() -> Result<()> {
        let root = std::env::temp_dir().join(unique().replace(' ', "-"));
        std::fs::create_dir(&root)?;
        let source = root.join("source.db");
        let safety = root.join("before.db");
        let old_name = unique();
        let occupied_name = unique();
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        let mut settings = crate::Settings::load(&c)?;
        settings.company_name = occupied_name.clone();
        settings.save(&c)?;
        crate::backup::save(&c, &source)?;
        let session = start(&root.join("live.db"))?;
        assert!(start(&root.join("live.db")).is_err());
        settings.company_name = old_name.clone();
        settings.save(&c)?;
        let occupied = Token::acquire(
            &format!("company:{}", normalized_company(&occupied_name)),
            "occupied",
        )?;
        settings.company_name = occupied_name.clone();
        assert!(settings.save(&c).is_err());
        assert_eq!(crate::Settings::load(&c)?.company_name, old_name);
        assert!(crate::backup::restore(&mut c, &source, &safety).is_err());
        assert!(!safety.exists());
        assert_eq!(crate::Settings::load(&c)?.company_name, old_name);
        drop(occupied);
        crate::backup::restore(&mut c, &source, &safety)?;
        assert_eq!(crate::Settings::load(&c)?.company_name, occupied_name);
        assert!(Token::acquire(
            &format!("company:{}", normalized_company(&occupied_name)),
            "new held"
        )
        .is_err());
        let old = Token::acquire(
            &format!("company:{}", normalized_company(&old_name)),
            "old released",
        )?;
        drop(old);
        drop(session);
        let released = Token::acquire(
            &format!("company:{}", normalized_company(&occupied_name)),
            "session released",
        )?;
        drop(released);
        std::fs::remove_file(source)?;
        std::fs::remove_file(safety)?;
        std::fs::remove_dir(root)?;
        Ok(())
    }
}
