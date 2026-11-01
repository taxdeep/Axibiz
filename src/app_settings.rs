//! Machine-local software preferences, independent of company data and restore.
use anyhow::{ensure, Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Clock {
    #[default]
    TwentyFour,
    Twelve,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Zone {
    #[default]
    Vancouver,
    System,
    Utc,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub automatic_backup: bool,
    pub backup_directory: String,
    pub daily_backups_kept: u16,
    pub clock: Clock,
    pub zone: Zone,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            automatic_backup: true,
            backup_directory: String::new(),
            daily_backups_kept: 14,
            clock: Clock::TwentyFour,
            zone: Zone::Vancouver,
        }
    }
}
pub fn path() -> Result<PathBuf> {
    let root = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|p| PathBuf::from(p).join(".config"))
        })
        .context("Cannot locate the user preferences directory.")?;
    Ok(root.join("Axibiz Desktop Beta").join("preferences.json"))
}
impl Preferences {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e).context("Cannot read software settings"),
        };
        let settings: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("Invalid software settings at {}. The file has been preserved; repair it or move it aside to use defaults.", path.display()))?;
        settings.validate()?;
        Ok(settings)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=365).contains(&self.daily_backups_kept),
            "Keep between 1 and 365 daily backups."
        );
        ensure!(
            self.backup_directory.is_empty() || Path::new(&self.backup_directory).is_absolute(),
            "Choose an absolute backup directory."
        );
        Ok(())
    }
    pub fn save(&self, path: &Path) -> Result<Self> {
        let mut settings = self.clone();
        settings.backup_directory = settings.backup_directory.trim().to_owned();
        settings.validate()?;
        if !settings.backup_directory.is_empty() {
            let folder = Path::new(&settings.backup_directory);
            std::fs::create_dir_all(folder).context("Cannot create the backup directory")?;
            let probe = folder.join(format!(
                ".axibiz-write-check-{}-{}",
                std::process::id(),
                Utc::now().timestamp_nanos_opt().unwrap_or_default()
            ));
            let file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&probe)
                .context("The backup directory is not writable")?;
            drop(file);
            std::fs::remove_file(&probe)?;
        }
        let parent = path.parent().context("Invalid preferences path")?;
        std::fs::create_dir_all(parent)?;
        crate::security::restrict(parent, true)?;
        let pending = path.with_extension(format!(
            "pending-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let result = (|| -> Result<()> {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&pending)?;
            crate::security::restrict(&pending, false)?;
            file.write_all(&serde_json::to_vec_pretty(&settings)?)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&pending, path).context("Cannot replace software settings")?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&pending);
        }
        result?;
        Ok(settings)
    }
    pub fn backup_root(&self, database: &Path) -> PathBuf {
        if self.backup_directory.is_empty() {
            database.parent().unwrap_or(Path::new(".")).join("backups")
        } else {
            PathBuf::from(&self.backup_directory)
        }
    }
    pub fn display_time(&self, input: &str) -> String {
        let parsed = DateTime::parse_from_rfc3339(input)
            .map(|d| d.with_timezone(&Utc))
            .ok()
            .or_else(|| {
                NaiveDateTime::parse_from_str(input, "%Y-%m-%d %H:%M:%S")
                    .ok()
                    .map(|d| d.and_utc())
            });
        let Some(time) = parsed else {
            return input.to_owned();
        };
        let format = match self.clock {
            Clock::TwentyFour => "%Y-%m-%d %H:%M:%S %:z",
            Clock::Twelve => "%Y-%m-%d %I:%M:%S %p %:z",
        };
        match self.zone {
            Zone::Vancouver => time
                .with_timezone(&chrono_tz::America::Vancouver)
                .format(format)
                .to_string(),
            Zone::System => time
                .with_timezone(&chrono::Local)
                .format(format)
                .to_string(),
            Zone::Utc => time.format(format).to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn software_preferences_replace_atomically_and_reject_invalid_files_and_directories(
    ) -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "axibiz-preferences-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let path = root.join("config/preferences.json");
        assert_eq!(Preferences::load(&path)?, Preferences::default());
        let mut preferences = Preferences {
            backup_directory: root.join("backups").to_string_lossy().into_owned(),
            daily_backups_kept: 3,
            clock: Clock::Twelve,
            ..Default::default()
        };
        preferences.save(&path)?;
        preferences.zone = Zone::Utc;
        preferences.save(&path)?;
        assert_eq!(Preferences::load(&path)?, preferences);
        let original = std::fs::read(&path)?;
        preferences.backup_directory = root.join("not-a-directory").to_string_lossy().into_owned();
        std::fs::write(&preferences.backup_directory, b"keep this file")?;
        assert!(preferences.save(&path).is_err());
        assert_eq!(std::fs::read(&path)?, original);
        assert_eq!(
            std::fs::read(&preferences.backup_directory)?,
            b"keep this file"
        );
        preferences.backup_directory = "relative-folder".into();
        assert!(preferences.save(&path).is_err());
        assert_eq!(std::fs::read(&path)?, original);
        std::fs::write(&path, b"invalid JSON")?;
        assert!(Preferences::load(&path).is_err());
        assert_eq!(std::fs::read(&path)?, b"invalid JSON");
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
    #[test]
    fn display_clock_handles_utc_offsets_and_dst_without_reinterpreting_document_dates() {
        let mut preferences = Preferences::default();
        assert_eq!(
            preferences.display_time("2026-07-01T20:30:00Z"),
            "2026-07-01 13:30:00 -07:00"
        );
        preferences.clock = Clock::Twelve;
        assert_eq!(
            preferences.display_time("2026-01-01 20:30:00"),
            "2026-01-01 12:30:00 PM -08:00"
        );
        assert_eq!(
            preferences.display_time("2026-11-01T08:30:00Z"),
            "2026-11-01 01:30:00 AM -07:00"
        );
        assert_eq!(
            preferences.display_time("2026-11-01T09:30:00Z"),
            "2026-11-01 01:30:00 AM -08:00"
        );
        assert_eq!(preferences.display_time("2026-11-01"), "2026-11-01");
        assert_eq!(preferences.display_time("unknown"), "unknown");
        preferences.zone = Zone::Utc;
        assert_eq!(
            preferences.display_time("2026-07-01T13:30:00-07:00"),
            "2026-07-01 08:30:00 PM +00:00"
        );
    }
}
