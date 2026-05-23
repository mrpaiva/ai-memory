//! `ai-memory embed` — thin HTTP client for the M9 embedding backfill.

use anyhow::Result;
use serde::Serialize;

use crate::cli::EmbedArgs;
use crate::config::Config;
use crate::http_client::{ServerEndpoint, post_json};

/// Request sent to `POST /admin/embed`.
#[derive(Serialize)]
struct EmbedRequest {
    workspace: String,
    project: String,
    reembed: bool,
}

/// Run the `embed` subcommand.
///
/// # Errors
/// Returns an error if the server is unreachable or returns a non-2xx
/// response.
pub async fn run(_config: &Config, args: EmbedArgs) -> Result<()> {
    let endpoint = ServerEndpoint::from_env();
    let report: serde_json::Value = post_json(
        &endpoint,
        "/admin/embed",
        &EmbedRequest {
            workspace: args.workspace,
            project: args.project,
            // The CLI flag was historically named `force`; the server
            // field is `reembed` — map them here.
            reembed: args.force,
        },
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
