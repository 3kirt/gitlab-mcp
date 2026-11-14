mod client;
mod commands;
mod config;
mod credentials;
#[cfg(test)]
mod test_util;
mod tools;

use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Context;
use clap::{Parser, Subcommand};
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
        GPG-ENCRYPTED TOKEN (pass)\n\
        \n\
        Instead of a plaintext \"token\", the config can name an entry in a pass-compatible\n\
        password store (~/.password-store, or $PASSWORD_STORE_DIR):\n  {\n    \"url\":        \"https://gitlab.com\",\n    \"pass_entry\": \"gitlab-mcp/gitlab.com\"\n  }\n\
        `gitlab-mcp login` verifies a token, encrypts it with gpg, and writes this config.\n\
        The server decrypts it on first use without ever prompting: if gpg-agent has not\n\
        cached the passphrase, tool calls fail with a message asking you to run\n\
        `gitlab-mcp unlock`, and succeed once you have. Set \"gpg\" in the config to the\n\
        full path of gpg if your MCP client launches servers with a minimal PATH.\n\
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
    #[command(subcommand)]
    command: Option<Command>,

    /// Path to the configuration file (default: ~/.gitlab_mcp.json)
    #[arg(long, global = true)]
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

/// Without a subcommand, the MCP server runs over stdio.
#[derive(Subcommand)]
enum Command {
    /// Verify a personal access token and store it GPG-encrypted in a pass store
    Login(commands::LoginArgs),
    /// Delete the stored token from the pass store
    Logout(commands::EntryArgs),
    /// Cache the token's GPG passphrase in gpg-agent (prompts if needed)
    Unlock,
    /// Show where the token comes from and whether its GPG key is unlocked
    Status,
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
            let mut opts = OpenOptions::new();
            opts.create(true).append(true);
            // The trace can hold private GitLab content (URLs, GraphQL variables,
            // error bodies), so restrict it to the owner — matching the
            // world-readable rejection config.rs applies to the credentials file.
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            let file = opts
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

    let config_path = args.config.as_deref();
    match args.command {
        Some(Command::Login(login)) => return commands::login(config_path, login).await,
        Some(Command::Logout(entry)) => return commands::logout(config_path, &entry),
        Some(Command::Unlock) => return commands::unlock(config_path).await,
        Some(Command::Status) => return commands::status(config_path).await,
        None => {}
    }

    let cfg = config::Config::load(config_path)?;
    let url = cfg.resolve_url()?;
    let source = cfg.resolve_token()?;

    info!(mode = "stdio", gitlab_url = %url, token_source = %source.describe(), "starting gitlab-mcp");
    let client = match source {
        config::TokenSource::Env(token) | config::TokenSource::File(token) => {
            client::GitlabClient::new(url, token)?
        }
        // Decrypted on first use, so a locked GPG key doesn't stop startup.
        config::TokenSource::Pass(entry) => client::GitlabClient::with_pass_entry(url, entry)?,
    };
    let server = tools::GitlabMcpServer::new_stdio(client);
    server
        // Non-blocking pipe I/O on Linux when the client launches us over
        // FIFOs; identical to `stdio()` everywhere else.
        .serve(rmcp::transport::io::stdio_pipes())
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
