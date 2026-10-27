//! `pricing-templates`: list.
//!
//! The ids go in a merchant application's `pricingTemplateIds`. Not paginated:
//! every template comes back at once and the response declares no `pageInfo`.
//! Needs a partner token.

use crate::Ctx;
use crate::cli::common;
use crate::cli::render::{self, Cell, Column, Resource};
use anyhow::Result;
use reqwest::Method;

#[derive(clap::Subcommand, Debug)]
pub enum PricingTemplatesCommand {
    /// List the partner's pricing templates (GET /v2/pricing-templates).
    List,
}

pub static PRICING_TEMPLATE: Resource = Resource {
    object: "pricing_template",
    object_list: "pricing_template_list",
    id: "/id",
    detail: &["/id", "/name", "/displayName", "/isDefault"],
    columns: &[
        Column {
            header: "ID",
            width: 36,
            cell: Cell::Path("/id"),
        },
        Column {
            header: "NAME",
            width: 24,
            cell: Cell::Path("/name"),
        },
        Column {
            header: "DISPLAY NAME",
            width: 28,
            cell: Cell::Path("/displayName"),
        },
        Column {
            header: "DEFAULT",
            width: 8,
            cell: Cell::Path("/isDefault"),
        },
    ],
    amounts: &[],
    yes_no: &["/isDefault"],
};

pub async fn dispatch(ctx: &Ctx, command: PricingTemplatesCommand) -> Result<()> {
    match command {
        PricingTemplatesCommand::List => {
            let resp = ctx
                .api
                .request(Method::GET, "/v2/pricing-templates", &[], None)
                .await?;
            let body = common::body_of(resp.body)?;
            render::page(
                ctx,
                &PRICING_TEMPLATE,
                &common::items_of(&body, "items")?,
                None,
                resp.correlation_id,
            )
        }
    }
}
