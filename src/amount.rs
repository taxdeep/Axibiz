use anyhow::{ensure, Context, Result};
pub fn parse_money(s: &str) -> Result<i64> {
    let parts: Vec<_> = s.split('.').collect();
    ensure!(parts.len() <= 2 && !parts[0].is_empty(), "金额格式错误");
    ensure!(
        parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit())),
        "金额必须为非负数"
    );
    let whole: i64 = parts[0].parse()?;
    let fraction = if parts.len() == 2 {
        ensure!(parts[1].len() <= 2, "金额最多两位小数");
        parts[1].parse::<i64>()? * if parts[1].len() == 1 { 10 } else { 1 }
    } else {
        0
    };
    whole
        .checked_mul(100)
        .and_then(|n| n.checked_add(fraction))
        .context("金额过大")
}
pub fn format_money(n: i64) -> String {
    let n_abs = n.unsigned_abs();
    format!(
        "{}{}.{:02}",
        if n < 0 { "-" } else { "" },
        n_abs / 100,
        n_abs % 100
    )
}
