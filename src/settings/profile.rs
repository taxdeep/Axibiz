use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

/// Optional company metadata. Unknown legacy values remain unknown on upgrade.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompanyProfile {
    pub country: String,
    pub phone: String,
    pub fax: String,
    pub email: String,
    pub website: String,
    pub legal_name: String,
    pub legal_address: String,
    pub legal_city: String,
    pub legal_province: String,
    pub legal_postal_code: String,
    pub legal_country: String,
    pub legal_same_as_contact: bool,
    pub business_number: String,
    pub gst_hst_number: String,
    pub qst_number: String,
    pub fiscal_year_start_month: Option<u8>,
    pub tax_year_start_month: Option<u8>,
    pub income_tax_form: String,
}
impl CompanyProfile {
    pub fn normalized(&self) -> Result<Self> {
        let mut value = serde_json::to_value(self)?;
        for field in value.as_object_mut().expect("profile object").values_mut() {
            if let Some(text) = field.as_str() {
                ensure!(
                    text.len() <= 4096 && !text.contains('\0'),
                    "Company field is too long or contains an invalid character."
                );
                *field = serde_json::Value::String(text.trim().to_owned());
            }
        }
        let mut p: Self = serde_json::from_value(value)?;
        let digits = |text: &str| {
            text.chars()
                .filter(|c| !c.is_ascii_whitespace() && *c != '-')
                .collect::<String>()
        };
        p.business_number = digits(&p.business_number);
        p.gst_hst_number = digits(&p.gst_hst_number).to_ascii_uppercase();
        ensure!(
            p.business_number.is_empty()
                || (p.business_number.len() == 9
                    && p.business_number.bytes().all(|b| b.is_ascii_digit())),
            "Business number must contain 9 digits."
        );
        let gst = p.gst_hst_number.as_bytes();
        ensure!(
            gst.is_empty()
                || (gst.len() == 15
                    && gst[..9].iter().all(u8::is_ascii_digit)
                    && &gst[9..11] == b"RT"
                    && gst[11..].iter().all(u8::is_ascii_digit)),
            "GST/HST account must use the format 123456789RT0001."
        );
        ensure!(
            p.business_number.is_empty()
                || gst.is_empty()
                || p.gst_hst_number.starts_with(&p.business_number),
            "Business number and GST/HST account must identify the same business."
        );
        for month in [p.fiscal_year_start_month, p.tax_year_start_month]
            .into_iter()
            .flatten()
        {
            ensure!(
                (1..=12).contains(&month),
                "Year start month must be January through December."
            );
        }
        ensure!(
            p.email.is_empty()
                || (p
                    .email
                    .split_once('@')
                    .is_some_and(|(name, host)| !name.is_empty()
                        && host.contains('.')
                        && !host.contains('@'))
                    && !p.email.chars().any(char::is_whitespace)),
            "Enter a valid email address."
        );
        ensure!(
            p.website.is_empty()
                || ((p.website.starts_with("https://") || p.website.starts_with("http://"))
                    && p.website
                        .split_once("://")
                        .is_some_and(|(_, host)| !host.is_empty())
                    && !p.website.chars().any(char::is_whitespace)),
            "Website must begin with https:// or http://."
        );
        Ok(p)
    }
}
