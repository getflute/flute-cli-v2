mod support;

use predicates::prelude::*;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn create_posts_exact_body_to_v2_customers() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/customers"))
        .and(body_json(serde_json::json!({
            "firstName": "Ada", "lastName": "Lovelace", "email": "ada@example.com"
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"customerId": "cus_1"})),
        )
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "--output",
            "quiet",
            "customers",
            "create",
            "--first-name",
            "Ada",
            "--last-name",
            "Lovelace",
            "--email",
            "ada@example.com",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("cus_1"));
}

#[tokio::test]
async fn get_renders_json_envelope_with_object_customer() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/customers/cus_1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"customerId": "cus_1", "firstName": "Ada"})),
        )
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "customers", "get", "cus_1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "customer");
    assert_eq!(v["data"]["customerId"], "cus_1");
    // A read of one resource is not a page.
    assert!(v["meta"].get("page_info").is_none());
}

/// A 404 on a read is not found; the idempotent-success rule covers deletes
/// only.
#[tokio::test]
async fn get_on_missing_customer_exits_4() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/customers/nope"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"Title":"Not found","CorrelationId":"c-9"})),
        )
        .mount(&server)
        .await;

    support::bin(&server)
        .args(["customers", "get", "nope"])
        .assert()
        .code(4);
}

/// A validation failure is exit 3 and its cause reaches the user without
/// `--debug`.
#[tokio::test]
async fn create_surfaces_a_field_level_validation_error() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/customers"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "Title": "Validation failed",
            "Cause": "email is not a valid address",
            "Resolution": "Supply a valid email",
            "CorrelationId": "c-4",
            "Errors": {"email": ["is not a valid address"]}
        })))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "customers",
            "create",
            "--first-name",
            "Ada",
            "--last-name",
            "Lovelace",
            "--email",
            "nope",
        ])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "api");
    assert_eq!(v["status"], 400);
    assert_eq!(v["correlation_id"], "c-4");
    assert!(v["message"].as_str().unwrap().contains("email"));
}

/// The address flags reach the wire under v2's key names, nested.
#[tokio::test]
async fn create_sends_the_billing_address_under_v2_key_names() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/customers"))
        .and(body_json(serde_json::json!({
            "firstName": "Ada", "lastName": "Lovelace",
            "billingAddress": {
                "addressLine1": "1 Main St", "city": "Austin",
                "stateCode": "TX", "postalCode": "78701", "countryCode": "US"}
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"customerId": "cus_1"})),
        )
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "customers",
            "create",
            "--first-name",
            "Ada",
            "--last-name",
            "Lovelace",
            "--billing-line1",
            "1 Main St",
            "--billing-city",
            "Austin",
            "--billing-state",
            "TX",
            "--billing-postal-code",
            "78701",
            "--billing-country",
            "US",
        ])
        .assert()
        .success();
}

/// The company field is spelled `companyName` on the wire and `--company` on
/// the command line.
#[tokio::test]
async fn create_sends_the_company_under_the_v2_key_name() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/customers"))
        .and(body_json(serde_json::json!({
            "firstName": "Ada", "lastName": "Lovelace",
            "companyName": "Analytical Engines"
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"customerId": "cus_1"})),
        )
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "customers",
            "create",
            "--first-name",
            "Ada",
            "--last-name",
            "Lovelace",
            "--company",
            "Analytical Engines",
        ])
        .assert()
        .success();
}

/// A missing required flag is a clap usage error, so no request is issued.
#[tokio::test]
async fn create_without_a_last_name_exits_3_without_calling_the_api() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["customers", "create", "--first-name", "Ada"])
        .assert()
        .code(3);
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
        "no /v2 request may be issued for a usage error"
    );
}

#[tokio::test]
async fn customer_create_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-customers", "minimal").await;
    support::bin(&server)
        .args([
            "customers",
            "create",
            "--first-name",
            "Ada",
            "--last-name",
            "Lovelace",
            "--email",
            "ada@example.com",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn customer_get_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-customers-customerId", "default").await;
    support::bin(&server)
        .args(["customers", "get", "8db2ff47-b143-4adb-ab58-a11111111111"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// The detail view reaches into a nested object, so a customer's address is
/// visible in table mode.
#[tokio::test]
async fn get_table_shows_the_nested_billing_address() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/customers/cus_1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "customerId": "cus_1",
            "firstName": "Ada",
            "billingAddress": {"city": "Austin", "stateCode": "TX", "countryCode": "US"}
        })))
        .mount(&server)
        .await;

    support::bin(&server)
        .args(["--output", "table", "customers", "get", "cus_1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("billingAddress.city"))
        .stdout(predicate::str::contains("Austin"))
        .stdout(predicate::str::contains("billingAddress.stateCode"));
}

/// The declared order is the resource's, not the JSON map's: `customerId`
/// prints first even though `companyName` sorts before it.
#[tokio::test]
async fn get_table_leads_with_the_customer_id() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/customers/cus_1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"companyName": "Analytical Engines", "customerId": "cus_1"}),
        ))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "table", "customers", "get", "cus_1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.lines().next().unwrap().starts_with("customerId"),
        "{text}"
    );
}

// ── list, update, delete ─────────────────────────────────────────────────────

#[tokio::test]
async fn customer_list_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-customers",
        "first page, server defaults",
    )
    .await;
    support::bin(&server)
        .args(["customers", "list"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn customer_list_filtered_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-customers", "filtered and paged").await;
    support::bin(&server)
        .args([
            "customers",
            "list",
            "--page-index",
            "1",
            "--page-size",
            "5",
            "--email",
            "ada@example.com",
            "--desc",
            "--sort-by",
            "lastName",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn customer_update_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-patch-customers-customerId",
        "changed fields",
    )
    .await;
    support::bin(&server)
        .args([
            "customers",
            "update",
            "8db2ff47-b143-4adb-ab58-a11111111111",
            "--company",
            "Analytical Engines",
            "--email",
            "ada@example.com",
            "--billing-city",
            "Austin",
            "--billing-state",
            "TX",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn customer_delete_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-delete-customers-customerId", "default").await;
    support::bin(&server)
        .args([
            "customers",
            "delete",
            "8db2ff47-b143-4adb-ab58-a11111111111",
            "--yes",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// The collection goes into `data` as an array and the API's `pageInfo` is
/// reproduced under `meta.page_info`, so an agent paginates without parsing
/// table output.
#[tokio::test]
async fn list_carries_the_page_info_in_the_envelope() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-get-customers",
        "first page, server defaults",
    )
    .await;
    let out = support::bin(&server)
        .args(["--output", "json", "customers", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "customer_list");
    assert!(v["data"].is_array(), "{v}");
    assert_eq!(v["meta"]["page_info"]["pageSize"], 20);
    assert_eq!(v["meta"]["page_info"]["hasMore"], false);
}

/// The list view is columns, not one line per field.
#[tokio::test]
async fn list_table_prints_the_declared_column_headers() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-get-customers",
        "first page, server defaults",
    )
    .await;
    support::bin(&server)
        .args(["--output", "table", "customers", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ID"))
        .stdout(predicate::str::contains("NAME"))
        .stdout(predicate::str::contains("LAST TXN"))
        .stdout(predicate::str::contains("Ada Lovelace"));
}

/// One id per line, and nothing else.
#[tokio::test]
async fn list_quiet_prints_one_id_per_line() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-get-customers",
        "first page, server defaults",
    )
    .await;
    let out = support::bin(&server)
        .args(["--output", "quiet", "customers", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "8db2ff47-b143-4adb-ab58-a11111111111\n"
    );
}

/// The bound comes from the parameter schema, and refusing it client-side
/// spends no round trip on a 400.
#[tokio::test]
async fn list_refuses_an_out_of_range_page_size_without_calling_the_api() {
    let server = support::mock_with_token().await;
    for size in ["0", "101"] {
        support::bin(&server)
            .args(["customers", "list", "--page-size", size])
            .assert()
            .code(3);
    }
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
        "an out-of-range page size must not reach the API"
    );
}

/// `--all` walks whole collection, so naming a starting page contradicts it.
#[tokio::test]
async fn list_all_with_a_page_index_is_refused_before_the_wire() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["customers", "list", "--all", "--page-index", "2"])
        .assert()
        .code(3);
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
    );
}

/// A multi-request sequence, which one canonical fixture cannot express.
/// `--all` follows `pageInfo.hasMore` and stops when it is false.
#[tokio::test]
async fn list_all_follows_has_more_until_it_is_false() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/customers"))
        .and(wiremock::matchers::query_param("pageIndex", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"customerId": "cus_1"}],
            "pageInfo": {"pageIndex": 0, "pageSize": 1, "hasMore": true}})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/customers"))
        .and(wiremock::matchers::query_param("pageIndex", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"customerId": "cus_2"}],
            "pageInfo": {"pageIndex": 1, "pageSize": 1, "hasMore": false}})))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "customers",
            "list",
            "--all",
            "--page-size",
            "1",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"].as_array().unwrap().len(), 2);
    // The data spans every page, so no single pageInfo describes it.
    assert!(v["meta"].get("page_info").is_none(), "{v}");
}

/// No field of `pageInfo` is declared required, so an absent `hasMore` stops
/// the walk rather than looping forever.
#[tokio::test]
async fn list_all_stops_when_has_more_is_absent() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/customers"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"customerId": "cus_1"}],
            "pageInfo": {"pageIndex": 0}})))
        .mount(&server)
        .await;

    support::bin(&server)
        .args(["customers", "list", "--all"])
        .assert()
        .success();
    let pages = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/v2/customers")
        .count();
    assert_eq!(pages, 1, "an absent hasMore must not request another page");
}

/// A server that ignores `pageIndex` answers every request with the same page
/// and `hasMore: true`, which is an endless walk the CLI must refuse rather
/// than join.
#[tokio::test]
async fn list_all_stops_when_a_page_repeats_the_previous_one() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/customers"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"customerId": "cus_1"}],
            "pageInfo": {"pageIndex": 0, "pageSize": 1, "hasMore": true}})))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "customers", "list", "--all"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "decode", "{v}");

    let pages = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/v2/customers")
        .count();
    assert!(pages < 5, "the walk kept going: {pages} pages requested");
}

/// Destructive, so it is refused client-side and **no request is issued** —
/// a confirmation gate that still reaches the API has already failed.
#[tokio::test]
async fn delete_without_yes_refuses_and_issues_no_request() {
    let server = support::mock_with_token().await;
    let out = support::bin(&server)
        .args(["customers", "delete", "cus_1"])
        .assert()
        .code(3)
        .get_output()
        .clone();
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim_end(),
        "Error: deletion requires --yes to confirm (e.g. `customers delete cus_1 --yes`)"
    );
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
        "a refused destructive command must not reach the API"
    );
}

/// Deleting an already-deleted customer is success: the caller's intent
/// holds. A 404 on `get` stays exit 4, which the test above asserts.
#[tokio::test]
async fn delete_on_a_missing_customer_still_exits_zero() {
    let server = support::mock_with_token().await;
    Mock::given(method("DELETE"))
        .and(path("/v2/customers/gone"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"Title": "Not found", "CorrelationId": "c-1"})),
        )
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "customers", "delete", "gone", "--yes"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["customerId"], "gone");
    assert_eq!(v["data"]["deleted"], false, "{v}");
    assert_eq!(v["data"]["found"], false, "{v}");
}

/// The table line for a delete that found nothing must not read as a
/// deletion: a mistyped id leaves the intended resource in place.
#[tokio::test]
async fn a_delete_that_finds_nothing_says_nothing_was_deleted() {
    let server = support::mock_with_token().await;
    Mock::given(method("DELETE"))
        .and(path("/v2/customers/gone"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"Title": "Not found", "CorrelationId": "c-1"})),
        )
        .mount(&server)
        .await;

    support::bin(&server)
        .args(["customers", "delete", "gone", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Deleted").not())
        .stdout(predicate::str::contains("nothing was deleted"));
}

/// Both answer 200 with **no body**, so they confirm rather than print, and
/// the id in `quiet` mode can only come from the request.
#[tokio::test]
async fn a_bodyless_write_confirms_with_the_id_from_the_request() {
    let server = support::mock_with_token().await;
    Mock::given(method("DELETE"))
        .and(path("/v2/customers/cus_9"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    support::bin(&server)
        .args(["--output", "quiet", "customers", "delete", "cus_9", "--yes"])
        .assert()
        .success()
        .stdout("cus_9\n");

    let out = support::bin(&server)
        .args(["--output", "json", "customers", "delete", "cus_9", "--yes"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "customer");
    assert_eq!(v["data"]["customerId"], "cus_9");
    assert_eq!(v["data"]["deleted"], true);
}

/// A PATCH with nothing in it is a round trip that cannot change anything.
#[tokio::test]
async fn update_with_no_fields_is_refused_before_the_wire() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["customers", "update", "cus_1"])
        .assert()
        .code(3);
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
    );
}

/// A PATCH must be able to clear a flag, so the two booleans take an explicit
/// value here where `create`'s bare switches cannot express `false`.
#[tokio::test]
async fn update_can_set_a_boolean_to_false() {
    let server = support::mock_with_token().await;
    Mock::given(method("PATCH"))
        .and(path("/v2/customers/cus_1"))
        .and(body_json(serde_json::json!({"hasSmsConsent": false})))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    support::bin(&server)
        .args(["customers", "update", "cus_1", "--sms-consent", "false"])
        .assert()
        .success();
}

/// Parity evidence for v1's single `customers list --search`: v2 declares
/// four named filters instead, and all four have to be reachable or the
/// replacement is a regression.
#[tokio::test]
async fn list_sends_every_named_filter_v1_search_replaces() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/customers"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"items": [], "pageInfo": {"hasMore": false}})),
        )
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "customers",
            "list",
            "--full-name",
            "Ada Lovelace",
            "--email",
            "ada@example.com",
            "--company-name",
            "Analytical",
            "--mobile",
            "+14155552309",
            "--created-from",
            "2026-01-01T00:00:00Z",
            "--created-to",
            "2026-12-31T23:59:59Z",
        ])
        .assert()
        .success();

    let reqs = server.received_requests().await.unwrap();
    let sent = reqs
        .iter()
        .find(|r| r.url.path() == "/v2/customers")
        .expect("no list request");
    let pairs: std::collections::BTreeMap<String, String> = sent
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(pairs["fullName"], "Ada Lovelace");
    assert_eq!(pairs["email"], "ada@example.com");
    assert_eq!(pairs["companyName"], "Analytical");
    assert_eq!(pairs["mobilePhoneNumber"], "+14155552309");
    assert_eq!(pairs["createdFrom"], "2026-01-01T00:00:00Z");
    assert_eq!(pairs["createdTo"], "2026-12-31T23:59:59Z");
}

/// Parity evidence for v1's `customers update`: every field it could change
/// is still reachable, under v2's wire names — `companyName` rather than
/// `company`, and `stateCode`/`countryCode` rather than v1's two id flags.
#[tokio::test]
async fn update_sends_every_v1_field_under_its_v2_wire_name() {
    let server = support::mock_with_token().await;
    Mock::given(method("PATCH"))
        .and(path("/v2/customers/cus_1"))
        .and(body_json(serde_json::json!({
            "firstName": "Ada",
            "lastName": "Lovelace",
            "companyName": "Analytical",
            "email": "ada@example.com",
            "mobilePhoneNumber": "+14155552309",
            "billingAddress": {
                "addressLine1": "1 Main St",
                "addressLine2": "Suite 2",
                "city": "Austin",
                "stateCode": "TX",
                "postalCode": "78701",
                "countryCode": "US"}
        })))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "customers",
            "update",
            "cus_1",
            "--first-name",
            "Ada",
            "--last-name",
            "Lovelace",
            "--company",
            "Analytical",
            "--email",
            "ada@example.com",
            "--mobile",
            "+14155552309",
            "--billing-line1",
            "1 Main St",
            "--billing-line2",
            "Suite 2",
            "--billing-city",
            "Austin",
            "--billing-state",
            "TX",
            "--billing-postal-code",
            "78701",
            "--billing-country",
            "US",
        ])
        .assert()
        .success();
}

/// The refusal precedes credential resolution, so a destructive command is
/// refused for want of `--yes` rather than for want of a login — the order v1
/// fixed, and the one that keeps the gate meaningful on an unconfigured
/// machine.
#[test]
fn delete_without_yes_is_refused_before_credentials_are_resolved() {
    let out = support::bin_without_credentials()
        .args(["customers", "delete", "cus_1"])
        .assert()
        .code(3)
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("requires --yes to confirm"), "{stderr}");
}

/// A bodyless write says what it did, in a sentence, because there is no
/// record to print in its place.
#[tokio::test]
async fn the_confirmation_lines_are_sentences() {
    const CUS: &str = "8db2ff47-b143-4adb-ab58-a11111111111";
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-patch-customers-customerId",
        "changed fields",
    )
    .await;
    Mock::given(method("DELETE"))
        .and(path("/v2/customers/cus_9"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "customers",
            "update",
            CUS,
            "--company",
            "Analytical Engines",
            "--email",
            "ada@example.com",
            "--billing-city",
            "Austin",
            "--billing-state",
            "TX",
        ])
        .assert()
        .success()
        .stdout(format!("Updated customer {CUS}.\n"));

    support::bin(&server)
        .args(["customers", "delete", "cus_9", "--yes"])
        .assert()
        .success()
        .stdout("Deleted customer cus_9.\n");
}

/// The bundle declares a body for this operation, so a success carrying none
/// is a response the caller cannot be shown — not an empty resource.
#[tokio::test]
async fn a_create_answered_with_no_body_is_a_decode_error() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/customers"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "customers",
            "create",
            "--first-name",
            "Ada",
            "--last-name",
            "Lovelace",
        ])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "decode", "{v}");
}

/// An identifier is interpolated into the request path, so an empty one
/// addresses the collection rather than a member — a delete that would reach
/// a different operation than the one the caller named.
#[tokio::test]
async fn an_empty_identifier_is_refused_before_the_wire() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["customers", "delete", "", "--yes"])
        .assert()
        .code(3);
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "a refused identifier still reached the network"
    );
}

/// `/`, `?` and `#` end a path segment, so an identifier carrying one rewrites
/// the request line into an operation the caller did not ask for.
#[tokio::test]
async fn an_identifier_carrying_a_path_separator_is_refused_before_the_wire() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["customers", "get", "abc/../x"])
        .assert()
        .code(3);
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "a refused identifier still reached the network"
    );
}

/// A dot segment is removed while the URL is parsed, so `.` would delete
/// against the collection and `..` against the API root. The refusal is a
/// `client` envelope on stdout, and nothing is sent.
#[tokio::test]
async fn a_dot_segment_identifier_is_refused_with_a_client_envelope() {
    let server = support::mock_with_token().await;
    let out = support::bin(&server)
        .args(["--output", "json", "customers", "delete", ".", "--yes"])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "client", "{v}");
    assert_eq!(
        v["message"],
        "\".\" is a path segment rather than an identifier: URL parsing resolves \
         it away, so the request would address another resource"
    );
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "a refused identifier still reached the network"
    );
}

/// A backslash separates path segments for the URL parser as a `/` does, and
/// the dot segments it separates are resolved away with it — so
/// `..\api-keys\other` on a delete would address another group's resource.
#[tokio::test]
async fn an_identifier_carrying_a_backslash_is_refused_before_the_wire() {
    let server = support::mock_with_token().await;
    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "customers",
            "delete",
            r"..\api-keys\other",
            "--yes",
        ])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "client", "{v}");
    assert_eq!(
        v["message"],
        r#"an identifier cannot contain '/', '\', '?', '#', '%' or whitespace: "..\\api-keys\\other""#
    );
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "a refused identifier still reached the network"
    );
}

/// **`--all` owns the pagination it walks with.** The batch size travels with
/// the walk and not in the filters as well: two `pageSize` pairs on one
/// request leave which of them the API reads up to the API.
#[tokio::test]
async fn list_all_sends_each_pagination_parameter_once() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/customers"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"customerId": "cus_1"}],
            "pageInfo": {"pageIndex": 0, "pageSize": 5, "hasMore": false}})))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "customers",
            "list",
            "--all",
            "--page-size",
            "5",
            "--email",
            "ada@example.test",
        ])
        .assert()
        .success();

    let reqs = server.received_requests().await.unwrap();
    let first = reqs
        .iter()
        .find(|r| r.url.path() == "/v2/customers")
        .expect("no list request was sent");
    let mut pairs: Vec<(String, String)> = first
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    pairs.sort();
    assert_eq!(
        pairs,
        vec![
            ("email".to_string(), "ada@example.test".to_string()),
            ("pageIndex".to_string(), "0".to_string()),
            ("pageSize".to_string(), "5".to_string()),
        ]
    );
}

/// A JSON `null` is not the resource that was read. Rendering it as
/// `"data": null` reports a success with nothing in it, which a caller has no
/// way to tell from a customer whose fields are all empty.
#[tokio::test]
async fn a_read_answered_with_a_null_body_is_a_decode_error() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/customers/cus_1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::Value::Null))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "customers", "get", "cus_1"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "decode", "{v}");
}

/// A `hasMore` the walk cannot read must not be taken for "no more": the
/// collection would be reported complete having been read to its first page.
#[tokio::test]
async fn list_all_refuses_a_has_more_that_is_not_a_boolean() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/customers"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"customerId": "cus_1"}],
            "pageInfo": {"pageIndex": 0, "hasMore": "true"}})))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "customers", "list", "--all"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "decode", "{v}");
}

/// **A page that is not the last says so in `table` mode.** The hint goes to
/// stderr so stdout stays the table; `json` carries `meta.page_info` and needs
/// no hint.
#[tokio::test]
async fn a_list_with_more_pages_says_so_on_stderr_in_table_mode() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/customers"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"customerId": "cus_1", "firstName": "Ada"}],
            "pageInfo": {"pageIndex": 0, "pageSize": 1, "totalItems": 386,
                         "totalPages": 386, "hasMore": true}
        })))
        .mount(&server)
        .await;

    let table = support::bin(&server)
        .args(["customers", "list", "--page-size", "1"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&table.get_output().stdout).to_string();
    let stderr = String::from_utf8_lossy(&table.get_output().stderr).to_string();
    assert!(!stdout.contains("--all"), "{stdout}");
    assert!(stderr.contains("1 of 386"), "{stderr}");
    assert!(stderr.contains("--page-index 1"), "{stderr}");
    assert!(stderr.contains("--all"), "{stderr}");

    let json = support::bin(&server)
        .args(["--output", "json", "customers", "list", "--page-size", "1"])
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&json.get_output().stderr).to_string();
    assert!(!stderr.contains("--all"), "{stderr}");
}
