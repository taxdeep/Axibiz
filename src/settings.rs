use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
mod profile;
pub use profile::CompanyProfile;

pub fn init(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS company_profile(id INTEGER PRIMARY KEY CHECK(id=1),data TEXT NOT NULL CHECK(json_valid(data) AND json_type(data)='object')); INSERT OR IGNORE INTO company_profile(id,data) VALUES(1,'{}');")?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub company_name: String,
    pub address: String,
    pub contact: String,
    pub tax_number: String,
    pub currency: String,
    pub profile: CompanyProfile,
}
impl Settings {
    pub fn load(c: &Connection) -> Result<Self> {
        let data: String = c.query_row("SELECT data FROM company_profile WHERE id=1", [], |r| {
            r.get(0)
        })?;
        let profile: CompanyProfile =
            serde_json::from_str(&data).context("Invalid company profile")?;
        Ok(c.query_row(
            "SELECT company_name,address,contact,tax_number,currency FROM settings WHERE id=1",
            [],
            |r| {
                Ok(Self {
                    company_name: r.get(0)?,
                    address: r.get(1)?,
                    contact: r.get(2)?,
                    tax_number: r.get(3)?,
                    currency: r.get(4)?,
                    profile,
                })
            },
        )?)
    }
    pub fn save(&self, c: &Connection) -> Result<()> {
        ensure!(
            !self.company_name.trim().is_empty(),
            "Company name is required."
        );
        let currency = crate::currency(&self.currency).map_err(anyhow::Error::msg)?;
        let profile = self.profile.normalized()?;
        crate::instance::with_company(&self.company_name, || {
            let tx = c.unchecked_transaction()?;
            ensure!(tx.execute("UPDATE settings SET company_name=?,address=?,contact=?,tax_number=?,currency=? WHERE id=1",
                params![self.company_name.trim(),self.address.trim(),self.contact.trim(),self.tax_number.trim(),currency])? == 1, "Company settings are missing.");
            ensure!(
                tx.execute(
                    "UPDATE company_profile SET data=? WHERE id=1",
                    [serde_json::to_string(&profile)?],
                )? == 1,
                "Company profile is missing."
            );
            tx.commit()?;
            Ok(())
        })
    }
    pub fn company_snapshot(&self) -> Value {
        let p = &self.profile;
        let mut contacts = vec![self.contact.trim().to_owned()];
        for (label, value) in [
            ("Phone", &p.phone),
            ("Email", &p.email),
            ("Web", &p.website),
        ] {
            if !value.is_empty() && !self.contact.contains(value.as_str()) {
                contacts.push(format!("{label}: {value}"));
            }
        }
        let mut taxes = vec![self.tax_number.trim().to_owned()];
        for (label, value) in [("GST/HST", &p.gst_hst_number), ("QST", &p.qst_number)] {
            if !value.is_empty() && !self.tax_number.contains(value.as_str()) {
                taxes.push(format!("{label}: {value}"));
            }
        }
        json!({"name":self.company_name,"address":self.address,"contact":contacts.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "),"tax_number":taxes.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "),"profile":p})
    }
    pub fn show(&self) {
        println!("\n========== 公司设置 ==========\n公司名称：{}\n公司地址：{}\n联系方式：{}\n税号：{}\n默认结算币种：{}\n==============================",self.company_name,self.address,self.contact,self.tax_number,self.currency);
    }
}
fn field<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    label: &str,
    current: &str,
) -> Result<String> {
    write!(writer, "{label} [{current}]：")?;
    writer.flush()?;
    let mut line = String::new();
    ensure!(reader.read_line(&mut line)? > 0, "输入已结束，设置未保存");
    let line = line.trim();
    Ok(match line {
        "" => current.to_owned(),
        "-" => String::new(),
        _ => line.to_owned(),
    })
}
pub fn edit(c: &Connection) -> Result<()> {
    let mut draft = Settings::load(c)?;
    draft.show();
    println!("回车保留原值，输入 - 清空字段。填写后选择保存或取消。");
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut writer = io::stdout();
    draft.company_name = field(&mut reader, &mut writer, "公司名称", &draft.company_name)?;
    draft.address = field(&mut reader, &mut writer, "公司地址", &draft.address)?;
    draft.contact = field(&mut reader, &mut writer, "联系方式", &draft.contact)?;
    draft.tax_number = field(&mut reader, &mut writer, "税号", &draft.tax_number)?;
    loop {
        let value = field(
            &mut reader,
            &mut writer,
            "结算币种 CAD/USD/CNY/EUR/GBP/HKD/AUD/SGD",
            &draft.currency,
        )?;
        match crate::currency(&value) {
            Ok(value) => {
                draft.currency = value;
                break;
            }
            Err(e) => writeln!(writer, "{e}")?,
        }
    }
    draft.show();
    write!(writer, "输入 s 保存，其他输入取消：")?;
    writer.flush()?;
    let mut confirm = String::new();
    reader.read_line(&mut confirm)?;
    if confirm.trim().eq_ignore_ascii_case("s") {
        draft.save(c).context("保存设置失败")?;
        println!("设置已保存");
    } else {
        println!("已取消，设置未修改");
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn company_changes_do_not_rewrite_issued_invoices() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer',''); INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor',''); INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','item',0,5000,'CAD');")?;
        let mut s = Settings::load(&c)?;
        s.company_name = "Original Ltd".into();
        s.profile.phone = "555 0100".into();
        s.save(&c)?;
        let task = crate::add_task(&c, "Frozen invoice", 1, "CAD")?;
        crate::add_item(&c, task, 1, 1, None, None)?;
        crate::set_task_status(&mut c, task, false)?;
        let invoice = crate::billing::merge(&mut c, &[task])?;
        let before: String =
            c.query_row("SELECT snapshot FROM invoices WHERE id=?", [invoice], |r| {
                r.get(0)
            })?;
        s.company_name = "Updated Ltd".into();
        s.profile.phone = "555 0200".into();
        s.save(&c)?;
        let after: String =
            c.query_row("SELECT snapshot FROM invoices WHERE id=?", [invoice], |r| {
                r.get(0)
            })?;
        assert_eq!(after, before);
        assert_eq!(
            serde_json::from_str::<Value>(&after)?["company"]["profile"]["phone"],
            "555 0100"
        );
        crate::storage::validate_projections(&c)?;
        Ok(())
    }
    #[test]
    fn structured_profile_round_trips_and_freezes_company_information() -> Result<()> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        let mut s = Settings::load(&c)?;
        s.company_name = "Company Ltd".into();
        s.contact = "Accounts team".into();
        s.tax_number = "Legacy reference".into();
        s.profile.email = " billing@example.com ".into();
        s.profile.business_number = "123 456 789".into();
        s.profile.gst_hst_number = "123456789-rt-0001".into();
        s.profile.qst_number = "1234567890TQ0001".into();
        s.profile.legal_name = "Company Legal Ltd".into();
        s.profile.legal_address = "Retained separate address".into();
        s.profile.legal_same_as_contact = true;
        s.profile.fiscal_year_start_month = Some(4);
        s.save(&c)?;
        let saved = Settings::load(&c)?;
        assert_eq!(saved.profile.email, "billing@example.com");
        assert_eq!(saved.profile.gst_hst_number, "123456789RT0001");
        assert_eq!(saved.tax_number, "Legacy reference");
        assert_eq!(saved.profile.legal_address, "Retained separate address");
        let issued = saved.company_snapshot();
        assert!(issued["contact"]
            .as_str()
            .unwrap()
            .contains("billing@example.com"));
        assert!(issued["tax_number"]
            .as_str()
            .unwrap()
            .contains("GST/HST: 123456789RT0001"));
        s.profile.email = "changed@example.com".into();
        s.save(&c)?;
        assert!(issued["contact"]
            .as_str()
            .unwrap()
            .contains("billing@example.com"));
        assert_eq!(issued["profile"]["fiscal_year_start_month"], 4);
        Ok(())
    }
    #[test]
    fn failed_profile_write_rolls_back_base_settings_and_audit() -> Result<()> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        let mut s = Settings::load(&c)?;
        s.company_name = "Before Ltd".into();
        s.save(&c)?;
        let before = Settings::load(&c)?;
        let audit: i64 = c.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0))?;
        c.execute_batch("CREATE TRIGGER reject_profile BEFORE UPDATE ON company_profile BEGIN SELECT RAISE(ABORT,'simulated write failure'); END;")?;
        s.company_name = "After Ltd".into();
        s.profile.phone = "555 0123".into();
        assert!(s.save(&c).is_err());
        assert_eq!(Settings::load(&c)?, before);
        assert_eq!(
            c.query_row("SELECT count(*) FROM audit_log", [], |r| r.get::<_, i64>(0))?,
            audit
        );
        Ok(())
    }
    #[test]
    fn legacy_upgrade_preserves_original_fields_and_unknown_metadata() -> Result<()> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute("UPDATE settings SET company_name='Legacy Ltd',contact='Freeform contact',tax_number='001234-AB' WHERE id=1", [])?;
        c.execute_batch("DROP TABLE company_profile; PRAGMA user_version=19;")?;
        crate::init(&c)?;
        let s = Settings::load(&c)?;
        assert_eq!(s.company_name, "Legacy Ltd");
        assert_eq!(s.contact, "Freeform contact");
        assert_eq!(s.tax_number, "001234-AB");
        assert_eq!(s.profile, CompanyProfile::default());
        Ok(())
    }
    #[test]
    fn invalid_optional_metadata_never_changes_saved_settings() -> Result<()> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        let before = Settings::load(&c)?;
        for profile in [
            CompanyProfile {
                business_number: "123".into(),
                ..Default::default()
            },
            CompanyProfile {
                business_number: "123456789".into(),
                gst_hst_number: "987654321RT0001".into(),
                ..Default::default()
            },
            CompanyProfile {
                gst_hst_number: "123456789".into(),
                ..Default::default()
            },
            CompanyProfile {
                fiscal_year_start_month: Some(13),
                ..Default::default()
            },
            CompanyProfile {
                email: "invalid email".into(),
                ..Default::default()
            },
            CompanyProfile {
                website: "javascript:alert(1)".into(),
                ..Default::default()
            },
        ] {
            let mut draft = before.clone();
            draft.company_name = "Rejected Ltd".into();
            draft.profile = profile;
            assert!(draft.save(&c).is_err());
            assert_eq!(Settings::load(&c)?, before);
        }
        Ok(())
    }
    #[test]
    fn settings_persist_and_validate() -> Result<()> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        let mut s = Settings::load(&c)?;
        assert_eq!(s.currency, "CAD");
        s.company_name = "Example Ltd".into();
        s.tax_number = "001234-AB".into();
        s.currency = "usd".into();
        s.save(&c)?;
        crate::init(&c)?;
        let saved = Settings::load(&c)?;
        assert_eq!(saved.tax_number, "001234-AB");
        assert_eq!(saved.currency, "USD");
        let snapshot = saved.company_snapshot();
        s.company_name = "Changed Ltd".into();
        s.save(&c)?;
        assert_eq!(snapshot["name"], "Example Ltd");
        s.currency = "bad".into();
        assert!(s.save(&c).is_err());
        s.currency = "CAD".into();
        s.company_name = " ".into();
        assert!(s.save(&c).is_err());
        assert_eq!(Settings::load(&c)?.company_name, "Changed Ltd");
        Ok(())
    }
    #[test]
    fn form_retains_and_clears_fields() -> Result<()> {
        let mut input = io::Cursor::new(b"\n-\nNew value\n");
        let mut output = Vec::new();
        assert_eq!(field(&mut input, &mut output, "name", "Old")?, "Old");
        assert_eq!(field(&mut input, &mut output, "name", "Old")?, "");
        assert_eq!(field(&mut input, &mut output, "name", "Old")?, "New value");
        assert!(field(&mut input, &mut output, "name", "Old").is_err());
        Ok(())
    }
}
