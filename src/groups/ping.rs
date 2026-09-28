//! `ping`.

use crate::Ctx;
use crate::cli::output::{Envelope, OutputFormat};
use anyhow::Result;
use reqwest::Method;

pub async fn dispatch(ctx: &Ctx) -> Result<()> {
    let correlation_id = ctx
        .api
        .request(Method::GET, "/v2/ping", &[], None)
        .await?
        .correlation_id;

    // 200 with no body is this endpoint's documented success, so the envelope
    // reports reachability rather than a payload.
    let data = serde_json::json!({"reachable": true});
    match ctx.output {
        OutputFormat::Json => {
            let env = Envelope::new("ping", data, &ctx.profile.name, correlation_id, None);
            println!("{}", serde_json::to_string_pretty(&env)?);
        }
        OutputFormat::Quiet => println!("ok"),
        OutputFormat::Table => println!("ping  ok  (profile={})", ctx.profile.name),
    }
    Ok(())
}
