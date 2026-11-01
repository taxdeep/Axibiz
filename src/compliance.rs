//! Read-only control checks, not a determination of tax eligibility or compliance.
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;

#[derive(Debug)]
pub struct Finding {
    pub error: bool,
    pub code: &'static str,
    pub message: String,
}
fn warning(code: &'static str, message: String) -> Finding {
    Finding {
        error: false,
        code,
        message,
    }
}
pub fn invoice_findings(data: &Value, reference: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    if data["company"]["name"]
        .as_str()
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        findings.push(warning(
            "SUPPLIER_NAME",
            format!("{reference}：缺少开票主体名称，需核对原始单据。"),
        ));
    }
    if data["customer"].as_str().unwrap_or("").trim().is_empty() {
        findings.push(warning(
            "BUYER_NAME",
            format!("{reference}：缺少客户名称。"),
        ));
    }
    if data["payment_terms"]
        .as_str()
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        findings.push(warning("PAYMENT_TERMS",format!("{reference}：没有冻结的付款条款。GST/HST 的部分单据资料要求包含付款条款；请核对金额门槛和原合同，新发票可先在模板中设置。")));
    }
    if let Some(items) = data["items"].as_array() {
        if items
            .iter()
            .any(|item| item["tax_snapshot"]["kind"] == "none" || item["tax_snapshot"].is_null())
        {
            findings.push(warning("TAX_UNSPECIFIED",format!("{reference}：含未指定税务性质的明细；NONE 不代表已确认免税。请核对供应地点、服务性质和登记状态。")));
        }
        if items.iter().any(|item| {
            item["tax_snapshot"]["parts"]
                .as_array()
                .is_some_and(|parts| {
                    parts.iter().any(|part| {
                        part["amount"].as_i64().unwrap_or(0) > 0
                            && part["registration"]
                                .as_str()
                                .unwrap_or("")
                                .trim()
                                .is_empty()
                    })
                })
        }) {
            findings.push(warning(
                "TAX_REGISTRATION",
                format!("{reference}：税额快照含缺失的税务注册号码，请核对原始资料。"),
            ));
        }
    }
    findings
}
pub fn review(c: &Connection) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    if let Err(error) = crate::storage::validate_projections(c) {
        findings.push(Finding {
            error: true,
            code: "DATA_INTEGRITY",
            message: format!("业务快照、税务或总账一致性检查失败：{error:#}"),
        });
    }
    let mut stmt =
        c.prepare("SELECT COALESCE(number,CAST(id AS TEXT)),snapshot FROM invoices ORDER BY id")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (number, raw) = row?;
        match serde_json::from_str::<Value>(&raw) {
            Ok(data) => findings.extend(invoice_findings(&data, &number)),
            Err(error) => findings.push(Finding {
                error: true,
                code: "INVOICE_JSON",
                message: format!("{number}：发票数据无法读取：{error}"),
            }),
        }
    }
    let legacy: i64 = c.query_row(
        "SELECT count(*) FROM tax_cash WHERE voided=1 AND voided_on IS NULL",
        [],
        |r| r.get(0),
    )?;
    if legacy > 0 {
        findings.push(warning("LEGACY_VOID_DATE",format!("{legacy} 条历史税款撤销没有可核实业务日期，旧期间余额需要人工核对；软件未猜填日期。")));
    }
    let historical:i64=c.query_row("SELECT count(*) FROM invoices i WHERE NOT EXISTS(SELECT 1 FROM gl_sources s WHERE s.kind='invoice' AND s.record=i.id)",[],|r|r.get(0))?;
    if historical > 0 {
        findings.push(warning(
            "OPENING_RECONCILIATION",
            format!(
                "{historical} 张历史发票未自动计入 GL，需核对期初应收和历史收入，避免重复入账。"
            ),
        ));
    }
    let early:i64=c.query_row("SELECT count(*) FROM payments p JOIN invoices i ON i.id=p.invoice WHERE p.date<COALESCE(json_extract(i.snapshot,'$.invoice_date'),substr(i.created,1,10)) AND p.voided=0",[],|r|r.get(0))?;
    if early > 0 {
        findings.push(warning("PAYMENT_CHRONOLOGY",format!("{early} 条有效收款早于发票日期。需核实是否客户预收款；当前不提供完整预收/递延收入子账。")));
    }
    let mut invoices = c.prepare("SELECT id FROM invoices ORDER BY id")?;
    for id in invoices.query_map([], |r| r.get::<_, i64>(0))? {
        let id = id?;
        if let Some(balance) = crate::billing::balance_as_of(c, id, &crate::today().to_string())? {
            if !balance.complete {
                findings.push(warning(
                    "HISTORICAL_RECEIPT_DATE",
                    format!(
                        "发票 ID {id} 的 {} 历史收款/撤销日期不完整，不能确定截止日余额。",
                        balance.balance.currency
                    ),
                ));
            }
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn review_is_read_only_and_flags_missing_documents() -> Result<()> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        let before = c.total_changes();
        let _ = review(&c)?;
        assert_eq!(c.total_changes(), before);
        let findings = invoice_findings(
            &serde_json::json!({"company":{"name":""},"customer":"Buyer","items":[{"tax_snapshot":{"kind":"none"}}]}),
            "INV1",
        );
        assert!(findings.iter().any(|f| f.code == "SUPPLIER_NAME"));
        assert!(findings.iter().any(|f| f.code == "PAYMENT_TERMS"));
        assert!(findings.iter().any(|f| f.code == "TAX_UNSPECIFIED"));
        Ok(())
    }
}
