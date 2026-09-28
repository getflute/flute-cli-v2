//! Arguments shared by more than one group.
//!
//! Pagination is written once here and reused by every later `list`, so the
//! 0-based index, the 1–100 bound and the omit-when-absent rule cannot drift
//! between groups.

use crate::api::{ApiClient, ApiError};
use crate::cli::money::PatchNumber;
use anyhow::Result;
use rust_decimal::Decimal;
use serde_json::Value;

#[derive(clap::Args, Debug, Default, Clone)]
pub struct PaginationArgs {
    /// Page number to fetch. Zero-based.
    #[arg(long, id = "page_index", value_name = "PAGE_INDEX")]
    pub page_index: Option<u32>,
    /// Maximum results per page (1-100). Omit it to use the server's default.
    #[arg(long, id = "page_size", value_name = "PAGE_SIZE")]
    pub page_size: Option<u32>,
    /// Fetch every page until the collection is exhausted.
    #[arg(long, id = "all")]
    pub all: bool,
}

/// An empty value clears a field; `None` leaves it alone.
///
/// On a merge-patch update an explicit null is the removal and an absent key
/// is "leave this as it is", so the two cases a flag can be in map onto both.
/// A `create` has nothing to remove and keeps omitting its blanks.
pub fn patch_string(value: &Option<String>) -> Option<Value> {
    value.as_ref().map(|v| {
        if v.is_empty() {
            Value::Null
        } else {
            Value::String(v.clone())
        }
    })
}

/// The numeric counterpart: an empty value clears, `None` leaves it alone.
///
/// `to_amount_number` decides how a set value is spelt, so the caller passes it
/// in rather than this choosing between an amount and a rate.
pub fn patch_number<E: Into<anyhow::Error>>(
    value: Option<PatchNumber>,
    set: impl Fn(Decimal) -> std::result::Result<Value, E>,
) -> Result<Option<Value>> {
    match value {
        None => Ok(None),
        Some(PatchNumber::Clear) => Ok(Some(Value::Null)),
        Some(PatchNumber::Set(d)) => set(d).map(Some).map_err(Into::into),
    }
}

/// Refuse an empty value for a field the API will not clear.
///
/// These map to non-nullable columns, so the API answers `cannot be cleared`.
/// Refusing here spends no round trip on a rejection the CLI can see coming.
pub fn reject_unclearable(flag: &str, value: Option<&str>) -> Result<()> {
    if value.is_some_and(str::is_empty) {
        anyhow::bail!("--{flag} cannot be cleared; pass a value.");
    }
    Ok(())
}

/// Refuse a processor id passed empty. It is a value that went missing, such
/// as an unset shell variable, and a request sent without it runs through the
/// account's default processor instead of the one named.
pub fn reject_empty_processor_id(flag: &str, value: Option<&str>) -> Result<()> {
    if value.is_some_and(str::is_empty) {
        anyhow::bail!(
            "{flag} needs a value. `flute2 settings payment-config` lists the \
             processors configured for this account."
        );
    }
    Ok(())
}

/// The bound the parameter schema declares. Refusing it here spends no round
/// trip on a 400 the CLI could see coming.
const PAGE_SIZE: std::ops::RangeInclusive<u32> = 1..=100;

impl PaginationArgs {
    pub fn validate(&self) -> Result<()> {
        if let Some(size) = self.page_size {
            if !PAGE_SIZE.contains(&size) {
                anyhow::bail!(
                    "--page-size must be between {} and {} (got {size})",
                    PAGE_SIZE.start(),
                    PAGE_SIZE.end()
                );
            }
        }
        if self.all && self.page_index.is_some() {
            anyhow::bail!(
                "--all walks the whole collection, so --page-index contradicts it. \
                 Drop one."
            );
        }
        Ok(())
    }

    /// The wire pairs, omitting an absent flag entirely.
    ///
    /// `--all` contributes none: the walk positions every page and carries the
    /// batch size itself, so a pair sent from here would arrive twice.
    pub fn query(&self) -> Vec<(&'static str, String)> {
        if self.all {
            return Vec::new();
        }
        let mut out = Vec::new();
        if let Some(index) = self.page_index {
            out.push(("pageIndex", index.to_string()));
        }
        if let Some(size) = self.page_size {
            out.push(("pageSize", size.to_string()));
        }
        out
    }
}

/// Case-sensitive on the wire. `AccountType` declares exactly `Checking` and
/// `Savings`, and the schema sets `additionalProperties: false`, so a
/// lowercase near-miss is rejected rather than forgiven.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum AccountType {
    Checking,
    Savings,
}

impl AccountType {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Checking => "Checking",
            Self::Savings => "Savings",
        }
    }
}

/// Case-sensitive on the wire: `Auto` or `Manual`. The `CaptureMethod` enum
/// declares exactly those two with `Auto` as the default, and `"auto"` is not
/// a near-miss the server forgives.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum CaptureMethod {
    #[default]
    Auto,
    Manual,
}

impl CaptureMethod {
    /// The capitalised wire value. `--capture-method` accepts lowercase on the
    /// command line, as clap value-enums conventionally do.
    pub fn wire(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Manual => "Manual",
        }
    }
}

/// `Card` or `Cash`, capitalised on the wire.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum PricingType {
    Card,
    Cash,
}

impl PricingType {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Card => "Card",
            Self::Cash => "Cash",
        }
    }
}

/// Likewise `Business` and `Personal`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum AccountHolderType {
    Business,
    Personal,
}

impl AccountHolderType {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Business => "Business",
            Self::Personal => "Personal",
        }
    }
}

/// Parse `MM/YY` or `MM/YYYY` into `(month, year)`.
pub fn parse_exp(s: &str) -> Result<(u32, u32)> {
    let parts: Vec<&str> = s.splitn(2, '/').collect();
    if parts.len() != 2 {
        anyhow::bail!("expiry must be MM/YY or MM/YYYY (got '{s}')");
    }
    // Both halves are digits and nothing else: `u32::from_str` accepts a
    // leading `+`, which counts towards the year's length and reads as a year
    // the caller never typed.
    let digits_only = |token: &str| !token.is_empty() && token.bytes().all(|b| b.is_ascii_digit());
    if !digits_only(parts[0]) {
        anyhow::bail!("invalid month in expiry '{s}'");
    }
    let month: u32 = parts[0]
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid month in expiry '{s}'"))?;
    if month == 0 || month > 12 {
        anyhow::bail!("month must be 01-12, got {month} (in '{s}')");
    }
    let token = parts[1];
    if token.len() != 2 && token.len() != 4 {
        anyhow::bail!(
            "year must be exactly 2 digits (YY) or 4 digits (YYYY), got {} digits (in '{s}')",
            token.len()
        );
    }
    if !digits_only(token) {
        anyhow::bail!("invalid year in expiry '{s}'");
    }
    let raw: u32 = token
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid year in expiry '{s}'"))?;
    Ok((month, if token.len() == 2 { 2000 + raw } else { raw }))
}

/// The body of a success the bundle declares one for.
///
/// The operations that answer bodyless confirm from the request instead and
/// never reach here, so an absent body is the resource the caller asked for
/// going missing — rendering it as `null` reports a success with no content.
/// A JSON `null` is that same absence spelled out, and no response schema
/// declares it.
pub fn body_of(body: Option<Value>) -> Result<Value, ApiError> {
    match body {
        Some(Value::Null) | None => Err(ApiError::Decode(
            "the response declares a body and carried none".to_string(),
        )),
        Some(value) => Ok(value),
    }
}

/// The JSON type of a value, for a message a caller can act on.
pub(crate) fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// The declared `items` of one page.
///
/// Every page schema declares `items` nullable and optional, so an absent or
/// null one is an empty page — and so is an empty array. That is what stops a
/// walk. A body that is not an object, or an `items` that is present and is
/// neither an array nor null, is a shape this CLI cannot read, and calling it
/// empty would report "no results" for a response nobody parsed.
pub fn items_of(page: &Value) -> Result<Vec<Value>, ApiError> {
    let Some(object) = page.as_object() else {
        return Err(ApiError::Decode(format!(
            "a collection response must be a JSON object, got {}",
            json_type(page)
        )));
    };
    match object.get("items") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => Ok(items.clone()),
        Some(other) => Err(ApiError::Decode(format!(
            "a collection response's `items` must be an array or null, got {}",
            json_type(other)
        ))),
    }
}

/// Whether the API says another page exists.
///
/// `hasMore` only. No field of `pageInfo` is declared required, so an absent
/// one stops the walk, and `totalPages` is a count rather than a statement
/// about *more* — inferring from it is how a walk loops past the end. The
/// field is declared a boolean and nothing else, so a present value of any
/// other type is unreadable rather than a quiet "no": reading it as one would
/// report a collection complete after its first page.
pub fn has_more(page: &Value) -> Result<bool, ApiError> {
    match page.pointer("/pageInfo/hasMore") {
        None => Ok(false),
        Some(Value::Bool(more)) => Ok(*more),
        Some(other) => Err(ApiError::Decode(format!(
            "a collection response's `hasMore` must be a boolean, got {}",
            json_type(other)
        ))),
    }
}

/// More pages than any collection this API paginates could hold.
const MAX_PAGES: u32 = 10_000;

/// Exhaust a paginated collection, following `pageInfo.hasMore`.
///
/// The page index is sent explicitly on every request, page zero included:
/// the walk is deliberately positioned rather than relying on the server's
/// default for its first request and its own arithmetic afterwards.
///
/// Returns the union of every page's items and the last correlation id, which
/// is the only one a caller could quote about the whole sequence.
///
/// `hasMore` is the server's claim, and the walk is bounded against it twice:
/// a page identical to the one before it means the index was ignored, and a
/// run past the page cap means the end is never coming. Both end the walk as
/// a decode failure rather than a request loop nobody can interrupt.
pub async fn fetch_all(
    api: &ApiClient,
    path: &'static str,
    filters: &[(&str, String)],
    page_size: Option<u32>,
) -> Result<(Vec<Value>, Option<String>), ApiError> {
    let mut collected = Vec::new();
    let mut correlation_id = None;
    let mut previous: Option<Vec<Value>> = None;
    for index in 0..MAX_PAGES {
        let mut query: Vec<(&str, String)> = filters.to_vec();
        query.push(("pageIndex", index.to_string()));
        if let Some(size) = page_size {
            query.push(("pageSize", size.to_string()));
        }
        let resp = api
            .request(reqwest::Method::GET, path, &query, None)
            .await?;
        correlation_id = resp.correlation_id.or(correlation_id);
        let page = body_of(resp.body)?;
        let items = items_of(&page)?;
        if previous.as_ref() == Some(&items) {
            return Err(ApiError::Decode(format!(
                "page {index} repeated the previous page's items: the server is \
                 ignoring pageIndex"
            )));
        }
        if items.is_empty() || !has_more(&page)? {
            collected.extend(items);
            return Ok((collected, correlation_id));
        }
        collected.extend(items.iter().cloned());
        previous = Some(items);
    }
    Err(ApiError::Decode(format!(
        "gave up after {MAX_PAGES} pages with the collection still unexhausted"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `u32::from_str` accepts a leading `+`, and `+023` is four characters
    /// long, so the sign would pass the digit-count check and send the year
    /// 23 under a spelling nobody typed.
    #[test]
    fn rejects_a_signed_month_or_year() {
        for bad in ["12/+023", "12/+2032", "+1/2032", "+12/2032"] {
            assert!(parse_exp(bad).is_err(), "{bad} was accepted");
        }
    }

    #[test]
    fn parses_two_and_four_digit_years_and_rejects_the_rest() {
        assert_eq!(parse_exp("12/32").unwrap(), (12, 2032));
        assert_eq!(parse_exp("01/2032").unwrap(), (1, 2032));
        assert!(parse_exp("13/2032").is_err());
        assert!(parse_exp("00/2032").is_err());
        assert!(parse_exp("12/203").is_err());
        assert!(parse_exp("122032").is_err());
    }

    fn args(page_index: Option<u32>, page_size: Option<u32>, all: bool) -> PaginationArgs {
        PaginationArgs {
            page_index,
            page_size,
            all,
        }
    }

    /// Absent means absent: the parameter is not sent, so the server's own
    /// defaults (0 and 20) govern. v1 always sent a `--limit` it had
    /// defaulted to 25, which made the server default unreachable.
    #[test]
    fn absent_pagination_flags_send_no_query_at_all() {
        assert!(args(None, None, false).query().is_empty());
    }

    #[test]
    fn present_pagination_flags_use_the_api_wire_names() {
        let q = args(Some(2), Some(50), false).query();
        assert_eq!(
            q,
            vec![
                ("pageIndex", "2".to_string()),
                ("pageSize", "50".to_string())
            ]
        );
    }

    /// Straight from the parameter schema: minimum 1, maximum 100.
    #[test]
    fn page_size_is_bounded_one_to_one_hundred() {
        assert!(args(None, Some(1), false).validate().is_ok());
        assert!(args(None, Some(100), false).validate().is_ok());
        assert!(args(None, Some(0), false).validate().is_err());
        assert!(args(None, Some(101), false).validate().is_err());
    }

    /// Under `--all` the walk owns the pagination, so the flags contribute
    /// nothing to the filters it is given.
    #[test]
    fn pagination_under_all_is_left_to_the_walk() {
        assert!(args(None, Some(5), true).query().is_empty());
    }

    /// A zero page index is a legitimate value, not an absent one.
    #[test]
    fn page_index_zero_is_sent_explicitly() {
        assert_eq!(
            args(Some(0), None, false).query(),
            vec![("pageIndex", "0".to_string())]
        );
    }

    /// `--all` exhausts the collection, so a starting page contradicts it.
    #[test]
    fn all_with_a_page_index_is_refused() {
        let err = args(Some(2), None, true)
            .validate()
            .unwrap_err()
            .to_string();
        assert!(err.contains("--all"), "{err}");
        // A batch size is not a starting point, so this combination is fine.
        assert!(args(None, Some(10), true).validate().is_ok());
    }

    /// `hasMore` is the only field the walk may consult, and no field of
    /// `pageInfo` is declared required.
    #[test]
    fn has_more_is_false_when_the_field_is_absent() {
        assert!(!has_more(&serde_json::json!({"pageInfo": {"pageIndex": 0}})).unwrap());
        assert!(!has_more(&serde_json::json!({})).unwrap());
        assert!(!has_more(&serde_json::json!({"pageInfo": {"hasMore": false}})).unwrap());
        assert!(has_more(&serde_json::json!({"pageInfo": {"hasMore": true}})).unwrap());
    }

    /// A `hasMore` the walk cannot read must not be taken for "no more": the
    /// collection would be reported complete having been read to page one.
    #[test]
    fn a_has_more_that_is_not_a_boolean_is_a_decode_error() {
        for (page, expected_type) in [
            (
                serde_json::json!({"pageInfo": {"hasMore": "true"}}),
                "string",
            ),
            (serde_json::json!({"pageInfo": {"hasMore": 1}}), "number"),
            (serde_json::json!({"pageInfo": {"hasMore": null}}), "null"),
        ] {
            let Err(ApiError::Decode(message)) = has_more(&page) else {
                panic!("{page} was accepted");
            };
            assert!(message.contains(expected_type), "{message}");
        }
    }

    /// `totalPages` alone must never imply a further page: it is a count, and
    /// the walk's only stop condition is what the API says about *more*.
    #[test]
    fn total_pages_alone_does_not_imply_another_page() {
        assert!(
            !has_more(&serde_json::json!({"pageInfo": {"pageIndex": 0, "totalPages": 8}})).unwrap()
        );
    }

    #[test]
    fn items_of_a_page_are_the_declared_items_array() {
        assert_eq!(
            items_of(&serde_json::json!({"items": [{"a": 1}]}))
                .unwrap()
                .len(),
            1
        );
        // An empty array is an empty page, and that is what stops the walk.
        assert!(
            items_of(&serde_json::json!({"items": [], "pageInfo": {}}))
                .unwrap()
                .is_empty()
        );
    }

    /// `items` is declared nullable and optional on every page schema, so
    /// absent and null are both an empty page.
    #[test]
    fn an_absent_or_null_items_array_is_an_empty_page() {
        assert!(
            items_of(&serde_json::json!({"pageInfo": {}}))
                .unwrap()
                .is_empty()
        );
        assert!(
            items_of(&serde_json::json!({"items": null, "pageInfo": {}}))
                .unwrap()
                .is_empty()
        );
    }

    /// A page the walk cannot read must not be reported as "no results", and
    /// the message names the type that arrived so a caller can quote it.
    #[test]
    fn a_body_that_is_not_a_page_is_a_decode_error() {
        for (body, expected_type) in [
            (serde_json::json!({"items": "oops"}), "string"),
            (serde_json::json!({"items": 7}), "number"),
            (serde_json::json!([{"a": 1}]), "array"),
            (serde_json::Value::Null, "null"),
        ] {
            let Err(ApiError::Decode(message)) = items_of(&body) else {
                panic!("{body} was accepted");
            };
            assert!(message.contains(expected_type), "{message}");
        }
    }

    /// Thirteen operations answer bodyless and are confirmed from the request
    /// instead; everywhere else a missing body is a missing resource.
    #[test]
    fn an_absent_body_on_a_body_bearing_operation_is_a_decode_error() {
        assert!(matches!(body_of(None), Err(ApiError::Decode(_))));
        // A JSON `null` is a body that names no resource, which is the same
        // absence spelled out.
        assert!(matches!(
            body_of(Some(Value::Null)),
            Err(ApiError::Decode(_))
        ));
        assert_eq!(
            body_of(Some(serde_json::json!({"a": 1}))).unwrap(),
            serde_json::json!({"a": 1})
        );
    }
}
