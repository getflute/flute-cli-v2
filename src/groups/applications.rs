//! `applications`: create and list merchant applications.
//!
//! **`create` takes most of its body from a file.** The request declares well
//! over a hundred fields — owners, addresses, bank details, volumes — and
//! every one the partner leaves out stays empty for the merchant to fill in.
//! The seven required fields are flags; `--file` carries the rest as JSON,
//! which also keeps Social Security and bank account numbers out of argv and
//! shell history. A flag wins over the file's value for the same field.
//!
//! Both operations need a partner token.

use crate::Ctx;
use crate::cli::common::{self, IdempotencyArgs, PaginationArgs};
use crate::cli::render::{self, Cell, Column, Resource};
use anyhow::{Context, Result};
use reqwest::Method;
use serde_json::{Map, Value};

/// How the merchant receives the application.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
pub enum DeliveryMethod {
    /// Flute emails the merchant the link.
    Email,
    /// The partner delivers the returned link.
    PartnerDirect,
    /// The partner embeds the returned link in a frame.
    Iframe,
}

/// Where an application stands in onboarding.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
pub enum ApplicationStatus {
    InProgress,
    InReview,
    InformationNeeded,
    Rejected,
    AutoDeclined,
    Active,
    Cancelled,
}

#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug)]
pub enum ApplicationsCommand {
    /// Invite a merchant to a pre-filled application (POST /v2/applications).
    ///
    /// The seven flags are the required fields. Put anything else the
    /// application should carry in a JSON file passed with `--file`; a flag
    /// wins over the file's value for the same field.
    Create(CreateApplicationArgs),
    /// List merchant applications, newest first (GET /v2/applications).
    List(ListApplicationsArgs),
}

#[derive(clap::Args, Debug, Clone)]
pub struct CreateApplicationArgs {
    /// The merchant's email address (required).
    #[arg(long)]
    pub email: String,
    /// How the merchant receives the application (required).
    #[arg(long, value_enum)]
    pub delivery_method: DeliveryMethod,
    /// The partner user the application is attributed to (required).
    /// `users list` finds one.
    #[arg(long)]
    pub partner_user_id: String,
    /// Processor id (required), 1 to 2147483647.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=i64::from(i32::MAX)))]
    pub processor_id: u32,
    /// Schedule A pricing schedule of the partner user (required).
    /// `users schedule-as <USER_ID>` lists them.
    #[arg(long)]
    pub schedule_a_id: String,
    /// Split Profile of the partner user (required).
    /// `users split-profiles <USER_ID>` lists them.
    #[arg(long)]
    pub split_profile_id: String,
    /// The business's DBA name (required).
    #[arg(long)]
    pub dba_name: String,
    /// The rest of the application as one JSON object, or `-` for stdin.
    #[arg(long, value_name = "PATH")]
    pub file: Option<String>,
    #[command(flatten)]
    pub idempotency: IdempotencyArgs,
}

#[derive(clap::Args, Debug, Default)]
pub struct ListApplicationsArgs {
    #[command(flatten)]
    pub pagination: PaginationArgs,
    /// Filter by onboarding status.
    #[arg(long = "status", value_enum, value_name = "APPLICATION_STATUS")]
    pub application_status: Option<ApplicationStatus>,
    /// Filter to one application.
    #[arg(long)]
    pub application_id: Option<String>,
    /// Created on or after this date (YYYY-MM-DD).
    #[arg(long)]
    pub created_from: Option<String>,
    /// Created before this date (YYYY-MM-DD); the date itself is excluded, so
    /// June is `--created-from 2026-06-01 --created-to 2026-07-01`.
    #[arg(long)]
    pub created_to: Option<String>,
}

/// The create body: the file's object, then each flag over it.
///
/// `file` is the file's text, already read.
pub fn build_create_application_body(
    args: &CreateApplicationArgs,
    file: Option<&str>,
) -> Result<Value> {
    for (flag, value) in [
        ("--email", &args.email),
        ("--partner-user-id", &args.partner_user_id),
        ("--schedule-a-id", &args.schedule_a_id),
        ("--split-profile-id", &args.split_profile_id),
        ("--dba-name", &args.dba_name),
    ] {
        common::reject_empty_id(flag, value)?;
    }
    let mut body = match file {
        None => Map::new(),
        Some(text) => match serde_json::from_str(text).context("--file is not valid JSON")? {
            Value::Object(map) => map,
            _ => anyhow::bail!("--file must hold one JSON object: the application body"),
        },
    };
    body.insert(
        "deliveryMethod".into(),
        Value::String(common::wire(args.delivery_method)),
    );
    for (key, value) in [
        ("email", &args.email),
        ("partnerUserId", &args.partner_user_id),
        ("scheduleAId", &args.schedule_a_id),
        ("splitProfileId", &args.split_profile_id),
    ] {
        body.insert(key.into(), Value::String(value.clone()));
    }
    body.insert("processorId".into(), Value::from(args.processor_id));
    let business = body
        .entry("businessInformation")
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(business) = business else {
        anyhow::bail!("--file's businessInformation must be a JSON object");
    };
    business.insert("dbaName".into(), Value::String(args.dba_name.clone()));
    Ok(Value::Object(body))
}

/// The `GET /v2/applications` query, omitting every absent flag.
pub fn build_list_applications_query(
    args: &ListApplicationsArgs,
) -> Result<Vec<(&'static str, String)>> {
    args.pagination.validate()?;
    let mut query = args.pagination.query();
    if let Some(v) = args.application_status {
        query.push(("applicationStatus", common::wire(v)));
    }
    common::push_id(
        &mut query,
        "--application-id",
        "applicationId",
        &args.application_id,
    )?;
    common::push_str(&mut query, "createdFrom", &args.created_from);
    common::push_str(&mut query, "createdTo", &args.created_to);
    Ok(query)
}

/// The file's text: a path, or stdin for `-`.
fn read_file(path: &str) -> Result<String> {
    if path == "-" {
        return std::io::read_to_string(std::io::stdin()).context("--file -: cannot read stdin");
    }
    std::fs::read_to_string(path).with_context(|| format!("--file {path}: cannot read it"))
}

/// One application as `list` reports it.
pub static APPLICATION: Resource = Resource {
    object: "application",
    object_list: "application_list",
    id: "/id",
    detail: &[
        "/id",
        "/applicationNumber",
        "/businessName",
        "/applicationStatus",
        "/applicationStatusStartedAt",
        "/merchantId",
        "/mid",
        "/createdOn",
    ],
    columns: &[
        Column {
            header: "ID",
            width: 36,
            cell: Cell::Path("/id"),
        },
        Column {
            header: "NUMBER",
            width: 8,
            cell: Cell::Path("/applicationNumber"),
        },
        Column {
            header: "BUSINESS",
            width: 28,
            cell: Cell::Path("/businessName"),
        },
        Column {
            header: "STATUS",
            width: 18,
            cell: Cell::Path("/applicationStatus"),
        },
        Column {
            header: "CREATED",
            width: 24,
            cell: Cell::Path("/createdOn"),
        },
    ],
    amounts: &[],
    yes_no: &[],
};

/// The invitation `create` answers with: the new application's id and the
/// link the merchant completes it at.
pub static APPLICATION_CREATED: Resource = Resource {
    object: "application",
    object_list: "application_list",
    id: "/applicationId",
    detail: &[
        "/applicationId",
        "/applicationUrl",
        "/deliveryMethod",
        "/createdOn",
    ],
    columns: &[
        Column {
            header: "ID",
            width: 36,
            cell: Cell::Path("/applicationId"),
        },
        Column {
            header: "DELIVERY",
            width: 14,
            cell: Cell::Path("/deliveryMethod"),
        },
        Column {
            header: "URL",
            width: 60,
            cell: Cell::Path("/applicationUrl"),
        },
    ],
    amounts: &[],
    yes_no: &[],
};

pub async fn dispatch(ctx: &Ctx, command: ApplicationsCommand) -> Result<()> {
    match command {
        ApplicationsCommand::Create(args) => {
            let file = args.file.as_deref().map(read_file).transpose()?;
            let body = build_create_application_body(&args, file.as_deref())?;
            let resp = ctx
                .api
                .request_keyed(
                    Method::POST,
                    "/v2/applications",
                    Some(body),
                    args.idempotency.key()?,
                )
                .await?;
            render::one(
                ctx,
                &APPLICATION_CREATED,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
        ApplicationsCommand::List(args) => {
            let query = build_list_applications_query(&args)?;
            common::list(
                ctx,
                &APPLICATION,
                "/v2/applications",
                &query,
                &args.pagination,
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_args() -> CreateApplicationArgs {
        CreateApplicationArgs {
            email: "marcus@example.com".into(),
            delivery_method: DeliveryMethod::PartnerDirect,
            partner_user_id: "pu-1".into(),
            processor_id: 7,
            schedule_a_id: "sa-1".into(),
            split_profile_id: "sp-1".into(),
            dba_name: "Corner Coffee".into(),
            file: None,
            idempotency: IdempotencyArgs::default(),
        }
    }

    #[test]
    fn the_required_flags_reach_the_body_under_their_wire_names() {
        assert_eq!(
            build_create_application_body(&create_args(), None).unwrap(),
            serde_json::json!({
                "deliveryMethod": "PartnerDirect",
                "email": "marcus@example.com",
                "partnerUserId": "pu-1",
                "processorId": 7,
                "scheduleAId": "sa-1",
                "splitProfileId": "sp-1",
                "businessInformation": {"dbaName": "Corner Coffee"}})
        );
    }

    /// The file's nested object keeps its own fields beside the flag's.
    #[test]
    fn a_flag_wins_over_the_file_and_the_files_other_fields_are_kept() {
        let body = build_create_application_body(
            &create_args(),
            Some(r#"{"email": "x@example.com", "businessInformation": {"dbaName": "Old", "legalName": "CC LLC"}, "owner1": {"ssn": "123456789"}}"#),
        )
        .unwrap();
        assert_eq!(body["email"], "marcus@example.com");
        assert_eq!(body["businessInformation"]["dbaName"], "Corner Coffee");
        assert_eq!(body["businessInformation"]["legalName"], "CC LLC");
        assert_eq!(body["owner1"]["ssn"], "123456789");
    }

    #[test]
    fn every_delivery_method_has_its_wire_spelling() {
        for (method, wire) in [
            (DeliveryMethod::Email, "Email"),
            (DeliveryMethod::PartnerDirect, "PartnerDirect"),
            (DeliveryMethod::Iframe, "Iframe"),
        ] {
            assert_eq!(common::wire(method), wire);
        }
    }

    #[test]
    fn an_unfiltered_application_list_sends_nothing() {
        assert!(
            build_list_applications_query(&ListApplicationsArgs::default())
                .unwrap()
                .is_empty()
        );
    }
}
