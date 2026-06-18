mod client;
mod config;
#[cfg(test)]
mod test_util;
mod tools;

use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Context;
use clap::Parser;
use rmcp::ServiceExt;
use tracing::info;
use tracing_subscriber::fmt::writer::BoxMakeWriter;
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
        HTTPS is required for all non-localhost URLs.\n\
        \n\
        DEBUGGING\n\
        \n\
        Pass --debug to log every GitLab request (method + URL, GraphQL query and\n\
        variables) and full error response bodies. Output is JSON on stderr unless\n\
        --log-file is given, which is the reliable way to capture a trace when an MCP\n\
        client spawns the server (its stderr is otherwise hard to reach). RUST_LOG, if\n\
        set, overrides --debug for fine-grained control (e.g. RUST_LOG=gitlab_mcp=trace).",
    version
)]
struct Args {
    /// Path to the configuration file (default: ~/.gitlab_mcp.json)
    #[arg(long)]
    config: Option<PathBuf>,

    /// Log every GitLab request and full error bodies (sets gitlab_mcp=debug
    /// unless RUST_LOG is set). The token is never logged.
    #[arg(long)]
    debug: bool,

    /// Write log output to this file instead of stderr (append + create).
    /// Use this to capture a debug trace when the server is spawned by an MCP client.
    #[arg(long, value_name = "PATH")]
    log_file: Option<PathBuf>,
}

/// Decide the tracing filter directive. An explicit, non-empty `RUST_LOG`
/// always wins; otherwise `--debug` raises this crate to `debug`, and the
/// default stays `info`.
fn log_directive(rust_log: Option<&str>, debug: bool) -> String {
    match rust_log {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ if debug => "gitlab_mcp=debug".to_string(),
        _ => "info".to_string(),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let directive = log_directive(std::env::var("RUST_LOG").ok().as_deref(), args.debug);
    let writer = match &args.log_file {
        Some(path) => {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .with_context(|| format!("opening log file {}", path.display()))?;
            BoxMakeWriter::new(Mutex::new(file))
        }
        None => BoxMakeWriter::new(std::io::stderr),
    };
    tracing_subscriber::registry()
        .with(EnvFilter::new(directive))
        .with(fmt::layer().json().with_writer(writer))
        .init();

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

#[cfg(test)]
mod tests {
    use super::log_directive;

    #[test]
    fn rust_log_takes_precedence_over_debug() {
        assert_eq!(
            log_directive(Some("gitlab_mcp=trace"), true),
            "gitlab_mcp=trace"
        );
        assert_eq!(log_directive(Some("warn"), false), "warn");
    }

    #[test]
    fn debug_flag_raises_this_crate_when_no_rust_log() {
        assert_eq!(log_directive(None, true), "gitlab_mcp=debug");
        // Empty/whitespace RUST_LOG is treated as unset.
        assert_eq!(log_directive(Some(""), true), "gitlab_mcp=debug");
        assert_eq!(log_directive(Some("   "), true), "gitlab_mcp=debug");
    }

    #[test]
    fn default_is_info() {
        assert_eq!(log_directive(None, false), "info");
        assert_eq!(log_directive(Some(""), false), "info");
    }
}
