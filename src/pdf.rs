//! Offline PDF rendering with a separate, temporary browser profile.
use anyhow::{ensure, Context, Result};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn browser() -> Result<PathBuf> {
    for variable in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
        if let Some(root) = std::env::var_os(variable) {
            for relative in [
                "Microsoft/Edge/Application/msedge.exe",
                "Google/Chrome/Application/chrome.exe",
            ] {
                let path = PathBuf::from(&root).join(relative);
                if path.is_file() {
                    return Ok(path);
                }
            }
        }
    }
    anyhow::bail!("未找到 Microsoft Edge 或 Chrome，请安装浏览器后重试保存 PDF 或打印。")
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        // This exact directory was created exclusively by this operation.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn print_document(html: &str) -> String {
    let script = "<script>window.addEventListener('load',function(){window.focus();window.print();},{once:true});</script>";
    // Keep print scripting separate from the document used for PDF export.
    if let Some(index) = html.rfind("</body>") {
        format!("{}{}{}", &html[..index], script, &html[index..])
    } else {
        format!("{html}{script}")
    }
}

/// Open an interactive browser print preview. Never silently submit to a printer.
/// The private temporary document stays available until the preview window closes.
pub fn print(html: &str) -> Result<()> {
    let browser = browser()?;
    let folder = std::env::temp_dir().join(format!(
        "axibiz-print-{}-{}",
        std::process::id(),
        chrono::Utc::now()
            .timestamp_nanos_opt()
            .context("无法生成临时目录名")?
    ));
    std::fs::create_dir(&folder)?;
    let scratch = Scratch(folder);
    crate::security::restrict(&scratch.0, false)?;
    let input = scratch.0.join("invoice.html");
    std::fs::write(&input, print_document(html))?;
    let url =
        reqwest::Url::from_file_path(&input).map_err(|_| anyhow::anyhow!("打印文件路径无效"))?;
    let mut command = Command::new(browser);
    command
        .args([
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-networking",
            "--disable-background-mode",
        ])
        .arg(format!(
            "--user-data-dir={}",
            scratch.0.join("profile").display()
        ))
        .arg(format!("--app={url}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let status = command.spawn().context("无法打开打印预览")?.wait()?;
    ensure!(status.success(), "打印窗口异常退出，请重试");
    Ok(())
}

pub fn save(html: &str, output: &Path) -> Result<()> {
    ensure!(!output.exists(), "文件已存在，请使用新的 PDF 文件名");
    let browser = browser()?;
    let folder = std::env::temp_dir().join(format!(
        "axibiz-pdf-{}-{}",
        std::process::id(),
        chrono::Utc::now()
            .timestamp_nanos_opt()
            .context("无法生成临时目录名")?
    ));
    std::fs::create_dir(&folder)?;
    let scratch = Scratch(folder);
    crate::security::restrict(&scratch.0, false)?;
    let input = scratch.0.join("invoice.html");
    let pdf = scratch.0.join("invoice.pdf");
    std::fs::write(&input, html)?;
    let url =
        reqwest::Url::from_file_path(&input).map_err(|_| anyhow::anyhow!("临时文件路径无效"))?;
    let mut command = Command::new(browser);
    command
        .args([
            "--headless",
            "--disable-gpu",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-networking",
            "--no-pdf-header-footer",
            "--timeout=20000",
        ])
        .arg(format!(
            "--user-data-dir={}",
            scratch.0.join("profile").display()
        ))
        .arg(format!("--print-to-pdf={}", pdf.display()))
        .arg(url.as_str())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().context("无法启动 PDF 渲染")?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), "PDF 渲染失败，请重试");
            break;
        }
        if started.elapsed() > Duration::from_secs(45) {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("生成 PDF 超时，请重试");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let bytes = std::fs::read(&pdf).context("浏览器没有生成 PDF，请重试")?;
    ensure!(
        bytes.starts_with(b"%PDF-") && bytes.len() > 100,
        "生成的 PDF 文件无效"
    );
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .context("无法创建 PDF，请使用不存在的文件名")?;
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(output);
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interactive_print_uses_a_separate_document() {
        let original = "<!doctype html><html><body><h1>INVOICE</h1></body></html>";
        let printable = print_document(original);
        assert!(printable.contains("window.print()"));
        assert!(printable.find("window.print()").unwrap() < printable.find("</body>").unwrap());
        assert!(!original.contains("<script>"));
    }
    #[test]
    #[ignore = "requires an installed Edge or Chrome; run explicitly for release verification"]
    fn renders_real_pdf_and_refuses_overwrite() -> Result<()> {
        let output = std::env::temp_dir().join(format!(
            "axibiz-pdf-test-{}.pdf",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let mut db = rusqlite::Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'测试客户','');INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'中英文服务 Service','item',0,5000,'CAD');")?;
        let task = crate::add_task(&db, "PDF 多页测试", 1, "CAD")?;
        for _ in 0..45 {
            crate::add_item(&db, task, 1, 1, None, None)?;
        }
        crate::set_task_status(&mut db, task, false)?;
        let invoice = crate::billing::merge(&mut db, &[task])?;
        crate::invoice::export(&db, invoice, &output)?;
        let original = std::fs::read(&output)?;
        assert!(original.starts_with(b"%PDF-"));
        assert!(save("replacement", &output).is_err());
        assert_eq!(std::fs::read(&output)?, original);
        std::fs::remove_file(output)?;
        Ok(())
    }
}
