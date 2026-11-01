//! OS-account file isolation. Company application sign-in lives in auth.rs.
use anyhow::{Context, Result};
use std::path::Path;

/// Restrict an application-owned file or directory. Never follow reparse points.
pub fn restrict(path: &Path, tree: bool) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Paths are passed as environment data, never interpolated into shell code.
        // Use the system PowerShell, not PATH, and do not load user profiles.
        let shell = std::env::var_os("SystemRoot").context("SystemRoot is unavailable")?;
        let shell =
            std::path::PathBuf::from(shell).join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let output = std::process::Command::new(shell)
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                SCRIPT,
            ])
            .env("AXIBIZ_SECURITY_PATH", std::path::absolute(path)?)
            .env("AXIBIZ_SECURITY_TREE", if tree { "1" } else { "0" })
            .output()
            .context("无法启动 Windows 文件权限保护")?;
        anyhow::ensure!(
            output.status.success(),
            "无法保护数据库文件权限，已停止操作：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::symlink_metadata(path)?;
        anyhow::ensure!(!metadata.file_type().is_symlink(), "Refusing symbolic link");
        std::fs::set_permissions(
            path,
            std::fs::Permissions::from_mode(if metadata.is_dir() { 0o700 } else { 0o600 }),
        )?;
        if tree && metadata.is_dir() {
            for entry in std::fs::read_dir(path)? {
                restrict(&entry?.path(), true)?;
            }
        }
    }
    Ok(())
}

/// Protect the live file and any existing SQLite auxiliary files before opening.
/// Do not change the ACL of an arbitrary --db parent directory.
pub fn database(path: &Path) -> Result<()> {
    if path == Path::new(":memory:") {
        return Ok(());
    }
    if !path.exists() {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .context("无法创建数据库文件")?;
    }
    restrict(path, false)?;
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        let auxiliary = std::path::PathBuf::from(name);
        if auxiliary.exists() {
            restrict(&auxiliary, false)?;
        }
    }
    Ok(())
}

#[cfg(windows)]
const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$identity = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$system = [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18')
$admins = [System.Security.Principal.SecurityIdentifier]::new('S-1-5-32-544')
function Protect-Item([string]$itemPath, [bool]$recursive) {
    $item = Get-Item -LiteralPath $itemPath -Force
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw 'Database protection refuses links and junctions.'
    }
    if ($item.PSIsContainer) {
        $acl = [System.Security.AccessControl.DirectorySecurity]::new()
        $inherit = [System.Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit'
    } else {
        $acl = [System.Security.AccessControl.FileSecurity]::new()
        $inherit = [System.Security.AccessControl.InheritanceFlags]::None
    }
    $acl.SetAccessRuleProtection($true, $false)
    $allowed = @($identity.Value, $system.Value, $admins.Value)
    $owner = $item.GetAccessControl([System.Security.AccessControl.AccessControlSections]::Owner).GetOwner([System.Security.Principal.SecurityIdentifier])
    if ($owner.Value -notin $allowed) { throw 'Database owner must transfer ownership before use.' }
    foreach ($sid in @($identity, $system, $admins)) {
        $rule = [System.Security.AccessControl.FileSystemAccessRule]::new(
            $sid, [System.Security.AccessControl.FileSystemRights]::FullControl,
            $inherit, [System.Security.AccessControl.PropagationFlags]::None,
            [System.Security.AccessControl.AccessControlType]::Allow)
        $acl.AddAccessRule($rule)
    }
    $item.SetAccessControl($acl)
    $check = Get-Acl -LiteralPath $item.FullName
    if (-not $check.AreAccessRulesProtected) { throw 'ACL inheritance was not disabled.' }
    foreach ($rule in $check.GetAccessRules($true, $true, [System.Security.Principal.SecurityIdentifier])) {
        if ($rule.IdentityReference.Value -notin $allowed) { throw 'Unexpected database access rule.' }
    }
    if ($recursive -and $item.PSIsContainer) {
        foreach ($child in Get-ChildItem -LiteralPath $item.FullName -Force) {
            Protect-Item $child.FullName $true
        }
    }
}
Protect-Item $env:AXIBIZ_SECURITY_PATH ($env:AXIBIZ_SECURITY_TREE -eq '1')
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_database_and_backup_remain_usable() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "axibiz-security-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir(&root)?;
        let path = root.join("company '$;.db");
        database(&path)?;
        let c = rusqlite::Connection::open(&path)?;
        crate::init(&c)?;
        let backup = root.join("backup.db");
        crate::backup::save(&c, &backup)?;
        restrict(&root, true)?;
        crate::backup::verify(&backup)?;
        drop(c);
        std::fs::remove_file(path)?;
        std::fs::remove_file(backup)?;
        std::fs::remove_dir(root)?;
        Ok(())
    }
}
