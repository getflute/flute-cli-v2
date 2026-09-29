//! Exchange conformance against the vendored OpenAPI bundle.
//!
//! The oracle for layers 1, 2 and 4a. Validation is **operation-oriented**, not
//! schema-oriented: a body that is internally valid but attached to the wrong
//! endpoint has to fail, and only checking method and path against the
//! operation can see that.

use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashSet};
use std::sync::LazyLock;

pub static SPEC: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../docs/reference/openapi-v2.json"))
        .expect("the vendored bundle is not valid JSON")
});

/// The bundle rewritten into a dialect `jsonschema` can compile.
///
/// The bundle is OpenAPI 3.0.3, whose Schema Object is not JSON Schema
/// 2020-12. One keyword actually changes validation: `nullable: true` appears
/// 429 times and means nothing in 2020-12, so a null in a nullable field would
/// fail its declared type. It is folded into the type union instead, which
/// keeps the type check — a *number* in a nullable string still fails.
///
/// `example` and `xml` are annotations, dropped to keep the compiled schemas
/// small. `discriminator` is not stripped because the bundle contains none.
static RELAXED: LazyLock<Value> = LazyLock::new(|| relax(&SPEC));

fn relax(node: &Value) -> Value {
    match node {
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                if matches!(k.as_str(), "nullable" | "example" | "xml") {
                    continue;
                }
                out.insert(k.clone(), relax(v));
            }
            if map.get("nullable") == Some(&Value::Bool(true)) {
                match out.get("type").cloned() {
                    Some(Value::String(t)) => {
                        out.insert("type".into(), json!([t, "null"]));
                    }
                    Some(Value::Array(mut ts)) => {
                        if !ts.contains(&json!("null")) {
                            ts.push(json!("null"));
                        }
                        out.insert("type".into(), Value::Array(ts));
                    }
                    _ => {}
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(relax).collect()),
        other => other.clone(),
    }
}

/// An exact JSON number for a fixture amount.
///
/// **Every fixture amount must come through here.** A float literal does not
/// survive: with `serde_json`'s `arbitrary_precision`, `json!(10.50)` goes
/// through `f64` and serialises as `10.5`, while the CLI sends `10.50`. The
/// two are unequal as `Value`s, so a fixture written with a bare literal
/// fails to match the request the CLI actually made — for the wrong reason.
pub fn amount(s: &str) -> Value {
    serde_json::from_str::<serde_json::Number>(s)
        .map(Value::Number)
        .unwrap_or_else(|e| panic!("fixture amount {s} is not a JSON number: {e}"))
}

/// Owned, because the contract matrix constructs these and the command tests
/// consume the same values. A borrowed fixture would force every group's test
/// to rebuild the body by hand, which is how a mock and its conformance check
/// drift apart.
#[derive(Clone, Debug)]
pub struct RequestFixture {
    /// The method and path the CLI actually used. Without these the harness
    /// cannot tell a right body on the wrong endpoint from a right exchange.
    pub method: String,
    pub path: String,
    pub path_params: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
    pub content_type: Option<String>,
    pub body: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct ResponseFixture {
    pub status: u16,
    pub body: Option<Value>,
}

/// One request paired with the response it is expected to produce.
#[derive(Clone, Debug)]
pub struct Exchange {
    pub request: RequestFixture,
    pub response: ResponseFixture,
}

/// The bundle operation a contract row names, found by the row's route
/// rather than the bundle's `operationId`, so an upstream rename of an id that
/// changes nothing on the wire changes nothing here. Returns its lowercase
/// method, templated path, and a pointer into `spec`.
fn lookup(spec: &'static Value, operation_id: &str) -> (String, String, &'static Value) {
    let contract = super::contracts::CONTRACTS
        .iter()
        .find(|c| c.operation_id == operation_id)
        .unwrap_or_else(|| panic!("no contract row for {operation_id}"));
    let (method, path) = contract
        .route
        .split_once(' ')
        .unwrap_or_else(|| panic!("{operation_id}: route is not `METHOD /path`"));
    let method = method.to_ascii_lowercase();
    let op = &spec["paths"][path][&method];
    assert!(
        op.is_object(),
        "{operation_id}: no {} in the vendored spec",
        contract.route
    );
    (method, path.to_string(), op)
}

/// Locate an operation by its contract row, returning its method, templated
/// path, and a pointer into the relaxed bundle.
pub fn operation(operation_id: &str) -> (String, String, &'static Value) {
    lookup(&RELAXED, operation_id)
}

/// The same operation in the **unrelaxed** bundle, for reading annotations
/// that `relax` drops.
pub fn operation_raw(operation_id: &str) -> &'static Value {
    lookup(&SPEC, operation_id).2
}

fn is_http_method(m: &str) -> bool {
    matches!(
        m,
        "get" | "put" | "post" | "delete" | "options" | "head" | "patch" | "trace"
    )
}

fn all_operations() -> Vec<(String, String, &'static Value)> {
    let spec: &'static Value = &RELAXED;
    let mut out = Vec::new();
    for (path, item) in spec["paths"].as_object().unwrap() {
        for (method, op) in item.as_object().unwrap() {
            if is_http_method(method) && op.is_object() {
                out.push((method.clone(), path.clone(), op));
            }
        }
    }
    out
}

/// Webhooks are out of scope for this CLI, so they are excluded here rather
/// than listed as exclusions in every matrix.
fn is_webhook(path: &str) -> bool {
    path.starts_with("/v2/webhooks")
}

/// `METHOD /path`, the spelling of a contract row's `route`.
fn route(method: &str, path: &str) -> String {
    format!("{} {path}", method.to_ascii_uppercase())
}

pub fn non_webhook_routes() -> Vec<String> {
    all_operations()
        .into_iter()
        .filter(|(_, path, _)| !is_webhook(path))
        .map(|(method, path, _)| route(&method, &path))
        .collect()
}

/// Non-webhook, non-OAuth `POST`/`PATCH`/`DELETE`, by route. Derived, never
/// counted by hand: a hard-coded total is how "every write endpoint --
/// nineteen" came to omit eleven of them.
pub fn write_routes() -> Vec<String> {
    all_operations()
        .into_iter()
        .filter(|(method, path, _)| {
            !is_webhook(path)
                && path != "/oauth2/token"
                && matches!(method.as_str(), "post" | "patch" | "delete")
        })
        .map(|(method, path, _)| route(&method, &path))
        .collect()
}

/// Every 2xx response declaring no `application/json` content, by route.
pub fn bodyless_successes() -> Vec<(String, u16)> {
    let mut out = Vec::new();
    for (method, path, op) in all_operations() {
        if is_webhook(&path) {
            continue;
        }
        for (status, resp) in op["responses"].as_object().unwrap() {
            let Ok(code) = status.parse::<u16>() else {
                continue;
            };
            if (200..300).contains(&code) && !resp["content"]["application/json"].is_object() {
                out.push((route(&method, &path), code));
            }
        }
    }
    out
}

/// Parameters declared on the operation, with `$ref`s resolved.
fn resolved_parameters(op: &'static Value) -> Vec<Value> {
    op["parameters"]
        .as_array()
        .map(|ps| ps.iter().map(|p| resolve(p).clone()).collect())
        .unwrap_or_default()
}

/// Walk a `#/`-rooted JSON pointer from the relaxed bundle's root.
fn follow(pointer: &str) -> &'static Value {
    let mut node: &'static Value = &RELAXED;
    for segment in pointer.trim_start_matches("#/").split('/') {
        node = &node[segment];
    }
    assert!(!node.is_null(), "dangling $ref {pointer}");
    node
}

/// Resolve a `$ref`, or hand back the node unchanged.
///
/// The returned lifetime is the caller's: a resolved `$ref` is `'static`
/// because it comes from the bundle root, and `'static` outlives any `'a`.
fn resolve(node: &Value) -> &Value {
    let mut current = node;
    let mut hops = 0;
    while let Some(pointer) = current["$ref"].as_str() {
        hops += 1;
        assert!(hops < 32, "$ref chain too deep at {pointer}");
        current = follow(pointer);
    }
    current
}

/// The `application/json` request schema with its `$ref` followed, so
/// `required` is read from the schema rather than from the reference.
fn resolved_request_schema(op: &'static Value) -> Option<&'static Value> {
    let schema = &op["requestBody"]["content"]["application/json"]["schema"];
    if schema.is_object() {
        Some(resolve(schema))
    } else {
        None
    }
}

/// The first 2xx `application/json` response schema, `$ref` followed.
fn resolved_response_schema(op: &'static Value) -> Option<&'static Value> {
    op["responses"].as_object()?.iter().find_map(|(status, r)| {
        let schema = &r["content"]["application/json"]["schema"];
        (status.starts_with('2') && schema.is_object()).then(|| resolve(schema))
    })
}

fn placeholders(template: &str) -> Vec<String> {
    template
        .split('/')
        .filter_map(|s| {
            s.strip_prefix('{')
                .and_then(|s| s.strip_suffix('}'))
                .map(str::to_string)
        })
        .collect()
}

/// A rendered path matches a template when their segment counts agree and
/// every non-placeholder segment is equal.
fn path_matches(template: &str, rendered: &str) -> bool {
    let (t, r): (Vec<_>, Vec<_>) = (template.split('/').collect(), rendered.split('/').collect());
    t.len() == r.len() && t.iter().zip(&r).all(|(t, r)| t.starts_with('{') || t == r)
}

/// Compile `schema` with the bundle's `components` alongside it, so internal
/// `$ref`s resolve.
///
/// A bare subschema does not compile: its `$ref`s point at
/// `#/components/schemas/...`, which is not present in the fragment.
fn compile(schema: &Value) -> jsonschema::Validator {
    let mut rooted = match schema {
        Value::Object(map) => map.clone(),
        other => {
            let mut m = Map::new();
            m.insert("__schema".into(), other.clone());
            m
        }
    };
    rooted.insert("components".into(), RELAXED["components"].clone());
    jsonschema::validator_for(&Value::Object(rooted))
        .unwrap_or_else(|e| panic!("schema does not compile: {e}"))
}

fn assert_validates(operation_id: &str, which: &str, schema: &Value, body: &Value) {
    let validator = compile(schema);
    let errors: Vec<String> = validator
        .iter_errors(body)
        .map(|e| format!("  at {}: {e}", e.instance_path))
        .collect();
    assert!(
        errors.is_empty(),
        "{operation_id}: {which} body does not conform:\n{}",
        errors.join("\n")
    );
}

/// Validate one query value against its declared parameter schema.
///
/// A query value is always a string on the wire, so an `integer` or `boolean`
/// parameter is coerced before validation — otherwise `pageSize=20` would fail
/// its own declared type and every paginated call would look non-conformant.
///
/// An `array` parameter is the same idea one level down: it arrives as
/// repeated pairs, so each *value* is checked against `items` rather than
/// against the array schema, which no single value can satisfy. The parameter
/// still has to be declared — the lookup that finds this schema is what
/// rejects an undeclared name, array or not.
fn assert_scalar_validates(operation_id: &str, name: &str, schema: &Value, raw: &str) {
    if has_type(schema, "array") {
        let items = &schema["items"];
        // A schema-less `items` constrains nothing, so there is nothing left
        // to check about the value.
        if items.is_object() {
            assert_scalar_validates(operation_id, name, items, raw);
        }
        return;
    }
    let coerced = match schema["type"].as_str() {
        Some("integer") => raw
            .parse::<i64>()
            .map(|n| json!(n))
            .unwrap_or_else(|_| json!(raw)),
        Some("number") => amount(raw),
        Some("boolean") => match raw {
            "true" => json!(true),
            "false" => json!(false),
            other => json!(other),
        },
        _ => json!(raw),
    };
    let validator = compile(schema);
    let errors: Vec<String> = validator
        .iter_errors(&coerced)
        .map(|e| e.to_string())
        .collect();
    assert!(
        errors.is_empty(),
        "{operation_id}: query parameter {name}={raw} does not conform: {}",
        errors.join("; ")
    );
}

/// Validate a whole exchange against one operation.
pub fn assert_exchange_conforms(operation_id: &str, req: &RequestFixture, resp: &ResponseFixture) {
    let (method, template, op) = operation(operation_id);

    // 1. Method and path match the operation. This is the check that makes the
    //    rest meaningful — a valid body on the wrong endpoint fails here.
    assert_eq!(
        req.method.to_lowercase(),
        method,
        "{operation_id}: expected {method}, fixture used {}",
        req.method
    );
    assert!(
        path_matches(&template, &req.path),
        "{operation_id}: path {} does not match template {template}",
        req.path
    );

    // 2. Every {placeholder} is supplied, and the rendered path agrees.
    for name in placeholders(&template) {
        let v = req
            .path_params
            .iter()
            .find(|(k, _)| k == &name)
            .unwrap_or_else(|| panic!("{operation_id}: path parameter {name} not supplied"));
        assert!(
            req.path.contains(&v.1),
            "{operation_id}: rendered path is missing {name} = {}",
            v.1
        );
    }

    let declared = resolved_parameters(op);

    // 3. Required parameters are present; every supplied one is declared and
    //    validates. An undeclared query parameter is a silent-ignore risk,
    //    so it fails rather than passes.
    for p in &declared {
        let name = p["name"].as_str().unwrap();
        if p["in"] == "query" && p["required"] == true {
            assert!(
                req.query.iter().any(|(k, _)| k == name),
                "{operation_id}: required query parameter {name} not supplied"
            );
        }
    }
    for (k, v) in &req.query {
        let p = declared
            .iter()
            .find(|p| p["name"] == k.as_str() && p["in"] == "query")
            .unwrap_or_else(|| panic!("{operation_id}: query parameter {k} is not declared"));
        assert_scalar_validates(operation_id, k, &p["schema"], v);
    }

    // 4. Content type is one the operation declares.
    if let Some(content) = op["requestBody"]["content"].as_object() {
        let used = req.content_type.as_deref().unwrap_or("application/json");
        assert!(
            content.contains_key(used),
            "{operation_id}: content type {used} not declared; spec has {:?}",
            content.keys()
        );
    }

    // 5. Body presence is decided by the schema, not by `requestBody.required`
    //    — which is absent on 23 of the 24 operations that take a body while
    //    twelve of their schemas declare required properties. Trusting the
    //    flag would accept an empty-bodied POST /v2/transactions.
    let schema_requires_fields = resolved_request_schema(op)
        .and_then(|s| s["required"].as_array().map(|r| !r.is_empty()))
        .unwrap_or(false);
    let body_required = op["requestBody"]["required"] == true || schema_requires_fields;
    match (op["requestBody"]["content"].as_object(), req.body.as_ref()) {
        (Some(content), Some(body)) => {
            let used = req.content_type.as_deref().unwrap_or("application/json");
            let stripped = strip_request_divergences(operation_id, body);
            let schema = relax_request_constraints(operation_id, &content[used]["schema"]);
            assert_validates(operation_id, "request", &schema, &stripped);
        }
        (Some(_), None) => assert!(
            !body_required,
            "{operation_id}: requestBody is required, fixture sends none"
        ),
        (None, Some(_)) => panic!("{operation_id}: sent a body, spec declares none"),
        (None, None) => {}
    }

    // 6. The response status is declared for THIS operation.
    let status = resp.status.to_string();
    let responses = op["responses"].as_object().unwrap();
    assert!(
        responses.contains_key(&status),
        "{operation_id}: status {status} not declared; spec has {:?}",
        responses.keys()
    );

    // 7. Empty-versus-JSON matches the declared status, in both directions.
    let declares_body = responses[&status]["content"]["application/json"].is_object();
    match (declares_body, resp.body.as_ref()) {
        (true, None) => panic!("{operation_id}: {status} declares a JSON body, fixture has none"),
        (false, Some(_)) => {
            panic!("{operation_id}: {status} declares no body, fixture invents one")
        }
        (true, Some(body)) => match response_shape_divergence(operation_id) {
            Some(schema) => assert_validates(
                operation_id,
                "response",
                &serde_json::json!({ "$ref": format!("#/components/schemas/{schema}") }),
                body,
            ),
            None => assert_validates(
                operation_id,
                "response",
                &responses[&status]["content"]["application/json"]["schema"],
                body,
            ),
        },
        (false, None) => {}
    }
}

pub fn assert_bundle_hash(bytes: &[u8], recorded: &str) {
    use sha2::{Digest, Sha256};
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        recorded,
        "vendored bundle does not match openapi-v2.sha256. Re-vendoring is a \
         deliberate act: read the diff, note anything that moved, then \
         re-record the hash."
    );
}

// ── Divergences ──────────────────────────────────────────────────────────────

pub struct Divergence {
    /// What this rule suspends, as a phrase that reads in an assertion.
    pub name: &'static str,
    pub rule: Rule,
    /// The live test proving the runtime behaves this way.
    pub evidence: &'static str,
    /// The condition under which this rule is deleted.
    pub removal: &'static str,
}

pub enum Rule {
    /// Strip exactly this JSON pointer from the **request** body before
    /// validating. Everything else, including any *other* undocumented field,
    /// still has to validate.
    RequestField {
        operations: &'static [&'static str],
        pointer: &'static str,
    },

    /// Drop the named keywords from one **declared** request property before
    /// validating.
    ///
    /// A pointer strip cannot express this case: the property may be required,
    /// so removing it from the body fails for a different reason than the one
    /// being exempted — and a *bound* is not the whole property. Type,
    /// `required`, `additionalProperties` and every other property's
    /// constraints still apply, so this is narrower than it looks.
    ///
    /// The keywords are named rather than fixed, because the two rules that
    /// need this do not relax the same thing: one has a pattern its own
    /// example fails, the other a minimum its own description contradicts.
    /// Every named keyword must be present on the property, or the rule is
    /// stale.
    RequestFieldConstraint {
        operations: &'static [&'static str],
        pointer: &'static str,
        keywords: &'static [&'static str],
    },

    /// Validate the **response** body against another declared component
    /// schema instead of the operation's own declared response schema.
    ///
    /// The substitute is a schema, not the operation's examples: an example
    /// constrains nothing it leaves out and permits every field it shows,
    /// true or not, so an example-shaped oracle agrees with whatever the
    /// examples say.
    ResponseShape {
        operations: &'static [&'static str],
        schema: &'static str,
    },
}

pub static DIVERGENCES: &[Divergence] = &[
    Divergence {
        name: "captureMethod on the create-transaction card body",
        rule: Rule::RequestField {
            operations: &["flute-v2-post-transactions"],
            pointer: "/transactionDetails/cardData/captureMethod",
        },
        evidence: "live_card_auth_manual_capture",
        removal: "published CardDataDto includes captureMethod",
    },
    Divergence {
        name: "customerId on the create-transaction body",
        rule: Rule::RequestField {
            operations: &["flute-v2-post-transactions"],
            pointer: "/customerId",
        },
        evidence: "live_card_sale_with_customer",
        removal: "published CreateTransactionRequestDto includes customerId",
    },
    Divergence {
        // `SendReceiptRequestDto.shareBy` is described as an enum of words
        // and declares `format: E.164` with a pattern its own example fails.
        // The pattern is a copy of the neighbouring `recipient` constraint.
        //
        // The CLI sends what the description documents, so the constraint is
        // dropped from that one property before validation. Everything else in
        // the body, `recipient` included, still has to validate.
        name: "the E.164 pattern on SendReceiptRequestDto.shareBy",
        rule: Rule::RequestFieldConstraint {
            operations: &["flute-v2-post-transactions-transactionId-share-receipt"],
            pointer: "/shareBy",
            keywords: &["pattern", "format"],
        },
        evidence: "live_share_receipt_attended",
        removal: "SendReceiptRequestDto.shareBy declares the enum its description \
                  documents instead of an E.164 pattern",
    },
    Divergence {
        // `CreatePaymentSessionRequestDto.amount` declares `minimum: 0.01`,
        // and its own description says the amount "must be zero" for a
        // `SaveMethod` session — the documented value is invalid against the
        // declared bound. Nothing else in the schema can express a vault-only
        // session, and an absent amount is documented as meaning something
        // different (flexible, set at checkout).
        //
        // The CLI sends what the description documents, so the bound is
        // dropped from that one property before validating. The type,
        // `additionalProperties` and every other property still apply.
        name: "the minimum on CreatePaymentSessionRequestDto.amount",
        rule: Rule::RequestFieldConstraint {
            operations: &["flute-v2-post-payment-sessions"],
            pointer: "/amount",
            keywords: &["minimum"],
        },
        evidence: "live_payment_session_vault_only_sends_a_zero_amount",
        removal: "CreatePaymentSessionRequestDto.amount admits the zero its own \
                  description requires for a SaveMethod session",
    },
    Divergence {
        // The seven writes declare `PageOfGetTransactionResponseDto`, and
        // their examples show `processorResponse` and `amountDetails`; the
        // API answers each with one transaction in the read's shape, the
        // object `GET /v2/transactions/{transactionId}` declares.
        name: "the paged response declared on seven single-transaction writes",
        rule: Rule::ResponseShape {
            schema: "GetTransactionResponseDtoFull",
            operations: &[
                "flute-v2-post-transactions",
                "flute-v2-post-transactions-transactionId-capture",
                "flute-v2-post-transactions-transactionId-reversal",
                "flute-v2-post-transactions-credit",
                "flute-v2-post-transactions-transactionId-tip-adjustment",
                "flute-v2-post-transactions-transactionId-ach-hold",
                "flute-v2-post-transactions-transactionId-ach-release",
            ],
        },
        evidence: "live_card_sale_auto_capture",
        removal: "the seven writes declare GetTransactionResponseDtoFull as their response schema",
    },
];

/// The component schema a `ResponseShape` divergence validates this
/// operation's response against, if one does.
pub fn response_shape_divergence(operation_id: &str) -> Option<&'static str> {
    DIVERGENCES.iter().find_map(|d| match &d.rule {
        Rule::ResponseShape { operations, schema } => {
            operations.contains(&operation_id).then_some(*schema)
        }
        Rule::RequestField { .. } | Rule::RequestFieldConstraint { .. } => None,
    })
}

/// The leaf pointers of one named component schema, in the surface matrix's
/// spelling.
pub fn schema_leaves(schema: &str) -> Vec<String> {
    let mut visited = HashSet::new();
    leaves(&SPEC["components"]["schemas"][schema], "", &mut visited)
}

/// Remove each `RequestField` pointer scoped to this operation.
///
/// Only the named pointer is removed, so a *second* undocumented field still
/// fails. "Validate the documented subset" would hide the next one as
/// effectively as the current one.
pub fn strip_request_divergences(operation_id: &str, body: &Value) -> Value {
    let mut out = body.clone();
    for d in DIVERGENCES {
        if let Rule::RequestField {
            operations,
            pointer,
        } = &d.rule
        {
            if operations.contains(&operation_id) {
                remove_pointer(&mut out, pointer);
            }
        }
    }
    out
}

/// Drop `pattern` and `format` from each `RequestFieldConstraint` pointer
/// scoped to this operation.
///
/// The schema is copied rather than mutated in place: `RELAXED` is shared, and
/// a divergence must not quietly weaken every other operation that references
/// the same component.
pub fn relax_request_constraints(operation_id: &str, schema: &Value) -> Value {
    let mut out = resolve(schema).clone();
    for d in DIVERGENCES {
        let Rule::RequestFieldConstraint {
            operations,
            pointer,
            keywords,
        } = &d.rule
        else {
            continue;
        };
        if !operations.contains(&operation_id) {
            continue;
        }
        let mut node = &mut out;
        for segment in pointer.trim_start_matches('/').split('/') {
            let Some(next) = node.get_mut("properties").and_then(|p| p.get_mut(segment)) else {
                // A pointer naming a property the schema does not declare is
                // a stale rule, not a silent no-op.
                panic!(
                    "{}: {pointer} is not a declared property of {operation_id}",
                    d.name
                );
            };
            node = next;
        }
        let map = node
            .as_object_mut()
            .unwrap_or_else(|| panic!("{}: {pointer} is not a schema object", d.name));
        for keyword in *keywords {
            assert!(
                map.remove(*keyword).is_some(),
                "{}: {pointer} declares no {keyword}, so that much of the \
                 exemption has nothing to relax and is stale",
                d.name
            );
        }
    }
    out
}

fn remove_pointer(root: &mut Value, pointer: &str) {
    let segments: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    let (last, parents) = segments.split_last().expect("empty pointer");
    let mut node = root;
    for segment in parents {
        match node.get_mut(*segment) {
            Some(next) => node = next,
            None => return,
        }
    }
    if let Some(map) = node.as_object_mut() {
        map.remove(*last);
    }
}

// ── Leaf request-field derivation (layer 4a) ─────────────────────────────────

/// The leaf request-body pointers of one operation, whatever its state.
///
/// `request_fields_of_mapped_operations` covers only mapped operations, which
/// is right for the surface matrix and useless for checking the generated fact
/// sheet — the sheet's whole purpose is to describe operations nobody has
/// built yet.
pub fn request_leaves(operation_id: &str) -> Vec<String> {
    let (_, _, op) = operation(operation_id);
    match resolved_request_schema(op) {
        Some(schema) => {
            let mut visited = HashSet::new();
            leaves(schema, "", &mut visited)
        }
        None => vec![],
    }
}

/// Every **leaf** request-body pointer and query parameter of every mapped
/// operation.
///
/// Leaf, not top level: a single row for `/transactionDetails` would account
/// for the whole card and ACH surface at once, which is the miss the surface
/// matrix exists to catch.
pub fn request_fields_of_mapped_operations() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for contract in super::contracts::CONTRACTS {
        let super::contracts::Mapping::Command(_) = &contract.mapping else {
            continue;
        };
        let (_, _, op) = operation(contract.operation_id);
        let id = contract.operation_id.to_string();

        for p in resolved_parameters(op) {
            if p["in"] == "query" {
                out.push((id.clone(), format!("?{}", p["name"].as_str().unwrap())));
            }
        }
        if let Some(schema) = resolved_request_schema(op) {
            let mut visited = HashSet::new();
            for leaf in leaves(schema, "", &mut visited) {
                out.push((id.clone(), leaf));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Does `schema` declare `wanted` as one of its types?
///
/// The relaxation pass rewrites a nullable `type: "array"` into
/// `type: ["array", "null"]`, so an equality test against the string silently
/// stops descending into every nullable array — and a nullable array is how
/// the bundle declares most collections.
fn has_type(schema: &Value, wanted: &str) -> bool {
    match &schema["type"] {
        Value::String(t) => t == wanted,
        Value::Array(ts) => ts.iter().any(|t| t == wanted),
        _ => false,
    }
}

/// Collect leaf pointers under `schema`.
///
/// `$ref` is followed with a visited set so a self-referential schema
/// terminates. An array contributes a `/[]` step. `allOf`/`oneOf`/`anyOf`
/// contribute the union of their branches, keyed by pointer — the CLI either
/// reaches a field or it does not, regardless of which branch documents it. A
/// schema that resolves to no leaves at all is itself a leaf, so a free-form
/// object cannot vanish from the matrix.
fn leaves(schema: &Value, prefix: &str, visited: &mut HashSet<String>) -> Vec<String> {
    if let Some(pointer) = schema["$ref"].as_str() {
        let key = format!("{prefix}#{pointer}");
        if !visited.insert(key) {
            return vec![prefix.to_string()];
        }
        return leaves(resolve(schema), prefix, visited);
    }

    let mut out = Vec::new();
    for branch_key in ["allOf", "oneOf", "anyOf"] {
        if let Some(branches) = schema[branch_key].as_array() {
            for branch in branches {
                out.extend(leaves(branch, prefix, visited));
            }
        }
    }

    if let Some(props) = schema["properties"].as_object() {
        for (name, sub) in props {
            out.extend(leaves(sub, &format!("{prefix}/{name}"), visited));
        }
    } else if has_type(schema, "array") {
        let items = &schema["items"];
        if items.is_object() {
            out.extend(leaves(items, &format!("{prefix}/[]"), visited));
        } else {
            out.push(format!("{prefix}/[]"));
        }
    }

    if out.is_empty() && !prefix.is_empty() {
        out.push(prefix.to_string());
    }
    out.sort();
    out.dedup();
    out
}

// ── Response leaves and declared vocabularies ────────────────────────────────

/// The leaf response-body pointers of one operation, in the surface matrix's
/// spelling, so an array element step reads `/items/[]/id`.
///
/// An operation whose 2xx carries no JSON body has none.
pub fn response_leaves(operation_id: &str) -> Vec<String> {
    let (_, _, op) = operation(operation_id);
    match resolved_response_schema(op) {
        Some(schema) => {
            let mut visited = HashSet::new();
            leaves(schema, "", &mut visited)
        }
        None => vec![],
    }
}

/// One named property of `schema`, looked through `$ref` and through any
/// `allOf`/`oneOf`/`anyOf` branch that declares it.
fn property<'a>(schema: &'a Value, name: &str) -> Option<&'a Value> {
    let schema = resolve(schema);
    if let Some(sub) = schema["properties"].get(name) {
        return Some(sub);
    }
    ["allOf", "oneOf", "anyOf"].into_iter().find_map(|key| {
        schema[key]
            .as_array()?
            .iter()
            .find_map(|branch| property(branch, name))
    })
}

/// The schema at one JSON pointer, `/[]` stepping into an array's items.
fn schema_at<'a>(schema: &'a Value, pointer: &str) -> Option<&'a Value> {
    let mut node = schema;
    for segment in pointer.split('/').filter(|s| !s.is_empty()) {
        node = if segment == "[]" {
            let items = &resolve(node)["items"];
            items.is_object().then_some(items)?
        } else {
            property(node, segment)?
        };
    }
    Some(resolve(node))
}

fn declared_enum(schema: &Value, what: &str) -> BTreeSet<String> {
    let values = schema["enum"]
        .as_array()
        .unwrap_or_else(|| panic!("{what} declares no enum"));
    values
        .iter()
        .map(|v| {
            v.as_str()
                .unwrap_or_else(|| panic!("{what} declares a non-string enum value {v}"))
                .to_string()
        })
        .collect()
}

/// The value set the bundle declares for one request field: `?name` for a
/// query parameter, a JSON pointer for a body field.
///
/// A field the operation does not declare, or declares without an `enum`, is a
/// stale row rather than an empty vocabulary, so it panics.
pub fn request_enum(operation_id: &str, field: &str) -> BTreeSet<String> {
    let (_, _, op) = operation(operation_id);
    let what = format!("{operation_id} {field}");
    let schema = match field.strip_prefix('?') {
        Some(name) => resolved_parameters(op)
            .into_iter()
            .find(|p| p["in"] == "query" && p["name"] == name)
            .map(|p| resolve(&p["schema"]).clone())
            .unwrap_or_else(|| panic!("{what} is not a declared query parameter")),
        None => resolved_request_schema(op)
            .and_then(|schema| schema_at(schema, field))
            .unwrap_or_else(|| panic!("{what} is not a declared request field"))
            .clone(),
    };
    declared_enum(&schema, &what)
}

/// The same, at a pointer into the declared 2xx response body.
pub fn response_enum(operation_id: &str, pointer: &str) -> BTreeSet<String> {
    let (_, _, op) = operation(operation_id);
    let what = format!("{operation_id} response {pointer}");
    let schema = resolved_response_schema(op)
        .and_then(|schema| schema_at(schema, pointer))
        .unwrap_or_else(|| panic!("{what} is not a declared response field"));
    declared_enum(schema, &what)
}
