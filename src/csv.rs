//! CSV data encoding shared by exports. User text is never interpreted as a formula.
pub enum Cell {
    Text(String),
    Amount(i64),
}
fn field(value: &str, untrusted: bool) -> String {
    let dangerous = untrusted
        && value
            .trim_start_matches(char::is_whitespace)
            .starts_with(['=', '+', '-', '@']);
    let value = if dangerous || untrusted && value.starts_with(['\t', '\r', '\n']) {
        format!("'{value}")
    } else {
        value.into()
    };
    format!("\"{}\"", value.replace('"', "\"\""))
}
pub fn encode(headers: &[&str], rows: &[Vec<Cell>]) -> Vec<u8> {
    let mut out = "\u{feff}".to_string();
    out.push_str(
        &headers
            .iter()
            .map(|s| field(s, false))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push_str("\r\n");
    for row in rows {
        out.push_str(
            &row.iter()
                .map(|c| match c {
                    Cell::Text(s) => field(s, true),
                    Cell::Amount(n) => field(&crate::format_money(*n), false),
                })
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push_str("\r\n")
    }
    out.into_bytes()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protects_prefixed_formulas_without_changing_numeric_amounts() {
        let result = String::from_utf8(encode(
            &["Memo", "Amount"],
            &[vec![
                Cell::Text("  =HYPERLINK(\"unsafe\")".into()),
                Cell::Amount(-123),
            ]],
        ))
        .unwrap();
        assert!(result.contains("'  =HYPERLINK(\"\"unsafe\"\")"));
        assert!(result.contains("\"-1.23\""));
    }
}
