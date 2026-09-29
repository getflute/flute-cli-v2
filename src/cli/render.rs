//! Shared table-rendering machinery, driven by per-resource descriptors.
//!
//! A response carries as much in its nested objects and arrays as in its
//! top-level scalars — `customers get` returns `billingAddress`, and
//! `transactions create` returns the four containers its declared response
//! example lists — so a descriptor reaches into them rather than printing the
//! top level in map order.
//!
//! This is deliberately **not** a universal JSON-to-table guesser. The layout
//! is shared; a resource declares the shape its own fields make and how they
//! are read, and anything the descriptor does not name still prints — a beta
//! API adds fields, and a dropped one is how a user misses a new decline
//! reason.

use crate::cli::output::{Envelope, OutputFormat};
use anyhow::Result;
use rust_decimal::Decimal;
use serde_json::Value;
use std::str::FromStr;

/// A value the response does not carry, and an explicit null.
pub(crate) const MISSING: &str = "\u{2014}";

/// How one list column gets its text.
pub enum Cell {
    /// A JSON pointer into the item.
    Path(&'static str),
    /// What a pointer cannot express — a first and last name joined, or a
    /// masked number taken from whichever instrument object is present.
    Derived(fn(&Value) -> Option<String>),
}

pub struct Column {
    pub header: &'static str,
    pub width: usize,
    pub cell: Cell,
}

/// What one resource wants said about it.
pub struct Resource {
    /// The envelope's `object` name for one resource.
    pub object: &'static str,
    /// The envelope's `object` name for a collection of them, such as
    /// `customer_list`. The name is part of the output contract.
    pub object_list: &'static str,
    /// JSON pointer to the identifier `quiet` mode prints.
    pub id: &'static str,
    /// The detail view's **fixed shape**, in the order it is worth reading:
    /// each pointer holds a row whether or not the response carries it, so
    /// the same questions are answered about every record of the resource.
    /// It is not a whitelist — everything not named here still renders, after
    /// these. A pointer may carry a `/[]` step — the surface matrix's
    /// spelling — which ranks every element of that array; those name
    /// elements rather than rows, and appear only for the ones that exist.
    pub detail: &'static [&'static str],
    /// List-view columns, in order.
    pub columns: &'static [Column],
    /// The pointers whose value is money. They render to two decimal places
    /// in every table: the API sends whatever its serialiser produced, and
    /// `10.5` for an amount is a wrong answer about money. Which fields are
    /// money is a fact about the resource, not something a renderer can read
    /// off a field name — a rate, a count and a battery level all keep the
    /// digits the API sent.
    pub amounts: &'static [&'static str],
    /// The pointers whose boolean reads as `yes`/`no`. The question the field
    /// answers is what a reader is asking, and `true` is the storage rather
    /// than the answer.
    pub yes_no: &'static [&'static str],
}

/// One `label: value` line per leaf: declared pointers first in declared
/// order, then everything else.
pub fn detail_table(resource: &Resource, data: &Value) -> String {
    labelled(&detail_rows(resource, data)).join("\n")
}

/// `label:` padded so every value starts in the same column, one space past
/// the widest label. Returns the lines, so a caller composing sections can
/// align them all against each other and then break them up.
pub(crate) fn labelled(rows: &[(String, String)]) -> Vec<String> {
    let width = rows
        .iter()
        .map(|(label, _)| label.chars().count() + 1)
        .max()
        .unwrap_or(0);
    rows.iter()
        .map(|(label, value)| {
            let label = format!("{label}:");
            format!("{label:<width$} {value}")
        })
        .collect()
}

/// A header, a rule, and one fixed-width row per item.
pub fn list_table(resource: &Resource, items: &[Value]) -> String {
    let header = resource
        .columns
        .iter()
        .map(|c| pad(c.header, c.width))
        .collect::<Vec<_>>()
        .join("  ");
    let rule = "-".repeat(header.chars().count());
    let mut lines = vec![header, rule];
    for item in items {
        lines.push(
            resource
                .columns
                .iter()
                .map(|c| fit(&cell_text(resource, c, item), c.width))
                .collect::<Vec<_>>()
                .join("  "),
        );
    }
    lines.join("\n")
}

/// The identifier `quiet` mode prints.
pub fn id_of(resource: &Resource, data: &Value) -> Option<String> {
    data.pointer(resource.id)
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Render one resource in whichever mode the invocation asked for.
///
/// `table` is human-ordered and complete, `json` is the API's own data inside
/// the envelope with nothing reordered or substituted, and `quiet` is an
/// identifier and nothing else.
pub fn one(
    ctx: &crate::Ctx,
    resource: &Resource,
    data: &Value,
    correlation_id: Option<String>,
) -> Result<()> {
    match ctx.output {
        OutputFormat::Json => {
            let env = Envelope::new(
                resource.object,
                data,
                &ctx.profile.name,
                correlation_id,
                None,
            );
            println!("{}", serde_json::to_string_pretty(&env)?);
        }
        // A record whose schema declares no identifier prints nothing at all:
        // a blank line is a value a script would read as one.
        OutputFormat::Quiet => {
            if let Some(id) = id_of(resource, data).filter(|id| !id.is_empty()) {
                println!("{id}");
            }
        }
        OutputFormat::Table => println!("{}", detail_table(resource, data)),
    }
    Ok(())
}

/// The date half of a timestamp, for a list column too narrow for the whole
/// thing. Truncating it with the shared ellipsis rule would spend a character
/// saying the value was cut, which for a date is the least useful ten
/// characters available.
pub fn date_only(item: &Value, pointer: &str) -> Option<String> {
    let raw = item.pointer(pointer)?.as_str()?;
    Some(raw.chars().take(10).collect())
}

/// Print a paginated collection.
///
/// `data` is the collection itself, never a transport wrapper: the API's
/// `pageInfo` moves to `meta.page_info`, which is what makes the wrapper
/// redundant. `page_info` is `None` under `--all`, where the data spans every
/// page and no single `pageInfo` describes it.
pub fn page(
    ctx: &crate::Ctx,
    resource: &Resource,
    items: &[Value],
    page_info: Option<Value>,
    correlation_id: Option<String>,
) -> Result<()> {
    match ctx.output {
        OutputFormat::Json => {
            let env = Envelope::new(
                resource.object_list,
                items,
                &ctx.profile.name,
                correlation_id,
                page_info,
            );
            println!("{}", serde_json::to_string_pretty(&env)?);
        }
        OutputFormat::Quiet => {
            for item in items {
                if let Some(id) = id_of(resource, item) {
                    println!("{id}");
                }
            }
        }
        OutputFormat::Table => {
            println!("{}", list_table(resource, items));
            if let Some(hint) = page_info
                .as_ref()
                .and_then(|p| more_pages_hint(items.len(), p))
            {
                eprintln!("{hint}");
            }
        }
    }
    Ok(())
}

/// The stderr line a `table` page prints when the server reports more pages.
///
/// `json` carries `meta.page_info` instead, and `quiet` output is chained into
/// other commands, so only the table view has no other way to say it.
fn more_pages_hint(shown: usize, page_info: &Value) -> Option<String> {
    if page_info.get("hasMore").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let next = page_info
        .get("pageIndex")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        + 1;
    let of = page_info
        .get("totalItems")
        .and_then(Value::as_u64)
        .map(|total| format!(" of {total}"))
        .unwrap_or_default();
    Some(format!(
        "Showing {shown}{of}. More pages: pass --page-index {next} for the next one, or --all for every page."
    ))
}

/// Confirm a write whose success carries **no body**.
///
/// Some operations answer with nothing, so there is no resource to render.
/// The identity can only come from the request, and the verb is what the CLI
/// knows happened — the same shape as `ping`, whose `reachable: true` is
/// likewise the CLI's own statement rather than the API's.
///
/// An **empty** `id` is a singleton resource — a settings document lives at a
/// fixed path and has none. It is then omitted rather than reported as an
/// empty string: `quiet` prints nothing, and the envelope carries the verb
/// alone. An invented empty id key is worse than no key, because a consumer
/// reading it gets a value that looks like an identifier.
///
/// `line` is the whole sentence the caller wants a reader to see, because
/// only the caller knows what its verb does to its resource: a delete removes
/// an instrument and deletes a customer, and the envelope's key cannot carry
/// that difference.
pub fn confirmed(
    ctx: &crate::Ctx,
    resource: &Resource,
    id: &str,
    verb: &'static str,
    line: &str,
    correlation_id: Option<String>,
) -> Result<()> {
    emit_confirmation(
        ctx,
        resource,
        id,
        confirmation_data(resource, id, verb),
        line,
        correlation_id,
    )
}

/// A delete or revoke whose target the server does not have: still exit 0,
/// because a retry after a timed-out delete lands here and the caller's intent
/// holds, but the envelope reports `<verb>: false, found: false` so a mistyped
/// id is not reported as a removal.
pub fn absent(
    ctx: &crate::Ctx,
    resource: &Resource,
    id: &str,
    verb: &'static str,
    line: &str,
) -> Result<()> {
    let mut data = confirmation_data(resource, id, verb);
    data[verb] = Value::Bool(false);
    data["found"] = Value::Bool(false);
    emit_confirmation(ctx, resource, id, data, line, None)
}

fn emit_confirmation(
    ctx: &crate::Ctx,
    resource: &Resource,
    id: &str,
    data: Value,
    line: &str,
    correlation_id: Option<String>,
) -> Result<()> {
    match ctx.output {
        OutputFormat::Json => {
            let env = Envelope::new(
                resource.object,
                data,
                &ctx.profile.name,
                correlation_id,
                None,
            );
            println!("{}", serde_json::to_string_pretty(&env)?);
        }
        OutputFormat::Quiet => {
            if !id.is_empty() {
                println!("{id}");
            }
        }
        OutputFormat::Table => println!("{line}"),
    }
    Ok(())
}

fn confirmation_data(resource: &Resource, id: &str, verb: &'static str) -> Value {
    let mut map = serde_json::Map::new();
    if !id.is_empty() {
        map.insert(id_key(resource).to_string(), Value::String(id.to_string()));
    }
    map.insert(verb.to_string(), Value::Bool(true));
    Value::Object(map)
}

/// The last segment of the descriptor's id pointer, which is the wire key
/// name — so a confirmation cannot name a key the resource does not use.
fn id_key(resource: &Resource) -> &'static str {
    resource.id.rsplit('/').next().unwrap_or(resource.id)
}

fn cell_text(resource: &Resource, column: &Column, item: &Value) -> String {
    match column.cell {
        Cell::Path(p) => value_at(resource, item, p),
        Cell::Derived(f) => f(item).unwrap_or_else(|| MISSING.to_string()),
    }
}

/// The text one pointer contributes to a table, formatted as the resource
/// says its values are read.
pub(crate) fn value_at(resource: &Resource, data: &Value, pointer: &str) -> String {
    let text = data
        .pointer(pointer)
        .map_or_else(|| MISSING.to_string(), scalar);
    formatted(resource, pointer, text)
}

/// One leaf of a response: the pointer pattern that addresses it, the label
/// it prints under, and its text.
struct Leaf {
    pattern: String,
    label: String,
    value: String,
}

fn leaves_of(resource: &Resource, data: &Value) -> Vec<Leaf> {
    leaves(data)
        .into_iter()
        .map(|(label, value)| {
            let pattern = canonical(&label);
            let value = formatted(resource, &pattern, value);
            Leaf {
                pattern,
                label,
                value,
            }
        })
        .collect()
}

/// The declared pointers first, in declared order, then everything else.
///
/// The flattening order decides among leaves of one pattern and among the
/// undeclared ones — alphabetical, because `serde_json::Map` is a `BTreeMap`
/// in this build. Deterministic either way, which is what a snapshot needs.
fn detail_rows(resource: &Resource, data: &Value) -> Vec<(String, String)> {
    let leaves = leaves_of(resource, data);
    let mut rows = declared_rows(resource, &leaves);
    rows.extend(
        leaves
            .iter()
            .filter(|leaf| !resource.detail.contains(&leaf.pattern.as_str()))
            // A null container's declared fields already print as dashes, so
            // the container itself would say the same thing twice.
            .filter(|leaf| {
                let parent = format!("{}/", leaf.pattern);
                leaf.value != MISSING || !resource.detail.iter().any(|d| d.starts_with(&parent))
            })
            .map(|leaf| (leaf.label.clone(), leaf.value.clone())),
    );
    rows
}

/// The declared set, in declared order, as rows.
///
/// A declared pointer the response does not carry still holds its row, with a
/// dash: the declared set is the shape of the view, and a field that vanishes
/// from the table reads as one the resource never had. A pattern with a `/[]`
/// step is the exception, because it addresses elements rather than a row —
/// a `parts[].name` row would report a part that does not exist.
///
/// Consecutive patterns that step into the same array are one group, read
/// element by element: each part's `id` then `name`, then the next part's,
/// rather than every part's `id` followed by every part's `name`.
fn declared_rows(resource: &Resource, leaves: &[Leaf]) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mut patterns = resource.detail.iter().peekable();
    while let Some(pattern) = patterns.next() {
        let Some(array) = array_prefix(pattern) else {
            match leaves.iter().find(|leaf| leaf.pattern == *pattern) {
                Some(_) => rows.extend(matching(leaves, pattern)),
                None => rows.push((label_of(pattern), MISSING.to_string())),
            }
            continue;
        };
        let mut group = vec![*pattern];
        while let Some(next) = patterns.next_if(|p| array_prefix(p) == Some(array)) {
            group.push(*next);
        }
        let mut elements: Vec<(usize, (String, String))> = group
            .iter()
            .flat_map(|p| matching(leaves, p))
            .map(|row| (element_index(&row.0), row))
            .collect();
        elements.sort_by_key(|(index, _)| *index);
        rows.extend(elements.into_iter().map(|(_, row)| row));
    }
    rows
}

/// The rows one declared pattern matches, in response order.
fn matching(leaves: &[Leaf], pattern: &str) -> Vec<(String, String)> {
    leaves
        .iter()
        .filter(|leaf| leaf.pattern == pattern)
        .map(|leaf| (leaf.label.clone(), leaf.value.clone()))
        .collect()
}

/// The pointer up to and including its first `/[]` step, if it has one.
fn array_prefix(pattern: &str) -> Option<&str> {
    pattern.find("/[]").map(|at| &pattern[..at + 3])
}

/// The first element index in a rendered label: `parts[2].name` is `2`.
fn element_index(label: &str) -> usize {
    label
        .split_once('[')
        .and_then(|(_, rest)| rest.split_once(']'))
        .and_then(|(index, _)| index.parse().ok())
        .unwrap_or(0)
}

/// A descriptor pointer as its rendered label: `/billingAddress/city` becomes
/// `billingAddress.city`.
fn label_of(pointer: &str) -> String {
    pointer.trim_start_matches('/').replace('/', ".")
}

/// One value as the resource reads it: money to two places, a declared flag
/// as `yes`/`no`, anything else as the API sent it.
fn formatted(resource: &Resource, pattern: &str, text: String) -> String {
    if resource.amounts.contains(&pattern) {
        return two_decimals(&text).unwrap_or(text);
    }
    if resource.yes_no.contains(&pattern) {
        return match text.as_str() {
            "true" => "yes".to_string(),
            "false" => "no".to_string(),
            _ => text,
        };
    }
    text
}

/// An amount to two decimal places, through `Decimal` because money never
/// touches `f64`. A value that is not a number — a dash, a null, a string the
/// schema did not promise — is left exactly as it came.
fn two_decimals(text: &str) -> Option<String> {
    let mut amount = Decimal::from_str(text).ok()?.round_dp(2);
    amount.rescale(2);
    Some(amount.to_string())
}

/// A rendered label back to its descriptor pointer: `cards[0].cardMask`
/// becomes `/cards/[]/cardMask`, so one pattern ranks every element.
fn canonical(label: &str) -> String {
    let mut out = String::with_capacity(label.len() + 1);
    for segment in label.split('.') {
        out.push('/');
        match segment.split_once('[') {
            Some((name, _index)) => {
                out.push_str(name);
                out.push_str("/[]");
            }
            None => out.push_str(segment),
        }
    }
    out
}

/// Flatten a response into `(dotted label, rendered value)` pairs.
///
/// An array of objects is indexed, because each element genuinely needs rows
/// of its own. An array of scalars is **one joined row** — six lines for six
/// SEC codes is noise — so it is addressed by its own pointer rather than by
/// a `/[]` step.
fn leaves(data: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    walk(data, "", &mut out);
    out
}

fn walk(node: &Value, prefix: &str, out: &mut Vec<(String, String)>) {
    match node {
        Value::Object(map) if !map.is_empty() => {
            for (key, value) in map {
                let label = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                walk(value, &label, out);
            }
        }
        Value::Array(items) if !items.is_empty() => {
            if items.iter().any(|i| i.is_object() || i.is_array()) {
                for (i, item) in items.iter().enumerate() {
                    walk(item, &format!("{prefix}[{i}]"), out);
                }
            } else {
                let joined = items.iter().map(scalar).collect::<Vec<_>>().join(", ");
                out.push((prefix.to_string(), joined));
            }
        }
        other => out.push((prefix.to_string(), scalar(other))),
    }
}

/// One value as text. A number keeps its exact digits: `arbitrary_precision`
/// means `10.50` arrives and leaves as `10.50`, and an amount rendered as
/// `10.5` is a wrong answer about money.
fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => MISSING.to_string(),
        Value::Object(_) => "{}".to_string(),
        Value::Array(_) => "[]".to_string(),
        other => other.to_string(),
    }
}

fn pad(s: &str, width: usize) -> String {
    format!("{s:<width$}")
}

/// Pad or truncate to `width`, marking a truncation. List cells only: a
/// detail view prints a value whole, because truncating an id or a decline
/// reason destroys the thing that was asked for.
fn fit(s: &str, width: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= width {
        pad(s, width)
    } else if width >= 1 {
        let kept: String = chars.into_iter().take(width - 1).collect();
        format!("{kept}\u{2026}")
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A synthetic resource, so the machinery is tested apart from any one
    /// group's field choices.
    static WIDGET: Resource = Resource {
        object: "widget",
        object_list: "widget_list",
        id: "/widgetId",
        detail: &[
            "/widgetId",
            "/status",
            "/billingAddress/city",
            "/parts/[]/name",
        ],
        columns: &[
            Column {
                header: "ID",
                width: 8,
                cell: Cell::Path("/widgetId"),
            },
            Column {
                header: "OWNER",
                width: 12,
                cell: Cell::Derived(|v| {
                    let first = v.pointer("/firstName")?.as_str()?;
                    let last = v.pointer("/lastName")?.as_str()?;
                    Some(format!("{first} {last}"))
                }),
            },
            Column {
                header: "PRICE",
                width: 10,
                cell: Cell::Path("/price"),
            },
        ],
        amounts: &["/price"],
        yes_no: &["/isDefault"],
    };

    fn lines(s: &str) -> Vec<String> {
        s.lines().map(|l| l.trim_end().to_string()).collect()
    }

    fn labels(s: &str) -> Vec<String> {
        s.lines()
            .map(|l| {
                l.split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(':')
                    .to_string()
            })
            .collect()
    }

    /// The column the value starts in, found from the label's own colon so a
    /// colon inside a timestamp cannot be mistaken for it.
    fn value_column(line: &str) -> usize {
        let colon = line.find(':').expect("every detail row carries a colon");
        colon + 1 + line[colon + 1..].find(|c: char| c != ' ').unwrap_or(0)
    }

    /// Declared order governs, not the map's own key order. `status` sorts
    /// after `widgetId` alphabetically and must print before it.
    #[test]
    fn detail_table_prints_declared_fields_in_declared_order() {
        let v = json!({"status": "Active", "widgetId": "w_1"});
        let labels = labels(&detail_table(&WIDGET, &v));
        assert_eq!(&labels[..2], ["widgetId", "status"]);
    }

    /// A nested object's fields print, not just the top-level scalars.
    #[test]
    fn detail_table_reaches_into_a_nested_object() {
        let v =
            json!({"widgetId": "w_1", "billingAddress": {"city": "Austin", "countryCode": "US"}});
        let out = detail_table(&WIDGET, &v);
        assert!(out.contains("billingAddress.city"), "{out}");
        assert!(out.contains("Austin"), "{out}");
        // Undeclared, and still present.
        assert!(out.contains("billingAddress.countryCode"), "{out}");
    }

    /// A beta API adds fields. Dropping one silently is how a user misses a
    /// new decline reason, so an unnamed leaf prints after the named ones.
    #[test]
    fn detail_table_keeps_a_field_no_descriptor_names() {
        let v = json!({"widgetId": "w_1", "brandNewField": "surprise"});
        let out = detail_table(&WIDGET, &v);
        let labels = labels(&out);
        assert_eq!(labels.first().unwrap(), "widgetId");
        assert_eq!(labels.last().unwrap(), "brandNewField");
        assert!(out.contains("surprise"), "{out}");
    }

    /// A declared field is part of the view's shape, so it holds its row
    /// whether the response omits it or sends an explicit null. A field the
    /// descriptor does not name appears only when it is there.
    #[test]
    fn detail_table_dashes_a_declared_field_that_is_missing_or_null() {
        let v = json!({"widgetId": "w_1", "receipt": null});
        let out = detail_table(&WIDGET, &v);
        assert!(
            out.lines()
                .any(|l| l.starts_with("status:") && l.ends_with('—')),
            "{out}"
        );
        assert!(
            out.lines()
                .any(|l| l.starts_with("receipt:") && l.ends_with('—')),
            "{out}"
        );
        assert!(!out.contains("brandNewField"), "{out}");
    }

    /// A null container whose fields the view declares is already shown by
    /// their dashed rows, so it gets no row of its own.
    #[test]
    fn detail_table_prints_no_row_for_a_null_parent_of_declared_fields() {
        let v = json!({"widgetId": "w_1", "billingAddress": null, "parts": null});
        let labels = labels(&detail_table(&WIDGET, &v));
        assert!(
            labels.contains(&"billingAddress.city".to_string()),
            "{labels:?}"
        );
        assert!(
            !labels.contains(&"billingAddress".to_string()),
            "{labels:?}"
        );
        assert!(!labels.contains(&"parts".to_string()), "{labels:?}");
    }

    /// A `/[]` pattern names an element, not a row, so an array the response
    /// does not carry has nothing to dash — a row for `parts[].name` would
    /// report a part that does not exist.
    #[test]
    fn detail_table_dashes_no_row_for_an_array_the_response_omits() {
        let out = detail_table(&WIDGET, &json!({"widgetId": "w_1"}));
        assert!(!out.contains("parts"), "{out}");
    }

    /// An array of objects earns its own rows, indexed; an array of scalars is
    /// one joined row.
    #[test]
    fn detail_table_indexes_object_arrays_and_joins_scalar_arrays() {
        let v = json!({
            "widgetId": "w_1",
            "parts": [{"name": "left"}, {"name": "right"}],
            "codes": ["WEB", "PPD"]});
        let out = detail_table(&WIDGET, &v);
        assert!(out.contains("parts[0].name"), "{out}");
        assert!(out.contains("parts[1].name"), "{out}");
        assert!(out.contains("codes"), "{out}");
        assert!(out.contains("WEB, PPD"), "{out}");
    }

    /// One `/[]` pattern ranks every element of the array, so declared order
    /// survives indexing.
    #[test]
    fn a_bracket_pattern_ranks_every_element_of_an_array() {
        let v = json!({"parts": [{"name": "left"}], "aaaFirstAlphabetically": 1});
        let labels = labels(&detail_table(&WIDGET, &v));
        let declared = labels.iter().position(|l| l == "parts[0].name").unwrap();
        let undeclared = labels
            .iter()
            .position(|l| l == "aaaFirstAlphabetically")
            .unwrap();
        assert!(declared < undeclared, "{labels:?}");
    }

    /// Consecutive patterns into one array read element by element, so each
    /// element's fields sit together however many elements there are.
    #[test]
    fn detail_table_groups_an_arrays_fields_by_element() {
        static PARTS: Resource = Resource {
            object: "widget",
            object_list: "widget_list",
            id: "/widgetId",
            detail: &["/widgetId", "/parts/[]/id", "/parts/[]/name", "/status"],
            columns: &[],
            amounts: &[],
            yes_no: &[],
        };
        let v = json!({
            "widgetId": "w_1",
            "status": "ok",
            "parts": [{"id": "p_0", "name": "zero"}, {"id": "p_1", "name": "one"}],
        });
        let out = detail_table(&PARTS, &v);
        let labels: Vec<&str> = out
            .lines()
            .map(|l| l.split(':').next().unwrap().trim())
            .collect();
        assert_eq!(
            labels,
            [
                "widgetId",
                "parts[0].id",
                "parts[0].name",
                "parts[1].id",
                "parts[1].name",
                "status"
            ],
            "{out}"
        );
    }

    /// An empty container is not nothing. Rendering it as blank would read as
    /// a field the API did not send.
    #[test]
    fn detail_table_shows_empty_containers_rather_than_dropping_them() {
        let v = json!({"widgetId": "w_1", "parts": [], "meta": {}});
        let out = detail_table(&WIDGET, &v);
        assert!(out.contains("parts"), "{out}");
        assert!(out.contains("[]"), "{out}");
        assert!(out.contains("meta"), "{out}");
        assert!(out.contains("{}"), "{out}");
    }

    /// A detail view is authoritative: truncating an id or a decline reason
    /// destroys the one thing that was asked for.
    #[test]
    fn detail_table_never_truncates_a_value() {
        let long = "d".repeat(200);
        let v = json!({"widgetId": long});
        assert!(detail_table(&WIDGET, &v).contains(&long));
    }

    /// Labels align to the widest label actually printed, so a table of short
    /// keys is not padded to a width chosen for some other resource.
    #[test]
    fn detail_rows_are_a_label_a_colon_and_a_value_one_space_past_the_widest() {
        let v = json!({
            "widgetId": "w_1",
            "status": "Active",
            "billingAddress": {"city": "Austin"}});
        let out = detail_table(&WIDGET, &v);
        assert!(out.starts_with("widgetId:"), "{out}");
        let columns: Vec<usize> = out.lines().map(value_column).collect();
        assert!(columns.iter().all(|c| *c == columns[0]), "{out}");
        assert_eq!(columns[0], "billingAddress.city:".len() + 1, "{out}");
    }

    /// Money prints to two places wherever it appears, because the API sends
    /// whatever its serialiser produced and a column that prints a different
    /// number of places for the same currency invites a misread. A number
    /// that is not money keeps the digits the API sent.
    #[test]
    fn a_declared_amount_renders_with_two_decimals_and_other_numbers_do_not() {
        let v: Value =
            serde_json::from_str(r#"{"widgetId":"w_1","price":10.5,"quantity":3}"#).unwrap();
        let out = detail_table(&WIDGET, &v);
        assert!(
            out.lines()
                .any(|l| l.starts_with("price:") && l.ends_with("10.50")),
            "{out}"
        );
        assert!(
            out.lines()
                .any(|l| l.starts_with("quantity:") && l.ends_with('3')),
            "{out}"
        );
    }

    #[test]
    fn a_declared_amount_renders_with_two_decimals_in_a_list_cell() {
        let items = vec![serde_json::from_str(r#"{"widgetId":"w_1","price":100}"#).unwrap()];
        assert!(lines(&list_table(&WIDGET, &items))[2].contains("100.00"));
    }

    /// A flag reads as an answer to a question — is this the default one? —
    /// and `true` is the storage, not the answer.
    #[test]
    fn a_declared_boolean_renders_yes_or_no() {
        let out = detail_table(&WIDGET, &json!({"widgetId": "w_1", "isDefault": true}));
        assert!(
            out.lines()
                .any(|l| l.starts_with("isDefault:") && l.ends_with("yes")),
            "{out}"
        );
        let out = detail_table(&WIDGET, &json!({"widgetId": "w_1", "isDefault": false}));
        assert!(
            out.lines()
                .any(|l| l.starts_with("isDefault:") && l.ends_with("no")),
            "{out}"
        );
    }

    /// `arbitrary_precision` is what makes this possible, and an amount that
    /// renders as `10.5` is a wrong answer about money.
    #[test]
    fn detail_table_renders_an_amount_with_its_exact_digits() {
        let v: Value =
            serde_json::from_str(r#"{"widgetId":"w_1","processedAmount":10.50}"#).unwrap();
        let out = detail_table(&WIDGET, &v);
        assert!(out.contains("10.50"), "{out}");
    }

    #[test]
    fn list_table_prints_a_header_a_rule_and_one_row_per_item() {
        let items = vec![json!({"widgetId": "w_1", "firstName": "Ada", "lastName": "Lovelace"})];
        let out = lines(&list_table(&WIDGET, &items));
        assert!(out[0].starts_with("ID"), "{out:?}");
        assert!(out[0].contains("OWNER"), "{out:?}");
        assert!(out[1].chars().all(|c| c == '-'), "{out:?}");
        assert!(out[2].starts_with("w_1"), "{out:?}");
        assert!(out[2].contains("Ada Lovelace"), "{out:?}");
    }

    /// A list must align, so a cell is padded or truncated to its declared
    /// width — the opposite rule from the detail view, deliberately.
    #[test]
    fn list_cells_are_padded_and_truncated_to_the_declared_width() {
        let items = vec![json!({"widgetId": "w_1234567890"})];
        let row = &lines(&list_table(&WIDGET, &items))[2];
        let first: String = row.chars().take(8).collect();
        assert_eq!(first.chars().count(), 8);
        assert!(first.ends_with('…'), "{row}");
    }

    /// A missing cell and a null cell both become a dash: a column has to
    /// occupy its width either way.
    #[test]
    fn list_cells_dash_when_missing_or_null() {
        let items = vec![json!({"widgetId": null})];
        let row = &lines(&list_table(&WIDGET, &items))[2];
        assert!(row.starts_with('—'), "{row}");
        // Every column: the pointers find nothing and the derived OWNER
        // column has nothing to derive from.
        assert_eq!(row.matches('—').count(), WIDGET.columns.len(), "{row}");
    }

    #[test]
    fn an_empty_page_is_a_header_and_a_rule_and_nothing_else() {
        assert_eq!(lines(&list_table(&WIDGET, &[])).len(), 2);
    }

    #[test]
    fn id_of_reads_the_declared_id_pointer() {
        assert_eq!(
            id_of(&WIDGET, &json!({"widgetId": "w_1"})).as_deref(),
            Some("w_1")
        );
        assert_eq!(id_of(&WIDGET, &json!({"other": "x"})), None);
    }

    /// A bodyless write has no resource to print, so the confirmation is
    /// built from the id the caller supplied — the only identity available.
    #[test]
    fn a_confirmation_reports_the_id_key_the_descriptor_names() {
        let data = confirmation_data(&WIDGET, "w_1", "deleted");
        assert_eq!(data["widgetId"], "w_1");
        assert_eq!(data["deleted"], true);
    }

    /// A settings document lives at a fixed path and has no id, so the
    /// confirmation must not report an empty one as if it were an identifier.
    #[test]
    fn a_confirmation_without_an_id_omits_it_rather_than_emptying_it() {
        let data = confirmation_data(&WIDGET, "", "updated");
        assert_eq!(data["updated"], true);
        assert!(data.get("widgetId").is_none(), "{data}");
        assert_eq!(data.as_object().unwrap().len(), 1);
    }
}
