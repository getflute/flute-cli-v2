//! `users`: list, schedule-as, split-profiles.
//!
//! `schedule-as` and `split-profiles` list what a partner user can assign to
//! a merchant application, and need a partner token. Neither is paginated:
//! each answers every item at once and declares no `pageInfo`.

use crate::Ctx;
use crate::api::ApiPath;
use crate::cli::common::{self, PaginationArgs};
use crate::cli::render::{self, Cell, Column, Resource};
use anyhow::Result;
use reqwest::Method;

/// The kind of account a user belongs to.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
pub enum UserType {
    Partner,
    Merchant,
    Applicant,
}

#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug)]
pub enum UsersCommand {
    /// List the users the credentials can access (GET /v2/users).
    List(ListUsersArgs),
    /// List a partner user's Schedule A pricing schedules
    /// (GET /v2/users/{userId}/schedule-as).
    ScheduleAs {
        /// Partner user UUID (positional).
        user_id: String,
    },
    /// List a partner user's Split Profiles
    /// (GET /v2/users/{userId}/split-profiles).
    SplitProfiles {
        /// Partner user UUID (positional).
        user_id: String,
    },
}

#[derive(clap::Args, Debug, Default)]
pub struct ListUsersArgs {
    #[command(flatten)]
    pub pagination: PaginationArgs,
    /// Filter by account type.
    #[arg(long, value_enum)]
    pub user_type: Option<UserType>,
    /// Find one user by email address.
    #[arg(long)]
    pub email: Option<String>,
}

/// The `GET /v2/users` query, omitting every absent flag.
pub fn build_list_users_query(args: &ListUsersArgs) -> Result<Vec<(&'static str, String)>> {
    args.pagination.validate()?;
    let mut query = args.pagination.query();
    if let Some(v) = args.user_type {
        query.push(("userType", common::wire(v)));
    }
    common::push_str(&mut query, "email", &args.email);
    Ok(query)
}

pub static USER: Resource = Resource {
    object: "user",
    object_list: "user_list",
    id: "/id",
    detail: &["/id", "/email"],
    columns: &[
        Column {
            header: "ID",
            width: 36,
            cell: Cell::Path("/id"),
        },
        Column {
            header: "EMAIL",
            width: 40,
            cell: Cell::Path("/email"),
        },
    ],
    amounts: &[],
    yes_no: &[],
};

pub static SCHEDULE_A: Resource = Resource {
    object: "schedule_a",
    object_list: "schedule_a_list",
    id: "/id",
    detail: &["/id", "/name", "/processorId"],
    columns: &[
        Column {
            header: "ID",
            width: 36,
            cell: Cell::Path("/id"),
        },
        Column {
            header: "NAME",
            width: 32,
            cell: Cell::Path("/name"),
        },
        Column {
            header: "PROCESSOR",
            width: 10,
            cell: Cell::Path("/processorId"),
        },
    ],
    amounts: &[],
    yes_no: &[],
};

pub static SPLIT_PROFILE: Resource = Resource {
    object: "split_profile",
    object_list: "split_profile_list",
    id: "/id",
    detail: &["/id", "/name", "/isHighRisk", "/isDefault"],
    columns: &[
        Column {
            header: "ID",
            width: 36,
            cell: Cell::Path("/id"),
        },
        Column {
            header: "NAME",
            width: 32,
            cell: Cell::Path("/name"),
        },
        Column {
            header: "HIGH RISK",
            width: 10,
            cell: Cell::Path("/isHighRisk"),
        },
        Column {
            header: "DEFAULT",
            width: 8,
            cell: Cell::Path("/isDefault"),
        },
    ],
    amounts: &[],
    yes_no: &["/isHighRisk", "/isDefault"],
};

/// One partner user's unpaginated collection.
async fn per_user(ctx: &Ctx, resource: &Resource, user_id: &str, what: &'static str) -> Result<()> {
    let resp = ctx
        .api
        .request(
            Method::GET,
            ApiPath::from("/v2/users").id(user_id)?.seg(what),
            &[],
            None,
        )
        .await?;
    let body = common::body_of(resp.body)?;
    render::page(
        ctx,
        resource,
        &common::items_of(&body, "items")?,
        None,
        resp.correlation_id,
    )
}

pub async fn dispatch(ctx: &Ctx, command: UsersCommand) -> Result<()> {
    match command {
        UsersCommand::List(args) => {
            let query = build_list_users_query(&args)?;
            common::list(ctx, &USER, "/v2/users", &query, &args.pagination).await
        }
        UsersCommand::ScheduleAs { user_id } => {
            per_user(ctx, &SCHEDULE_A, &user_id, "schedule-as").await
        }
        UsersCommand::SplitProfiles { user_id } => {
            per_user(ctx, &SPLIT_PROFILE, &user_id, "split-profiles").await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unfiltered_user_list_sends_nothing() {
        assert!(
            build_list_users_query(&ListUsersArgs::default())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn every_user_type_has_its_wire_spelling() {
        for (t, wire) in [
            (UserType::Partner, "Partner"),
            (UserType::Merchant, "Merchant"),
            (UserType::Applicant, "Applicant"),
        ] {
            assert_eq!(common::wire(t), wire);
        }
    }
}
