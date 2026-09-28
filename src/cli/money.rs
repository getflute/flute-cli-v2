//! Parse and validate user-supplied money amounts. Never `f64` math.
//!
//! Every flag taking one of these parsers declares `allow_negative_numbers`:
//! a leading `-` otherwise reads as a flag of its own, and the value is
//! refused as a stray token rather than by the rule it breaks. The allowance
//! covers numbers only, so a flag left without a value still reports the
//! missing value instead of consuming the flag after it.

use anyhow::{Result, bail};
use rust_decimal::Decimal;
use std::str::FromStr;

/// A numeric flag's three states on a merge patch.
///
/// A string carries its own removal signal — `""` is not a name or a code — but
/// a decimal has no spelling to spare, so the states live in a type the value
/// parser produces and the body builder reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchNumber {
    /// Send an explicit null, which removes the stored value.
    Clear,
    Set(Decimal),
}

impl PatchNumber {
    /// The value a caller set, if they set one rather than clearing.
    pub fn set(self) -> Option<Decimal> {
        match self {
            Self::Set(d) => Some(d),
            Self::Clear => None,
        }
    }
}

/// An amount, or the empty value that clears it.
pub fn parse_amount_patch(raw: &str) -> Result<PatchNumber> {
    if raw.trim().is_empty() {
        return Ok(PatchNumber::Clear);
    }
    parse_amount(raw).map(PatchNumber::Set)
}

/// A rate, or the empty value that clears it.
pub fn parse_rate_patch(raw: &str) -> Result<PatchNumber> {
    if raw.trim().is_empty() {
        return Ok(PatchNumber::Clear);
    }
    parse_rate(raw).map(PatchNumber::Set)
}

/// Digits with at most one `.`, after an optional leading `-` that the caller
/// refuses by its own rule. `Decimal`'s parser reads `_` as a digit
/// separator, so `10_00` is a thousand to it.
fn is_plain_decimal(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    let mut parts = digits.split('.');
    let whole = parts.next().unwrap_or_default();
    let frac = parts.next().unwrap_or_default();
    parts.next().is_none()
        && !(whole.is_empty() && frac.is_empty())
        && whole
            .bytes()
            .chain(frac.bytes())
            .all(|b| b.is_ascii_digit())
}

/// Parse a user-supplied money amount.
///
/// The API expects a plain decimal string, so scientific notation and a
/// leading `+` are rejected here rather than left to whatever `Decimal`'s
/// parser happens to accept — the policy belongs in code where it is visible.
pub fn parse_amount(raw: &str) -> Result<Decimal> {
    parse_decimal(raw, "amount", 2)
}

/// Parse a rate. Rates are not money: `0.1850` is a legitimate value, so four
/// decimal places are allowed where amounts allow two.
pub fn parse_rate(raw: &str) -> Result<Decimal> {
    parse_decimal(raw, "rate", 4)
}

/// A non-negative plain decimal with at most `max_scale` places, refused in
/// messages that name it `noun`.
fn parse_decimal(raw: &str, noun: &str, max_scale: u32) -> Result<Decimal> {
    let s = raw.trim();
    if s.contains('e') || s.contains('E') {
        bail!("{noun} must be a plain decimal, not scientific notation: {raw}");
    }
    if s.starts_with('+') {
        bail!("{noun} must be a plain decimal without a leading '+': {raw}");
    }
    if !is_plain_decimal(s) {
        bail!("{noun} must be a plain decimal of digits and one '.': {raw}");
    }
    let d = Decimal::from_str(s).map_err(|_| anyhow::anyhow!("invalid {noun}: {raw}"))?;
    // The sign is read from the text: `Decimal` drops it on a zero mantissa,
    // so `-0.00` parses as a plain zero.
    if s.starts_with('-') {
        bail!("{noun} must not be negative: {raw}");
    }
    if d.scale() > max_scale {
        bail!("{noun} must have at most {max_scale} decimal places: {raw}");
    }
    Ok(d)
}

/// The note a rate earns when it reads as a fraction rather than the
/// percentage it is.
///
/// Every rate flag takes a percentage, so `0.18` is eighteen hundredths of a
/// percent. That is a legal rate — a 0.5% surcharge is real — so the value is
/// noted rather than refused. Zero and whole numbers say nothing ambiguous.
pub fn fractional_rate_note(flag: &str, rate: Decimal) -> Option<String> {
    (rate > Decimal::ZERO && rate < Decimal::ONE).then(|| {
        let percent = (rate * Decimal::ONE_HUNDRED).normalize();
        format!("`{flag} {rate}` means {rate}%, not {percent}%")
    })
}

/// Write those notes to stderr, leaving the request, the output and the exit
/// code alone.
pub fn note_fractional_rates(rates: &[(&str, Option<Decimal>)]) {
    for (flag, rate) in rates {
        if let Some(note) = rate.and_then(|r| fractional_rate_note(flag, r)) {
            eprintln!("{note}");
        }
    }
}

/// Render a `Decimal` as a JSON number that keeps its exact decimal form.
///
/// `baseAmount` is declared `number/double`, and routing it through `f64`
/// would make `1234567.89` unrepresentable. `Decimal::to_string` emits the
/// exact digits, and `serde_json`'s `arbitrary_precision` feature parses them
/// into a `Number` that serialises back byte for byte — `"100.00"` stays
/// `100.00`, not `100`, `100.0`, or `0.10000000000000001`.
pub fn to_amount_number(d: Decimal) -> serde_json::Result<serde_json::Value> {
    serde_json::from_str::<serde_json::Number>(&d.to_string()).map(serde_json::Value::Number)
}

#[cfg(test)]
mod patch_tests {
    use super::*;

    /// A number has no spelling for "absent" the way a string has `""`, so the
    /// three states a merge patch needs live in one type the parser produces.
    #[test]
    fn an_empty_value_parses_as_the_clear() {
        assert_eq!(parse_amount_patch("").unwrap(), PatchNumber::Clear);
        assert_eq!(parse_rate_patch("   ").unwrap(), PatchNumber::Clear);
    }

    #[test]
    fn a_number_parses_as_the_value_it_names() {
        assert_eq!(
            parse_amount_patch("10.50").unwrap(),
            PatchNumber::Set(Decimal::from_str("10.50").unwrap())
        );
    }

    /// The patch parsers refuse everything the plain parsers refuse; the empty
    /// value is the one input they read differently, as a clear.
    #[test]
    fn the_underlying_parser_still_refuses_what_it_refused() {
        for bad in ["abc", "1e5", "+1", "-1"] {
            assert!(parse_amount_patch(bad).is_err(), "{bad}");
            assert!(parse_rate_patch(bad).is_err(), "{bad}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    #[test]
    fn rejects_more_than_two_decimal_places() {
        assert!(parse_amount("10.001").is_err());
        assert!(parse_amount("10.00").is_ok());
    }

    #[test]
    fn accepts_an_integer_amount() {
        assert_eq!(
            parse_amount("100").unwrap(),
            Decimal::from_str("100").unwrap()
        );
    }

    #[test]
    fn rejects_negative_amounts() {
        assert!(parse_amount("-5.00").is_err());
    }

    /// A zero mantissa drops the sign inside `Decimal`, so a negative zero
    /// has to be refused from the text the caller typed.
    #[test]
    fn rejects_a_negative_zero_by_the_rule_a_negative_breaks() {
        for raw in ["-0", "-0.0", "-0.00", " -0.00 "] {
            let err = parse_amount(raw).unwrap_err().to_string();
            assert!(err.contains("must not be negative"), "{raw}: {err}");
        }
        assert!(parse_rate("-0.0000").is_err());
    }

    /// The API expects a plain decimal string, so these are refused up front
    /// rather than left to whatever Decimal's parser happens to accept.
    #[test]
    fn rejects_scientific_notation_and_leading_plus() {
        assert!(parse_amount("1e2").is_err());
        assert!(parse_amount("1E2").is_err());
        assert!(parse_amount("+5.00").is_err());
    }

    /// `Decimal`'s parser reads `_` as a digit separator, so `10_00` would
    /// otherwise be a thousand.
    #[test]
    fn rejects_underscores_and_other_non_digits() {
        for raw in ["10_00", "1_0.0_0", "10_", "_10", "1,000", "10.0.0"] {
            assert!(parse_amount(raw).is_err(), "amount {raw}");
            assert!(parse_rate(raw).is_err(), "rate {raw}");
        }
    }

    #[test]
    fn rejects_non_numeric_and_empty() {
        assert!(parse_amount("abc").is_err());
        assert!(parse_amount("").is_err());
    }

    /// Rates are not money: sub-cent precision is legitimate, so they allow
    /// four decimal places where amounts allow two.
    #[test]
    fn rate_allows_four_decimal_places_but_not_five() {
        assert!(parse_rate("0.1850").is_ok());
        assert!(parse_rate("0.18505").is_err());
        assert!(parse_rate("-0.1").is_err());
    }

    /// The flags are percentages, so only a value between zero and one reads
    /// as a fraction typed by habit. A whole-number rate and a zero say
    /// nothing ambiguous — nobody means 1800% by `18`, and `0` is already 0%.
    #[test]
    fn only_a_rate_between_zero_and_one_is_noted() {
        let note = |s: &str| fractional_rate_note("--tip-rate", s.parse().unwrap());
        assert_eq!(
            note("0.18").as_deref(),
            Some("`--tip-rate 0.18` means 0.18%, not 18%")
        );
        assert_eq!(
            note("0.0025").as_deref(),
            Some("`--tip-rate 0.0025` means 0.0025%, not 0.25%")
        );
        assert_eq!(note("0"), None);
        assert_eq!(note("1"), None);
        assert_eq!(note("18.5"), None);
    }

    /// The API declares baseAmount as number/double. Going through f64 would
    /// make 1234567.89 unrepresentable; it must stay exact.
    #[test]
    fn serializes_as_exact_json_number_not_float() {
        let d = Decimal::from_str("1234567.89").unwrap();
        let v = to_amount_number(d).unwrap();
        assert!(v.is_number());
        assert_eq!(serde_json::to_string(&v).unwrap(), "1234567.89");
    }

    #[test]
    fn preserves_trailing_zero_scale() {
        let d = parse_amount("10.50").unwrap();
        assert_eq!(
            serde_json::to_string(&to_amount_number(d).unwrap()).unwrap(),
            "10.50"
        );
    }

    /// The classic float artifact: 0.10 must not become 0.10000000000000001.
    #[test]
    fn no_float_artifact_on_one_tenth() {
        let d = parse_amount("0.10").unwrap();
        assert_eq!(
            serde_json::to_string(&to_amount_number(d).unwrap()).unwrap(),
            "0.10"
        );
    }
}
