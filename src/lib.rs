#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! Recurring-charge detection for bank-export CSV data.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use time::macros::format_description;
use time::{Date, Duration};

const MIN_OCCURRENCES: usize = 3;

/// Selects which amount direction represents spending in an export.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChargeSign {
    /// Positive amounts are charges.
    Positive,
    /// Negative amounts are charges.
    Negative,
}

/// One parsed charge from a bank export.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transaction {
    /// Posting date.
    pub date: Date,
    /// Original statement description.
    pub description: String,
    /// Positive charge amount in cents.
    pub amount_cents: i64,
}

/// A detected recurring merchant series.
#[derive(Clone, Debug, Serialize)]
pub struct RecurringCharge {
    /// Normalized merchant key used for grouping.
    pub merchant: String,
    /// Number of observed charges.
    pub occurrences: usize,
    /// First observed charge date.
    pub first_date: Date,
    /// Most recent observed charge date.
    pub last_date: Date,
    /// Median amount in cents.
    pub typical_amount_cents: i64,
    /// Human-readable cadence.
    pub cadence: &'static str,
    /// Median number of days between charges.
    pub median_interval_days: i64,
    /// Expected next charge date.
    pub next_expected: Date,
    /// Estimated annual spend in cents.
    pub annualized_cents: i64,
    /// Confidence score from 0 to 100.
    pub confidence: u8,
    /// Change from first to last charge, in basis points.
    pub change_basis_points: i64,
    /// True when the series meets the price-creep threshold.
    pub price_creep: bool,
}

/// Summary and detected series for one analysis.
#[derive(Clone, Debug, Serialize)]
pub struct Analysis {
    /// Number of spending rows parsed.
    pub parsed_charges: usize,
    /// Number of distinct normalized merchants.
    pub merchant_count: usize,
    /// Detected recurring series.
    pub recurring: Vec<RecurringCharge>,
}

/// CSV or data validation error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalyzeError(pub String);

impl std::fmt::Display for AnalyzeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for AnalyzeError {}

/// Parses and analyzes CSV with `date`, `description`, and `amount` columns.
///
/// Column names are case-insensitive. Dates may use `YYYY-MM-DD`, `MM/DD/YYYY`,
/// or `DD/MM/YYYY`. Amounts may include currency symbols, commas, or accounting
/// parentheses.
///
/// # Errors
///
/// Returns an error when the CSV is malformed, required columns are absent, or
/// a selected spending row has an invalid date or amount.
pub fn analyze_csv(input: &str, charge_sign: ChargeSign) -> Result<Analysis, AnalyzeError> {
    let transactions = parse_csv(input, charge_sign)?;
    Ok(analyze_transactions(&transactions))
}

/// Analyzes already parsed charges.
#[must_use]
pub fn analyze_transactions(transactions: &[Transaction]) -> Analysis {
    let mut groups: BTreeMap<String, Vec<&Transaction>> = BTreeMap::new();
    for transaction in transactions {
        groups
            .entry(normalize_merchant(&transaction.description))
            .or_default()
            .push(transaction);
    }

    let merchant_count = groups.len();
    let mut recurring: Vec<_> = groups
        .into_iter()
        .filter_map(|(merchant, group)| detect_series(merchant, group))
        .collect();
    recurring.sort_by(|left, right| {
        right
            .annualized_cents
            .cmp(&left.annualized_cents)
            .then_with(|| left.merchant.cmp(&right.merchant))
    });

    Analysis {
        parsed_charges: transactions.len(),
        merchant_count,
        recurring,
    }
}

/// Produces a stable merchant key from noisy statement text.
#[must_use]
pub fn normalize_merchant(description: &str) -> String {
    const NOISE: &[&str] = &[
        "ACH",
        "CARD",
        "CHECKCARD",
        "DEBIT",
        "ONLINE",
        "PAYMENT",
        "POS",
        "PURCHASE",
        "RECURRING",
        "VISA",
    ];

    let upper = description.to_ascii_uppercase();
    let mut tokens = Vec::new();
    for raw in upper.split(|character: char| !character.is_ascii_alphanumeric()) {
        if raw.is_empty()
            || NOISE.contains(&raw)
            || raw.chars().all(|character| character.is_ascii_digit())
            || (raw.len() >= 8 && raw.chars().any(|character| character.is_ascii_digit()))
        {
            continue;
        }
        tokens.push(raw);
    }

    if tokens.is_empty() {
        "UNKNOWN MERCHANT".to_owned()
    } else {
        tokens.join(" ")
    }
}

/// Renders cents as a currency amount without floating-point arithmetic.
#[must_use]
pub fn format_money(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let absolute = cents.unsigned_abs();
    format!("{sign}${}.{:02}", absolute / 100, absolute % 100)
}

/// Escapes untrusted text for HTML text and attribute contexts.
#[must_use]
pub fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

/// Renders analysis results as HTML table rows.
#[must_use]
pub fn render_rows(analysis: &Analysis) -> String {
    let mut output = String::new();
    for series in &analysis.recurring {
        let trend = if series.price_creep {
            format!(
                "<span class=\"badge rise\">+{:.1}% price creep</span>",
                series.change_basis_points as f64 / 100.0
            )
        } else if series.change_basis_points < -300 {
            format!(
                "<span class=\"badge fall\">{:.1}% lower</span>",
                series.change_basis_points as f64 / 100.0
            )
        } else {
            "<span class=\"badge steady\">steady</span>".to_owned()
        };
        let _ = write!(
            output,
            "<tr><td><strong>{}</strong><small>{} charges · {} to {}</small></td>\
             <td>{}<small>{}</small></td><td>{}</td><td>{}</td><td>{}%</td><td>{}</td></tr>",
            escape_html(&series.merchant),
            series.occurrences,
            series.first_date,
            series.last_date,
            format_money(series.typical_amount_cents),
            series.cadence,
            trend,
            series.next_expected,
            series.confidence,
            format_money(series.annualized_cents),
        );
    }
    output
}

fn parse_csv(input: &str, charge_sign: ChargeSign) -> Result<Vec<Transaction>, AnalyzeError> {
    if input.trim().is_empty() {
        return Err(AnalyzeError("Paste a CSV export to analyze.".to_owned()));
    }

    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(input.as_bytes());
    let headers = reader
        .headers()
        .map_err(|error| AnalyzeError(format!("Could not read CSV headers: {error}")))?
        .clone();
    let date_index = header_index(&headers, &["date", "posted date", "transaction date"])
        .ok_or_else(|| AnalyzeError("CSV needs a date column.".to_owned()))?;
    let description_index = header_index(
        &headers,
        &["description", "merchant", "name", "details", "memo"],
    )
    .ok_or_else(|| AnalyzeError("CSV needs a description column.".to_owned()))?;
    let amount_index = header_index(&headers, &["amount", "value", "debit"])
        .ok_or_else(|| AnalyzeError("CSV needs an amount column.".to_owned()))?;

    let mut transactions = Vec::new();
    for (offset, row) in reader.records().enumerate() {
        let row_number = offset + 2;
        let row = row.map_err(|error| AnalyzeError(format!("CSV row {row_number}: {error}")))?;
        let raw_amount = row.get(amount_index).unwrap_or_default();
        let signed_cents = parse_cents(raw_amount).ok_or_else(|| {
            AnalyzeError(format!(
                "CSV row {row_number} has an invalid amount: {raw_amount}"
            ))
        })?;
        let is_charge = match charge_sign {
            ChargeSign::Positive => signed_cents > 0,
            ChargeSign::Negative => signed_cents < 0,
        };
        if !is_charge {
            continue;
        }
        let raw_date = row.get(date_index).unwrap_or_default();
        let date = parse_date(raw_date).ok_or_else(|| {
            AnalyzeError(format!(
                "CSV row {row_number} has an unsupported date: {raw_date}"
            ))
        })?;
        let description = row.get(description_index).unwrap_or_default().trim();
        if description.is_empty() {
            return Err(AnalyzeError(format!(
                "CSV row {row_number} has an empty description."
            )));
        }
        transactions.push(Transaction {
            date,
            description: description.to_owned(),
            amount_cents: signed_cents.abs(),
        });
    }
    if transactions.is_empty() {
        return Err(AnalyzeError(
            "No charges matched the selected amount direction.".to_owned(),
        ));
    }
    Ok(transactions)
}

fn header_index(headers: &csv::StringRecord, choices: &[&str]) -> Option<usize> {
    headers.iter().position(|header| {
        choices
            .iter()
            .any(|choice| header.trim().eq_ignore_ascii_case(choice))
    })
}

fn parse_date(input: &str) -> Option<Date> {
    [
        format_description!("[year]-[month]-[day]"),
        format_description!("[month]/[day]/[year]"),
        format_description!("[day]/[month]/[year]"),
    ]
    .iter()
    .find_map(|format| Date::parse(input.trim(), format).ok())
}

fn parse_cents(input: &str) -> Option<i64> {
    let trimmed = input.trim();
    let accounting_negative = trimmed.starts_with('(') && trimmed.ends_with(')');
    let cleaned: String = trimmed
        .chars()
        .filter(|character| character.is_ascii_digit() || matches!(character, '.' | '-'))
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    let negative = accounting_negative || cleaned.starts_with('-');
    let unsigned = cleaned.trim_start_matches('-');
    let mut parts = unsigned.split('.');
    let dollars: i64 = parts.next()?.parse().ok()?;
    let fraction = parts.next().unwrap_or("0");
    if parts.next().is_some() || fraction.len() > 2 || !fraction.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let cents = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().ok()? * 10,
        _ => fraction.parse().ok()?,
    };
    let value = dollars.checked_mul(100)?.checked_add(cents)?;
    Some(if negative { -value } else { value })
}

fn detect_series(merchant: String, mut group: Vec<&Transaction>) -> Option<RecurringCharge> {
    if group.len() < MIN_OCCURRENCES || merchant == "UNKNOWN MERCHANT" {
        return None;
    }
    group.sort_by_key(|transaction| transaction.date);
    let intervals: Vec<i64> = group
        .windows(2)
        .map(|pair| (pair[1].date - pair[0].date).whole_days())
        .collect();
    if intervals.iter().any(|interval| *interval <= 0) {
        return None;
    }
    let median_interval = median(intervals.clone());
    let (cadence, tolerance) = classify_cadence(median_interval)?;
    let interval_consistency = intervals
        .iter()
        .filter(|interval| (*interval - median_interval).abs() <= tolerance)
        .count();
    if interval_consistency * 100 / intervals.len() < 70 {
        return None;
    }

    let amounts: Vec<i64> = group
        .iter()
        .map(|transaction| transaction.amount_cents)
        .collect();
    let typical_amount = median(amounts.clone());
    let amount_consistency = amounts
        .iter()
        .filter(|amount| amount_distance_basis_points(**amount, typical_amount) <= 2_000)
        .count();
    if amount_consistency * 100 / amounts.len() < 70 {
        return None;
    }

    let first = group.first()?;
    let last = group.last()?;
    let change_basis_points = if first.amount_cents == 0 {
        0
    } else {
        (last.amount_cents - first.amount_cents) * 10_000 / first.amount_cents
    };
    let confidence = confidence_score(
        interval_consistency,
        intervals.len(),
        amount_consistency,
        amounts.len(),
        group.len(),
    );
    let next_expected = last.date.checked_add(Duration::days(median_interval))?;

    Some(RecurringCharge {
        merchant,
        occurrences: group.len(),
        first_date: first.date,
        last_date: last.date,
        typical_amount_cents: typical_amount,
        cadence,
        median_interval_days: median_interval,
        next_expected,
        annualized_cents: typical_amount * 365 / median_interval,
        confidence,
        change_basis_points,
        price_creep: change_basis_points >= 300 && last.amount_cents - first.amount_cents >= 50,
    })
}

fn classify_cadence(days: i64) -> Option<(&'static str, i64)> {
    match days {
        5..=9 => Some(("weekly", 2)),
        11..=17 => Some(("every two weeks", 3)),
        25..=35 => Some(("monthly", 5)),
        80..=102 => Some(("quarterly", 11)),
        340..=390 => Some(("yearly", 25)),
        _ => None,
    }
}

fn median(mut values: Vec<i64>) -> i64 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn amount_distance_basis_points(amount: i64, reference: i64) -> i64 {
    if reference == 0 {
        return i64::MAX;
    }
    (amount - reference).abs() * 10_000 / reference
}

fn confidence_score(
    matching_intervals: usize,
    interval_count: usize,
    matching_amounts: usize,
    amount_count: usize,
    occurrences: usize,
) -> u8 {
    let interval_score = matching_intervals * 45 / interval_count;
    let amount_score = matching_amounts * 35 / amount_count;
    let history_score = occurrences.saturating_sub(MIN_OCCURRENCES).min(4) * 5;
    u8::try_from(interval_score + amount_score + history_score).unwrap_or(100)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn test_date(year: i32, month: u8, day: u8) -> Date {
        Date::from_calendar_date(year, Month::try_from(month).unwrap(), day).unwrap()
    }

    const SAMPLE: &str = "date,description,amount\n\
        2026-01-03,STREAMBOX 9821,-9.99\n\
        2026-02-03,STREAMBOX 1734,-9.99\n\
        2026-03-03,STREAMBOX 6612,-10.99\n\
        2026-04-03,STREAMBOX 4319,-10.99\n\
        2026-01-10,GROCERY MART,-83.12\n";

    #[test]
    fn detects_monthly_series_and_price_creep() {
        let analysis = analyze_csv(SAMPLE, ChargeSign::Negative).expect("sample should parse");
        assert_eq!(analysis.parsed_charges, 5);
        assert_eq!(analysis.recurring.len(), 1);
        let series = &analysis.recurring[0];
        assert_eq!(series.merchant, "STREAMBOX");
        assert_eq!(series.cadence, "monthly");
        assert_eq!(series.occurrences, 4);
        assert!(series.price_creep);
        assert_eq!(series.next_expected, test_date(2026, 5, 4));
    }

    #[test]
    fn parses_positive_amount_mode_and_accounting_amounts() {
        let input = "posted date,merchant,value\n2026-01-01,Cloud Books,$12.00\n2026-02-01,Cloud Books,12.00\n2026-03-01,Cloud Books,(12.00)\n2026-03-02,Cloud Books,12.00\n";
        let analysis = analyze_csv(input, ChargeSign::Positive).expect("sample should parse");
        assert_eq!(analysis.parsed_charges, 3);
        assert_eq!(analysis.recurring.len(), 1);
    }

    #[test]
    fn normalizes_common_statement_noise() {
        assert_eq!(
            normalize_merchant("POS PURCHASE StreamBox*90210 2026-03-03"),
            "STREAMBOX"
        );
        assert_eq!(
            normalize_merchant("VISA RECURRING Cloud Books 82736182"),
            "CLOUD BOOKS"
        );
    }

    #[test]
    fn rejects_irregular_series() {
        let dates = [(2026, 1, 1), (2026, 1, 9), (2026, 3, 20), (2026, 4, 5)];
        let transactions: Vec<_> = dates
            .into_iter()
            .map(|(year, month, day)| Transaction {
                date: test_date(year, month, day),
                description: "Odd Shop".to_owned(),
                amount_cents: 1000,
            })
            .collect();
        assert_eq!(analyze_transactions(&transactions).recurring.len(), 0);
    }

    #[test]
    fn rejects_missing_headers() {
        let error = analyze_csv("when,where,total\n2026-01-01,X,10\n", ChargeSign::Positive)
            .expect_err("headers should be rejected");
        assert!(error.to_string().contains("date column"));
    }

    #[test]
    fn escapes_untrusted_merchant_text() {
        assert_eq!(
            escape_html("<script>&\"'"),
            "&lt;script&gt;&amp;&quot;&#39;"
        );
    }

    #[test]
    fn formats_money_without_floats() {
        assert_eq!(format_money(123_456), "$1234.56");
        assert_eq!(format_money(-5), "-$0.05");
    }
}
