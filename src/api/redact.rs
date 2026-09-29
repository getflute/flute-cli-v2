//! PCI masking for anything the client traces.
//!
//! One entry point, `redact`, because a second trace path is how a value
//! reaches a log unmasked. Over-redacting a debug trace costs a support
//! engineer a round trip; under-redacting one is a compliance incident.

use serde_json::Value;

/// Mask everything sensitive in `text`, whatever shape it arrives in.
///
/// Three shapes reach the trace: a JSON body, a form-encoded body (the token
/// exchange), and header lines. JSON is redacted structurally so field names
/// survive; the rest is redacted pairwise. Text matching none of those shapes
/// is suppressed rather than echoed — a plaintext token or an error page
/// reflecting a secret would otherwise leak whole.
pub fn redact(text: &str) -> String {
    match serde_json::from_str::<Value>(text) {
        Ok(v) => scrub_bearer(&redact_value(&v).to_string()),
        Err(_) => scrub_bearer(&mask_digit_runs(&redact_pairs(text))),
    }
}

/// Mask a message that will be shown to a user.
///
/// `redact` suppresses text it cannot recognise, which is right for a trace —
/// an error page reflecting a secret would otherwise leak wholesale — and
/// wrong for a message somebody has to read. This masks the identifiers and
/// keeps the prose.
pub fn redact_message(text: &str) -> String {
    scrub_bearer(&mask_digit_runs(&mask_keyed_values(text)))
}

/// Mask the value that follows a sensitive key name in free text: the keys
/// the JSON redactor masks, then `=`, `:` or whitespace, then the value.
///
/// A CVV or a short account number is sensitive only by the key before it.
/// A value joined to its key with no space (`cvv=123`, `cvv:123`) or quoted
/// is masked whatever it is. A value set off by whitespace (`cvv 123`,
/// `securityCode: 123`) is masked only when it carries a digit, so prose
/// that names a field (`Invalid token: The token has expired.`) survives.
fn mask_keyed_values(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let is_key_char = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let ends_value = |c: char| c.is_whitespace() || "&,;\"'<>()[]{}".contains(c);
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if !is_key_char(chars[i]) {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && is_key_char(chars[i]) {
            i += 1;
        }
        out.extend(&chars[start..i]);
        let key = chars[start..i]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();
        let leaf = if is_full_secret_key(&key) {
            Leaf::Secret
        } else if is_account_like_key(&key) {
            Leaf::Account
        } else {
            continue;
        };

        let mut j = i;
        if matches!(chars.get(j), Some('"' | '\'')) {
            j += 1;
        }
        let blank = |c: Option<&char>| matches!(c, Some(' ' | '\t'));
        let mut spaced = false;
        while blank(chars.get(j)) {
            j += 1;
            spaced = true;
        }
        let punct = matches!(chars.get(j), Some('=' | ':'));
        if punct {
            j += 1;
            while blank(chars.get(j)) {
                j += 1;
                spaced = true;
            }
        }
        if !spaced && !punct {
            continue;
        }
        let quoted = matches!(chars.get(j), Some('"' | '\''));
        let value_start = j + usize::from(quoted);
        let mut end = value_start;
        while end < chars.len() && !ends_value(chars[end]) {
            end += 1;
        }
        // A sentence's closing full stop is not part of the value.
        while end > value_start && chars[end - 1] == '.' {
            end -= 1;
        }
        let value: String = chars[value_start..end].iter().collect();
        if value.is_empty() || !(quoted || !spaced || value.chars().any(|c| c.is_ascii_digit())) {
            continue;
        }
        out.extend(&chars[i..value_start]);
        out.push_str(&match leaf {
            Leaf::Secret => "***".into(),
            Leaf::Account => mask_last4(&value),
        });
        i = end;
    }
    out
}

/// The shortest digit run treated as an account identifier.
///
/// A PAN is 13-19 digits and an ACH account number can be 12. Below that the
/// runs worth reading dominate — a status code, an amount, a year, a port —
/// and masking them would make a trace useless without making it safer.
const ACCOUNT_DIGITS: usize = 12;

/// Mask long digit runs wherever they appear in free text.
///
/// The key-based rules cannot reach these: a PAN echoed inside a validation
/// *message*, or nested in the array the API wraps messages in, is a string
/// whose key says nothing about its contents.
///
/// Only a run standing on its own counts. A run welded into a longer token —
/// a group of a hyphenated GUID, the digits of a hexadecimal trace id — is
/// part of an identifier a caller has to match character for character, and
/// no account number is written that way.
pub fn mask_digit_runs(text: &str) -> String {
    mask_runs_of(text, ACCOUNT_DIGITS, Run::Standalone)
}

/// The shortest digit run treated as an account identifier under a key that
/// says the text is about one.
///
/// No card, bank account or routing number is shorter, and the counts a
/// validation message states (`must be 9 digits`) are.
const KEYED_ACCOUNT_DIGITS: usize = 4;

/// Mask a leaf under a key that says it is an account identifier.
///
/// A leaf with no whitespace is the value itself, and every digit in it goes.
/// A sentence keeps the short counts that explain a rule and loses every
/// number long enough to be the account, counted across the separators it is
/// written with: `021 000 021` and `021-000-021` are nine digits, not three
/// runs of three.
fn mask_account_leaf(text: &str) -> String {
    if !text.contains(char::is_whitespace) {
        return mask_runs_of(text, 1, Run::Anywhere);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        // A number is digit runs joined by one separator each.
        let start = i;
        while i < chars.len()
            && (chars[i].is_ascii_digit()
                || (is_digit_separator(chars[i])
                    && chars.get(i + 1).is_some_and(char::is_ascii_digit)))
        {
            i += 1;
        }
        let span = &chars[start..i];
        let digits = span.iter().filter(|c| c.is_ascii_digit()).count();
        if digits < KEYED_ACCOUNT_DIGITS {
            out.extend(span);
            continue;
        }
        let mut seen = 0;
        for &c in span {
            if c.is_ascii_digit() {
                seen += 1;
                out.push(if seen > digits - 4 { c } else { '*' });
            } else {
                out.push(c);
            }
        }
    }
    out
}

/// A character a number may be grouped with: `021 000 021`, `021-000-021`.
fn is_digit_separator(c: char) -> bool {
    matches!(c, ' ' | '-' | '.' | '\u{a0}')
}

/// Which digit runs a pass may mask.
#[derive(Copy, Clone, PartialEq)]
enum Run {
    /// A run written as a number of its own, which is how an account number
    /// reaches a message.
    Standalone,
    /// Every run, whatever it is attached to.
    Anywhere,
}

fn mask_runs_of(text: &str, threshold: usize, scope: Run) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        if scope == Run::Standalone {
            if let Some(end) = grouped_pan_end(&chars, start) {
                let digits = chars[start..end]
                    .iter()
                    .filter(|c| c.is_ascii_digit())
                    .count();
                let mut seen = 0;
                for &c in &chars[start..end] {
                    if c.is_ascii_digit() {
                        seen += 1;
                        out.push(if seen > digits - 4 { c } else { '*' });
                    } else {
                        out.push(c);
                    }
                }
                i = end;
                continue;
            }
        }
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        let run: String = chars[start..i].iter().collect();
        let glued = (start > 0 && continues_a_token(chars[start - 1]))
            || chars.get(i).is_some_and(|&c| continues_a_token(c));
        if run.len() >= threshold && (scope == Run::Anywhere || !glued) {
            out.push_str(&mask_last4(&run));
        } else {
            out.push_str(&run);
        }
    }
    out
}

/// Where a card number written in groups (`4111 1111 1111 1111`,
/// `3782-822463-10005`) that starts at `start` ends, if one does.
///
/// Groups of three to six digits with one repeated separator, 13 to 19
/// digits in all: the shape card numbers are printed in. A GUID's eight- and
/// twelve-digit groups, a phone number and a date all fall outside it.
fn grouped_pan_end(chars: &[char], start: usize) -> Option<usize> {
    if start > 0 && continues_a_token(chars[start - 1]) {
        return None;
    }
    let group_end = |from: usize| {
        let mut j = from;
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1;
        }
        (3..=6).contains(&(j - from)).then_some(j)
    };
    let mut end = group_end(start)?;
    let sep = *chars.get(end).filter(|&&c| c == ' ' || c == '-')?;
    let mut groups = 1;
    while chars.get(end) == Some(&sep) {
        match chars.get(end + 1) {
            Some(c) if c.is_ascii_digit() => {}
            _ => break,
        }
        let Some(next) = group_end(end + 1) else {
            break;
        };
        end = next;
        groups += 1;
    }
    let digits = chars[start..end]
        .iter()
        .filter(|c| c.is_ascii_digit())
        .count();
    let welded = chars
        .get(end)
        .is_some_and(|&c| c.is_ascii_digit() || continues_a_token(c));
    (groups >= 2 && (13..=19).contains(&digits) && !welded).then_some(end)
}

/// Characters that carry a token across a digit run, making the run a part of
/// something longer rather than a number in its own right.
fn continues_a_token(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '-' || c == '_'
}

/// Keys whose values must never appear in a trace in any form, masked or not.
///
/// Covers card verification values, which PCI-DSS forbids logging at all,
/// auth credentials, whose last four characters are still leaked entropy, and
/// government identifiers, which are short enough that no length rule sees
/// them and identifying enough that a masked tail is still a disclosure. An
/// exact list first, then conservative stems so a compound name nobody
/// enumerated — `merchantSecret`, `xAuthToken` — is still caught.
fn is_full_secret_key(lower_key: &str) -> bool {
    const EXACT: &[&str] = &[
        "securitycode",
        "cvv",
        "cvc",
        "cvv2",
        "cardverificationvalue",
        "clientsecret",
        "client_secret",
        "secret",
        "password",
        "passwd",
        "pwd",
        "token",
        "accesstoken",
        "access_token",
        "refreshtoken",
        "refresh_token",
        "idtoken",
        "id_token",
        "jwt",
        "bearer",
        "apikey",
        "api_key",
        "apisecret",
        "api_secret",
        "authorization",
        "auth",
        "privatekey",
        "private_key",
        "sessiontoken",
        "session_token",
        "taxid",
        "tax_id",
    ];
    if EXACT.contains(&field_name(lower_key)) {
        return true;
    }
    const STEMS: &[&str] = &[
        "secret",
        "password",
        "passwd",
        "token",
        "jwt",
        "apikey",
        "authorization",
        "credential",
        "privatekey",
    ];
    STEMS.iter().any(|stem| lower_key.contains(stem))
}

/// Keys naming a request rather than a party to it. A correlation id is the
/// handle support searches on and the caller holds no second copy of it, so a
/// masked one matches nothing and is not recoverable; its digits identify no
/// account.
fn is_request_identifier_key(lower_key: &str) -> bool {
    matches!(
        lower_key,
        "correlationid" | "correlation_id" | "traceid" | "trace_id"
    )
}

/// Keys carrying a full card or bank account identifier. PCI-DSS permits the
/// last four digits, which is what keeps a trace useful for support.
fn is_account_like_key(lower_key: &str) -> bool {
    matches!(
        field_name(lower_key),
        "cardnumber" | "accountnumber" | "pan" | "routingnumber"
    )
}

/// The field a key names. The API keys a field error by its path
/// (`transactionDetails.cardData.securityCode`, `$.cvv`, `products[0].pan`),
/// and it is the last segment that says what the value is.
fn field_name(lower_key: &str) -> &str {
    let last = lower_key
        .rsplit(['.', '$'])
        .find(|s| !s.is_empty())
        .unwrap_or(lower_key);
    last.split('[').next().unwrap_or(last)
}

/// Keep the last four characters and star the rest. Four characters or fewer
/// are removed outright, so a short value is not echoed whole.
fn mask_last4(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > 4 {
        std::iter::repeat_n('*', chars.len() - 4)
            .chain(chars[chars.len() - 4..].iter().copied())
            .collect()
    } else {
        "***".into()
    }
}

/// Redact a JSON body **without changing its shape**.
///
/// `redact_value` replaces a secret node outright, which is right for a trace:
/// nothing downstream reads it. The error path cannot afford that. The
/// envelope's `Errors` must stay a map of arrays of strings for it to
/// deserialize at all, and a collapsed node would fail that parse and drop
/// the whole body into the fallback arm — which echoes it. So here the
/// *leaves* are masked and every container survives.
///
/// This is the rule the digit-run heuristic cannot express. A CVV is three
/// digits, so nothing about its shape marks it as sensitive; only the field
/// name does, and the field name is gone once the envelope has been flattened
/// into prose.
pub fn redact_preserving_shape(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let lk = k.to_ascii_lowercase();
                    let rv = if is_full_secret_key(&lk) {
                        mask_leaves(v, Leaf::Secret)
                    } else if is_account_like_key(&lk) {
                        mask_leaves(v, Leaf::Account)
                    } else if is_request_identifier_key(&lk) {
                        v.clone()
                    } else {
                        redact_preserving_shape(v)
                    };
                    (k.clone(), rv)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact_preserving_shape).collect()),
        Value::String(s) => Value::String(mask_digit_runs(s)),
        other => other.clone(),
    }
}

/// How a leaf under a sensitive key is masked.
///
/// The two differ because the values differ. A secret may be any string, so
/// nothing short of removing the leaf is safe — `password: hunter2` survives
/// a digit rule untouched. An account identifier is digits, and the same key
/// carries both the value itself and *sentences about* it, so masking the
/// whole leaf would turn "4111111111111111 is not acceptable" into a row of
/// stars and lose the reason.
#[derive(Copy, Clone)]
enum Leaf {
    /// Removed outright. PCI-DSS forbids logging a verification value at all,
    /// and the last four characters of a secret are still leaked entropy.
    Secret,
    /// Every digit run long enough to be the account masked to its last
    /// four, and the prose left alone.
    Account,
}

/// Mask every leaf beneath `value`, whatever containers it is wrapped in.
///
/// The API wraps field messages in an array, so the sensitive value is a
/// string inside a list rather than the value of the key itself.
fn mask_leaves(value: &Value, leaf: Leaf) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(|v| mask_leaves(v, leaf)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), mask_leaves(v, leaf)))
                .collect(),
        ),
        Value::Null => Value::Null,
        Value::String(s) => Value::String(match leaf {
            Leaf::Secret => "***".into(),
            Leaf::Account => mask_account_leaf(s),
        }),
        // A numeric CVV or account number is the same value in a different
        // JSON type, and masking one shape but not the other is no rule.
        other => Value::String(match leaf {
            Leaf::Secret => "***".into(),
            Leaf::Account => mask_last4(&other.to_string()),
        }),
    }
}

fn redact_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let lk = k.to_ascii_lowercase();
                    let rv = if is_full_secret_key(&lk) {
                        Value::String("***".into())
                    } else if is_account_like_key(&lk) {
                        match v.as_str() {
                            Some(s) => Value::String(mask_last4(s)),
                            // Not a string: the API wraps validation messages
                            // in an array under the field name, and the key
                            // still says the digits in them are an account's.
                            None => mask_leaves(v, Leaf::Account),
                        }
                    } else {
                        redact_value(v)
                    };
                    (k.clone(), rv)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact_value).collect()),
        // Every string leaf, whatever its key. A message that quotes the
        // value that caused a failure can carry it under a key that says
        // nothing, such as `Cause`.
        Value::String(s) => Value::String(mask_digit_runs(s)),
        other => other.clone(),
    }
}

/// Redact `name=value` and `name: value` pairs line by line.
fn redact_pairs(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            if let Some((name, _)) = line.split_once(':') {
                if is_sensitive(name) {
                    return redact_one(name, ": ");
                }
            }
            if line.contains('=') {
                return line
                    .split('&')
                    .map(|pair| match pair.split_once('=') {
                        Some((name, _)) if is_sensitive(name) => redact_one(name, "="),
                        Some((name, value)) if is_account(name) => {
                            format!("{name}={}", mask_last4(value))
                        }
                        _ => pair.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join("&");
            }
            format!("<unstructured body suppressed ({} bytes)>", line.len())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_sensitive(name: &str) -> bool {
    is_full_secret_key(&name.trim().to_ascii_lowercase())
}

fn is_account(name: &str) -> bool {
    is_account_like_key(&name.trim().to_ascii_lowercase())
}

fn redact_one(name: &str, sep: &str) -> String {
    format!("{name}{sep}***")
}

/// A last pass over text that survived the shape rules: a bearer token can
/// appear inside a line that is otherwise unremarkable.
fn scrub_bearer(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = find_bearer(rest) {
        out.push_str(&rest[..at]);
        out.push_str("Bearer ***");
        let after = &rest[at + "Bearer ".len()..];
        let end = after
            .find(|c: char| c.is_whitespace() || c == '"' || c == ',')
            .unwrap_or(after.len());
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn find_bearer(text: &str) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    lower.find("bearer ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_pan_to_last_four() {
        let out = redact(r#"{"cardNumber":"4111111111111111"}"#);
        assert!(!out.contains("4111111111111111"));
        assert!(out.contains("1111"));
    }

    /// A tax id is nine digits, so no length rule reaches it, and it is worth
    /// as little in a trace as a verification value.
    #[test]
    fn removes_a_tax_id_entirely() {
        let out = redact(r#"{"taxId":"987654321"}"#);
        assert!(!out.contains("987654321"), "{out}");
        assert!(out.contains("taxId"), "{out}");
    }

    #[test]
    fn removes_security_code_entirely() {
        let out = redact(r#"{"securityCode":"123"}"#);
        assert!(!out.contains("123"));
    }

    #[test]
    fn masks_bearer_token() {
        let out = redact("authorization: Bearer abc.def.ghi");
        assert!(!out.contains("abc.def.ghi"));
    }

    #[test]
    fn masks_client_secret_in_form_body() {
        let out = redact("grant_type=client_credentials&client_secret=s3cr3t");
        assert!(!out.contains("s3cr3t"));
    }

    /// A trace of unrecognisable text is suppressed rather than echoed, so a
    /// plaintext token or an error page reflecting a secret cannot leak whole.
    #[test]
    fn unrecognisable_trace_text_is_suppressed_not_echoed() {
        let out = redact("a plaintext eyJhbGciOiJIUzI1NiJ9.body.sig blob");
        assert!(!out.contains("eyJhbGciOiJIUzI1NiJ9"), "{out}");
        assert!(out.contains("suppressed"), "{out}");
    }

    /// A PAN inside a *message* is still a PAN. An error body that echoes the
    /// value that caused the failure is the realistic way one reaches a log.
    #[test]
    fn masks_a_pan_inside_free_text() {
        let out = redact_message("cardNumber 4111111111111111 is not acceptable");
        assert!(!out.contains("4111111111111111"), "{out}");
        assert!(out.contains("1111"), "{out}");
        assert!(out.contains("is not acceptable"), "{out}");
    }

    /// The API nests validation messages in an array under the field name, so
    /// the key-based rule sees an array rather than a string and masks
    /// nothing.
    #[test]
    fn masks_a_pan_nested_in_an_array_value() {
        let out = redact(r#"{"Errors":{"cardNumber":["4111111111111111 is not acceptable"]}}"#);
        assert!(!out.contains("4111111111111111"), "{out}");
    }

    /// A short secret in prose is sensitive only by the key before it, so
    /// the message masks a value that follows a sensitive key name.
    #[test]
    fn masks_a_value_after_a_sensitive_key_in_free_text() {
        for (text, secret) in [
            ("cvv=123", "123"),
            ("securityCode: 123", "123"),
            ("client_secret=abc", "abc"),
            ("invalid request: cvv 987 rejected", "987"),
            ("routingNumber=021000021", "02100"),
            ("accountNumber: 12345678 is closed", "1234"),
        ] {
            let out = redact_message(text);
            assert!(!out.contains(secret), "{text} -> {out}");
        }
        assert_eq!(redact_message("cvv=123"), "cvv=***");
        assert_eq!(redact_message("securityCode: 123"), "securityCode: ***");
    }

    /// Prose that names a sensitive field without a value beside it survives.
    #[test]
    fn leaves_prose_around_sensitive_words_alone() {
        for text in [
            "The card was declined: insufficient funds (code 51).",
            "Invalid token: The token has expired.",
            "The password is wrong.",
            "A security code is required.",
            "cvv is required",
        ] {
            assert_eq!(redact_message(text), text);
        }
    }

    /// Masking every digit run would destroy the messages worth reading. An
    /// amount, a year, a port and a status code must all survive.
    #[test]
    fn leaves_short_digit_runs_alone() {
        let out = redact_message("status 400 amount 1234.56 expiry 12/2032 port 8080");
        assert!(out.contains("400"), "{out}");
        assert!(out.contains("1234.56"), "{out}");
        assert!(out.contains("2032"), "{out}");
        assert!(out.contains("8080"), "{out}");
    }

    /// An ACH account number is shorter than a PAN but still long enough to
    /// be identifying.
    #[test]
    fn masks_a_bare_account_number_in_free_text() {
        let out = redact_message("accountNumber 000123456789 rejected");
        assert!(!out.contains("000123456789"), "{out}");
    }

    #[test]
    fn masks_ach_account_number() {
        let out = redact(r#"{"accountNumber":"000123456789"}"#);
        assert!(!out.contains("000123456789"));
    }

    /// A digit run that is part of a longer identifier is not an account
    /// number: an identifier the API names has to come back byte for byte or
    /// it matches nothing the caller sent.
    #[test]
    fn a_digit_run_glued_to_a_larger_identifier_is_left_alone() {
        let id = "11111111-2222-3333-4444-555555555555";
        let out = redact_message(&format!("Customer with ID {id} does not exist"));
        assert!(out.contains(id), "{out}");
    }

    #[test]
    fn a_digit_run_inside_a_hexadecimal_token_is_left_alone() {
        let trace = "00-4844bd4e873a1616096628377ad30ab2-7502ae290882b49f-01";
        let out = redact_message(&format!("request {trace} failed"));
        assert!(out.contains(trace), "{out}");
    }

    /// A card number is as often written in groups as unbroken, and each group
    /// alone is too short for the length rule.
    #[test]
    fn masks_a_grouped_pan_inside_free_text() {
        for pan in [
            "4111 1111 1111 1111",
            "4111-1111-1111-1111",
            "3782-822463-10005",
        ] {
            let out = redact_message(&format!("Card {pan} is invalid"));
            assert!(!out.contains(pan), "{out}");
            assert!(
                out.ends_with(&format!("{} is invalid", &pan[pan.len() - 4..])),
                "{out}"
            );
        }
    }

    /// Grouped digits short of a card number are a phone number or a date.
    #[test]
    fn leaves_grouped_digits_short_of_a_pan_alone() {
        let out = redact_message("call 415-555-2309 on 2032-12-01");
        assert!(out.contains("415-555-2309"), "{out}");
        assert!(out.contains("2032-12-01"), "{out}");
    }

    /// The API keys a field error by its path, so the field is the last
    /// segment of the key rather than the whole of it.
    #[test]
    fn a_secret_under_a_dotted_path_key_is_removed() {
        for key in [
            "transactionDetails.cardData.paymentMethodDetails.securityCode",
            "$.cardData.securityCode",
            "products[0].cvv",
        ] {
            let body = format!(r#"{{"Errors":{{"{key}":["837 is invalid"]}}}}"#);
            let traced = redact(&body);
            assert!(!traced.contains("837"), "{traced}");
            let shaped = redact_preserving_shape(&serde_json::from_str(&body).unwrap());
            assert!(!shaped.to_string().contains("837"), "{shaped}");
        }
    }

    #[test]
    fn an_account_number_under_a_dotted_path_key_is_masked() {
        let body = r#"{"achDetails.accountNumber":"123456789"}"#;
        let traced = redact(body);
        assert!(!traced.contains("123456789"), "{traced}");
        let shaped = redact_preserving_shape(&serde_json::from_str(body).unwrap());
        assert!(!shaped.to_string().contains("123456789"), "{shaped}");
    }

    /// Under a key that says the value is an account number, no shape rule
    /// applies: the digits go, whatever they are attached to.
    #[test]
    fn an_account_keyed_value_is_masked_even_when_glued_to_other_text() {
        let out = redact(r#"{"accountNumber":"acct-000123456789"}"#);
        assert!(!out.contains("000123456789"), "{out}");
    }

    /// A validation message under an account key keeps the count it states,
    /// in the trace and in the error envelope alike.
    #[test]
    fn a_rule_under_an_account_key_keeps_its_digit_count() {
        let body = r#"{"Errors":{"routingNumber":["Routing number must be 9 digits."]}}"#;
        let traced = redact(body);
        assert!(traced.contains("must be 9 digits."), "{traced}");
        let shaped = redact_preserving_shape(&serde_json::from_str(body).unwrap());
        assert!(shaped.to_string().contains("must be 9 digits."), "{shaped}");
    }

    /// A message under an account key that quotes the number loses it, down
    /// to the shortest account number and in whatever grouping it is written.
    #[test]
    fn a_message_under_an_account_key_that_quotes_the_number_is_masked() {
        for (key, message, secret) in [
            (
                "routingNumber",
                "021000021 is not a routing number",
                "021000021",
            ),
            ("accountNumber", "Account 98765 is closed", "98765"),
            (
                "cardNumber",
                "4111 1111 1111 1111 is not valid",
                "4111 1111 1111",
            ),
            (
                "cardNumber",
                "cardNumber=4111111111111111 rejected",
                "411111111111",
            ),
            (
                "routingNumber",
                "021 000 021 is not a routing number",
                "021 000",
            ),
            ("routingNumber", "021-000-021 rejected", "021-000"),
            ("accountNumber", "Account 123 456 789 is closed", "123 456"),
        ] {
            let body = format!(r#"{{"Errors":{{"{key}":["{message}"]}}}}"#);
            let traced = redact(&body);
            assert!(!traced.contains(secret), "{traced}");
            let shaped = redact_preserving_shape(&serde_json::from_str(&body).unwrap());
            assert!(!shaped.to_string().contains(secret), "{shaped}");
        }
    }

    /// A bare value under an account key loses every digit, however short.
    #[test]
    fn a_short_bare_value_under_an_account_key_is_masked() {
        let body = r#"{"Errors":{"routingNumber":["123"]}}"#;
        let shaped = redact_preserving_shape(&serde_json::from_str(body).unwrap());
        assert!(!shaped.to_string().contains("123"), "{shaped}");
    }
}
