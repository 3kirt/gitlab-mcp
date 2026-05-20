mod client;
mod config;
mod tools;

use std::path::PathBuf;

use clap::Parser;
use rmcp::ServiceExt;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[derive(Parser)]
#[command(
    name = "gitlab-mcp",
    about = "MCP server for the GitLab API",
    long_about = "MCP server that exposes the GitLab API as tools for Claude and other MCP clients.\n\
        \n\
        Runs over stdio transport — add it to your MCP client config and it will be launched\n\
        automatically.\n\
        \n\
        CONFIGURATION\n\
        \n\
        Credentials are loaded from environment variables or a JSON config file.\n\
        Environment variables take precedence over the config file.\n\
        \n\
        Environment variables:\n\
          GITLAB_URL    Base URL of your GitLab instance (e.g. https://gitlab.com)\n\
          GITLAB_TOKEN  Personal access token with api scope\n\
        \n\
        Config file (~/.gitlab_mcp.json):\n  {\n    \"url\":   \"https://gitlab.com\",\n    \"token\": \"glpat-xxxxxxxxxxxxxxxxxxxx\"\n  }\n\
        \n\
        The config file must not be world-readable (chmod o-r ~/.gitlab_mcp.json).\n\
        HTTPS is required for all non-localhost URLs.",
    version
)]
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
