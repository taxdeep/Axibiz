use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection};

#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub accent: String,
    pub payment_instructions: String,
    pub payment_terms: String,
    pub footer: String,
}
impl Default for Template {
    fn default() -> Self {
        Self {
            accent: "#16835d".into(),
            payment_instructions: "Please quote the invoice number with your payment.".into(),
            payment_terms: String::new(),
            footer: "Thank you for your business.".into(),
        }
    }
}
pub fn init(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS invoice_template(id INTEGER PRIMARY KEY CHECK(id=1),accent TEXT NOT NULL,payment_instructions TEXT NOT NULL,footer TEXT NOT NULL);")?;
    let has:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('invoice_template') WHERE name='payment_terms')",[],|r|r.get(0))?;
    if !has {
        c.execute_batch(
            "ALTER TABLE invoice_template ADD COLUMN payment_terms TEXT NOT NULL DEFAULT '';",
        )?;
    }
    let default = Template::default();
    c.execute(
        "INSERT OR IGNORE INTO invoice_template(id,accent,payment_instructions,footer) VALUES(1,?,?,?)",
        params![default.accent, default.payment_instructions, default.footer],
    )?;
    Ok(())
}
impl Template {
    pub fn load(c: &Connection) -> Result<Self> {
        Ok(c.query_row(
            "SELECT accent,payment_instructions,footer,payment_terms FROM invoice_template WHERE id=1",
            [],
            |r| {
                Ok(Self {
                    accent: r.get(0)?,
                    payment_instructions: r.get(1)?,
                    footer: r.get(2)?,
                    payment_terms:r.get(3)?,
                })
            },
        )?)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.accent.len() == 7
                && self.accent.starts_with('#')
                && self.accent[1..].bytes().all(|b| b.is_ascii_hexdigit()),
            "Accent color must be a six-digit hex color, e.g. #16835d"
        );
        ensure!(
            self.payment_instructions.len() <= 4000
                && self.footer.len() <= 2000
                && self.payment_terms.len() <= 1000,
            "Template text is too long"
        );
        Ok(())
    }
    pub fn save(&self, c: &Connection) -> Result<()> {
        self.validate()?;
        c.execute(
            "UPDATE invoice_template SET accent=?,payment_instructions=?,footer=?,payment_terms=? WHERE id=1",
            params![self.accent, self.payment_instructions, self.footer,self.payment_terms],
        )?;
        Ok(())
    }
    pub fn render(&self, fields: &[(&str, String)]) -> Result<String> {
        self.validate()?;
        let mut fields = fields.to_vec();
        fields.extend([
            (
                "PAYMENT_TERMS",
                crate::escape(&self.payment_terms).replace('\n', "<br>"),
            ),
            ("ACCENT", self.accent.clone()),
            (
                "PAYMENT_INSTRUCTIONS",
                crate::escape(&self.payment_instructions).replace('\n', "<br>"),
            ),
            ("FOOTER", crate::escape(&self.footer).replace('\n', "<br>")),
        ]);
        let mut sections = include_str!("../templates/invoice.html").split("{{");
        let mut result = sections.next().unwrap_or("").to_owned();
        for section in sections {
            let (key, tail) = section
                .split_once("}}")
                .context("Invalid invoice template placeholder")?;
            let value = &fields
                .iter()
                .find(|(name, _)| *name == key)
                .context("Unknown invoice template placeholder")?
                .1;
            result.push_str(value);
            result.push_str(tail);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn templates_upgrade_persist_escape_and_do_not_change_invoice_snapshots() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        crate::init(&c)?;
        c.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer','');INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','hour',0,5000,'CAD');")?;
        let task = crate::add_task(&c, "Services", 1, "CAD")?;
        crate::add_item(&c, task, 1, 2, None, None)?;
        crate::set_task_status(&mut c, task, false)?;
        let mut issued_template = Template::load(&c)?;
        issued_template.payment_terms = "Net 30".into();
        issued_template.payment_instructions = "Original bank instructions".into();
        issued_template.save(&c)?;
        let invoice = crate::billing::merge(&mut c, &[task])?;
        let original: String =
            c.query_row("SELECT snapshot FROM invoices WHERE id=?", [invoice], |r| {
                r.get(0)
            })?;
        // Simulate the previously shipped schema without the new template table.
        c.execute_batch("DROP TABLE invoice_template;PRAGMA user_version=12;")?;
        crate::init(&c)?;
        let mut template = Template::load(&c)?;
        assert_eq!(template, Template::default());
        template.accent = "#123abc".into();
        template.payment_terms = "Net 60".into();
        template.payment_instructions = "Use reference <INV> & bank transfer\n{{NUMBER}}".into();
        template.footer = "Thank you <script>alert(1)</script>".into();
        template.save(&c)?;
        crate::init(&c)?;
        assert_eq!(Template::load(&c)?, template);
        let html = crate::invoice::document(&c, invoice)?;
        assert!(html.contains("lang=\"en\""));
        assert!(html.contains("#123abc"));
        assert!(!html.contains("&lt;INV&gt; &amp; bank transfer<br>{{NUMBER}}"));
        assert!(crate::invoice::preview(&c, &template)?
            .contains("&lt;INV&gt; &amp; bank transfer<br>{{NUMBER}}"));
        assert!(html.contains("Net 30") && !html.contains("Net 60"));
        assert!(html.contains("Original bank instructions"));
        assert!(!html.contains("<script>"));
        assert!(!html.contains("UTC"));
        assert!(!html
            .chars()
            .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch)));
        assert_eq!(
            c.query_row("SELECT snapshot FROM invoices WHERE id=?", [invoice], |r| r
                .get::<_, String>(0))?,
            original
        );
        template.accent = "red;bad".into();
        assert!(template.save(&c).is_err());
        assert_eq!(Template::load(&c)?.accent, "#123abc");
        let backup = std::env::temp_dir().join(format!(
            "axibiz-template-{}.db",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        crate::backup::save(&c, &backup)?;
        crate::backup::verify(&backup)?;
        let saved = Connection::open(&backup)?;
        assert_eq!(Template::load(&saved)?.footer, template.footer);
        drop(saved);
        std::fs::remove_file(backup)?;
        Ok(())
    }
}
