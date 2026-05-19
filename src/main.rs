mod client;
mod config;
mod tools;

use std::path::PathBuf;

use clap::Parser;
use rmcp::ServiceExt;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[derive(Parser)]
#[command(name = "gitlab-mcp", about = "MCP server for the GitLab API")]
struct Args {
    /// Path to the configuration file (default: ~/.gitlab_mcp.json)
    #[arg(long)]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt::layer().json().with_writer(std::io::stderr))
        .init();

    let args = Args::parse();

    let cfg = config::Config::load(args.config.as_deref())?;
    let url = cfg.resolve_url()?;
    let token = cfg.resolve_token()?;

    info!(mode = "stdio", gitlab_url = %url, "starting gitlab-mcp");
    let server = tools::GitlabMcpServer::new_stdio(url, token)?;
    server
        .serve(rmcp::transport::io::stdio())
        .await?
        .waiting()
        .await?;

    Ok(())
}
