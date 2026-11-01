use anyhow::{ensure, Context, Result};
use rusqlite::Connection;
use serde_json::Value;
use std::{io::Write, path::Path};
pub fn validate(value: &Value) -> Result<()> {
    ensure!(value.is_object(), "发票必须是对象");
    for key in ["title", "customer"] {
        ensure!(value[key].is_string(), "发票缺少 {key}");
    }
    let items = value["items"].as_array().context("发票缺少明细")?;
    ensure!(!items.is_empty(), "发票没有明细");
    let mut total = 0i64;
    for item in items {
        ensure!(
            item["description"].is_string() && item["unit"].is_string(),
            "发票明细说明或单位无效"
        );
        let quantity = item["quantity"].as_i64().context("发票数量无效")?;
        let price = item["unit_price"].as_i64().context("发票单价无效")?;
        ensure!(quantity > 0 && price >= 0, "发票数量或单价无效");
        let subtotal = quantity.checked_mul(price).context("发票金额过大")?;
        ensure!(
            item["subtotal"].as_i64() == Some(subtotal),
            "发票明细金额不一致"
        );
        total = total.checked_add(subtotal).context("发票合计过大")?;
    }
    let tax_total = if value.get("tax_total").is_some() {
        let mut taxes = 0i64;
        for item in items {
            let tax = &item["tax_snapshot"];
            ensure!(
                tax["net"].as_i64() == item["subtotal"].as_i64(),
                "税额快照与未税金额不一致"
            );
            let amount = tax["tax"].as_i64().context("税额无效")?;
            ensure!(amount >= 0, "税额无效");
            let mut parts_total = 0i64;
            for part in tax["parts"].as_array().context("税目快照无效")? {
                let part_amount = part["amount"].as_i64().context("税目金额无效")?;
                ensure!(part_amount >= 0, "税目金额无效");
                parts_total = parts_total.checked_add(part_amount).context("税额过大")?;
            }
            ensure!(
                parts_total == amount
                    && tax["gross"].as_i64()
                        == tax["net"].as_i64().and_then(|n| n.checked_add(amount)),
                "税额明细不一致"
            );
            taxes = taxes.checked_add(amount).context("税额过大")?;
        }
        ensure!(
            value["net_total"].as_i64() == Some(total)
                && value["tax_total"].as_i64() == Some(taxes),
            "税额合计不一致"
        );
        taxes
    } else {
        0
    };
    ensure!(
        value["total"].as_i64() == total.checked_add(tax_total),
        "发票合计与明细不一致"
    );
    Ok(())
}
pub fn export(c: &Connection, id: i64, output: &Path) -> Result<()> {
    let html = document(c, id)?;
    if output
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
    {
        return crate::pdf::save(&html, output);
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .context("无法创建导出文件，请使用不存在的文件名")?;
    file.write_all(html.as_bytes())?;
    Ok(())
}
pub fn document(c: &Connection, id: i64) -> Result<String> {
    let (raw, date, paid): (String, String, bool) = c
        .query_row(
            "SELECT snapshot,created,paid FROM invoices WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .context("发票不存在")?;
    let mut value: Value = serde_json::from_str(&raw).context("发票数据损坏")?;
    let number: Option<String> =
        c.query_row("SELECT number FROM invoices WHERE id=?", [id], |r| r.get(0))?;
    if let Some(number) = number {
        value["number"] = serde_json::json!(number);
    }
    let balance = crate::billing::balance(c, id)?;
    value["received"] = serde_json::json!(balance.received);
    value["remaining"] = serde_json::json!(balance.remaining);
    validate(&value)?;
    let mut template = crate::invoice_template::Template::load(c)?;
    // Commercial terms are immutable invoice data; appearance may change on reprint.
    template.payment_terms = value["payment_terms"]
        .as_str()
        .unwrap_or("Not recorded on the original invoice; refer to the original agreement.")
        .into();
    template.payment_instructions = value["payment_instructions"].as_str().unwrap_or("").into();
    render(id, &date, paid, &value, &template)
}
fn render(
    id: i64,
    date: &str,
    paid: bool,
    v: &Value,
    template: &crate::invoice_template::Template,
) -> Result<String> {
    validate(v)?;
    let mut rows = String::new();
    let currency = crate::escape(v["currency"].as_str().unwrap_or("UNSET"));
    for item in v["items"].as_array().unwrap() {
        rows.push_str(&format!(r#"<tr><td>{}</td><td>{}</td><td class="num">{}</td><td class="num">{}</td><td class="num">{}</td><td>{}</td></tr>"#,crate::escape(item["description"].as_str().unwrap()),crate::escape(item["unit"].as_str().unwrap()),item["quantity"].as_i64().unwrap(),crate::format_money(item["unit_price"].as_i64().unwrap()),crate::format_money(item["subtotal"].as_i64().unwrap()),crate::escape(item["tax_snapshot"]["code"].as_str().unwrap_or("Legacy"))));
    }
    let number = crate::escape(v["number"].as_str().unwrap_or(""));
    let number = if number.is_empty() {
        format!("INV-{id:06}")
    } else {
        number
    };
    let receipts = format!(
        "Received: {currency} {} · Balance due: {currency} {}",
        crate::format_money(v["received"].as_i64().unwrap_or(0)),
        crate::format_money(
            v["remaining"]
                .as_i64()
                .unwrap_or(v["total"].as_i64().unwrap())
        )
    );
    let task_section = if let Some(tasks) = v["tasks"].as_array().filter(|tasks| !tasks.is_empty())
    {
        let lines = tasks
            .iter()
            .map(|task| {
                format!(
                    "<li>{} · {} · {}</li>",
                    crate::escape(task["number"].as_str().unwrap_or("")),
                    crate::escape(task["title"].as_str().unwrap_or("")),
                    crate::escape(task["date"].as_str().unwrap_or("Not set"))
                )
            })
            .collect::<String>();
        format!("<section><h3>Related tasks</h3><ul>{lines}</ul></section>")
    } else {
        String::new()
    };
    let invoice_date = v["invoice_date"]
        .as_str()
        .unwrap_or_else(|| date.split(' ').next().unwrap_or(date));
    let mut date_section = format!("<p>Invoice date: {}</p>", crate::escape(invoice_date));
    if v["origin"] != "direct" {
        date_section.push_str(&format!(
            "<p>Task date: {}</p>",
            crate::escape(v["task_date"].as_str().unwrap_or("Not set"))
        ));
    }
    let company = &v["company"];
    let mut tax_summary = String::new();
    if let Some(net) = v["net_total"].as_i64() {
        tax_summary.push_str(&format!(
            "<p class=\"num\">Subtotal: {currency} {}</p>",
            crate::format_money(net)
        ));
        let mut groups = std::collections::BTreeMap::<(String, i64, String), i64>::new();
        for item in v["items"].as_array().unwrap() {
            for part in item["tax_snapshot"]["parts"].as_array().unwrap() {
                let key = (
                    part["name"].as_str().unwrap_or("").to_owned(),
                    part["rate"].as_i64().unwrap_or(0),
                    part["registration"].as_str().unwrap_or("").to_owned(),
                );
                let entry = groups.entry(key).or_default();
                *entry = entry
                    .checked_add(part["amount"].as_i64().unwrap())
                    .context("税额过大")?;
            }
        }
        for ((name, rate, registration), amount) in groups {
            tax_summary.push_str(&format!(
                "<p class=\"num\">{} {}% · Tax registration {}: {currency} {}</p>",
                crate::escape(&name),
                crate::tax::format_rate(rate),
                crate::escape(&registration),
                crate::format_money(amount)
            ));
        }
    }
    let status = if paid {
        "Paid"
    } else if v["received"].as_i64().unwrap_or(0) > 0 {
        "Partially paid"
    } else {
        "Unpaid"
    };
    template.render(&[
        ("NUMBER", number),
        (
            "COMPANY_NAME",
            crate::escape(
                company["name"]
                    .as_str()
                    .unwrap_or("Company information not recorded"),
            ),
        ),
        (
            "COMPANY_ADDRESS",
            crate::escape(company["address"].as_str().unwrap_or("")),
        ),
        (
            "COMPANY_CONTACT",
            crate::escape(company["contact"].as_str().unwrap_or("")),
        ),
        (
            "COMPANY_TAX_NUMBER",
            crate::escape(company["tax_number"].as_str().unwrap_or("")),
        ),
        ("CUSTOMER", {
            let name = v["customer_company"]
                .as_str()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(v["customer"].as_str().unwrap());
            let mut customer = crate::escape(name);
            for key in ["customer_address", "customer_email", "customer_phone"] {
                if let Some(value) = v[key].as_str().filter(|s| !s.trim().is_empty()) {
                    customer.push_str("<br>");
                    customer.push_str(&crate::escape(value).replace('\n', "<br>"));
                }
            }
            customer
        }),
        ("PROJECT", crate::escape(v["title"].as_str().unwrap())),
        ("DATES", date_section),
        ("STATUS", status.into()),
        ("TASKS", task_section),
        ("CURRENCY", currency),
        ("ROWS", rows),
        ("TAX_SUMMARY", tax_summary),
        ("TOTAL", crate::format_money(v["total"].as_i64().unwrap())),
        ("RECEIPTS", receipts),
    ])
}
pub fn preview(c: &Connection, template: &crate::invoice_template::Template) -> Result<String> {
    let value = serde_json::json!({
        "number":"PREVIEW", "title":"Sample services", "customer":"Sample Customer",
        "origin":"direct", "invoice_date":crate::today().to_string(),
        "company":crate::Settings::load(c)?.company_snapshot(), "currency":"CAD", "total":10000,
        "items":[{"description":"Professional services", "unit":"hour", "quantity":2, "unit_price":5000, "subtotal":10000}]
    });
    render(0, "", false, &value, template)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_corrupt_amounts_and_escapes_user_text() -> Result<()> {
        let mut value = serde_json::json!({"title":"<script>alert(1)</script>","customer":"A & B","total":100,"items":[{"description":"<b>Work</b>","unit":"item","quantity":1,"unit_price":100,"subtotal":100}]});
        let html = render(1, "2026-10-25", false, &value, &Default::default())?;
        assert!(!html.contains("UTC"));
        assert!(!html.contains("Issue time"));
        assert!(!html.contains("开票时间"));
        assert!(!html.contains("<script>"));
        assert!(html.contains("A &amp; B"));
        value["origin"] = serde_json::json!("direct");
        value["tasks"] = serde_json::json!([]);
        value["invoice_date"] = serde_json::json!("2026-10-23");
        let direct = render(1, "2026-10-27", false, &value, &Default::default())?;
        assert!(direct.contains("Invoice date: 2026-10-23"));
        assert!(!direct.contains("任务日期"));
        assert!(!direct.contains("Related tasks"));
        assert!(direct.contains("&lt;b&gt;Work&lt;/b&gt;"));
        value["items"][0]["quantity"] = serde_json::json!("<img>");
        assert!(validate(&value).is_err());
        value["items"][0]["quantity"] = serde_json::json!(1);
        value["total"] = serde_json::json!(200);
        assert!(validate(&value).is_err());
        Ok(())
    }
}
