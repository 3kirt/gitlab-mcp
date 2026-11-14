//! `login` / `logout` / `unlock` / `status`: managing the GPG-encrypted token
//! in a `pass`-compatible store. These run in the user's terminal, so (unlike
//! the server) they may prompt. All output goes to stderr, matching the
//! server's rule that stdout is reserved.

use std::io::{BufRead, IsTerminal};
use std::path::Path;

use anyhow::{Context, anyhow, bail};

use crate::client::GitlabClient;
use crate::config::{self, Config, TokenSource};
use crate::credentials::PassError;

#[derive(clap::Args)]
pub struct LoginArgs {
    /// GitLab base URL (default: GITLAB_URL, then "url" in the config file)
    #[arg(long)]
    url: Option<String>,

    /// Password-store entry to write (default: "pass_entry" in the config
    /// file, else gitlab-mcp/<host>)
    #[arg(long)]
    entry: Option<String>,

    /// GPG key to encrypt to when the store has no .gpg-id yet (writes it,
    /// like `pass init`)
    #[arg(long, value_name = "KEY_ID")]
    gpg_id: Option<String>,

    /// Read the token from stdin instead of prompting
    #[arg(long)]
    token_stdin: bool,
}

#[derive(clap::Args)]
pub struct EntryArgs {
    /// Password-store entry (default: "pass_entry" in the config file)
    #[arg(long)]
    entry: Option<String>,
}

/// Verify a token, encrypt it into the store, and point the config at it.
pub async fn login(config_path: Option<&Path>, args: LoginArgs) -> anyhow::Result<()> {
    let cfg = Config::load_or_empty(config_path)?;
    let url = match args.url {
        Some(u) => {
            config::enforce_https(&u)?;
            u
        }
        None => cfg.resolve_url()?,
    };
    let entry_name = match args.entry.or_else(|| cfg.pass_entry().map(str::to_owned)) {
        Some(name) => name,
        None => default_entry_name(&url)?,
    };
    let store = cfg.pass_store()?;
    let entry = store.entry(&entry_name)?;

    let token = read_token_input(args.token_stdin)?;
    let user = GitlabClient::new(&url, &token)?
        .get("/api/v4/user")
        .await
        .map_err(|e| anyhow!("GitLab rejected the token: {}", e.to_tool_message()))?;
    let username = user["username"].as_str().unwrap_or("<unknown>");

    let recipients = match (entry.recipients()?, args.gpg_id) {
        (Some(_), Some(_)) => bail!(
            "the password store at {} already has a .gpg-id; drop --gpg-id",
            store.root().display()
        ),
        (Some(ids), None) => ids,
        (None, Some(id)) => {
            store.init(&id)?;
            eprintln!(
                "Initialised password store at {} for {id}",
                store.root().display()
            );
            vec![id]
        }
        (None, None) => bail!(
            "no .gpg-id in the password store at {}; pass --gpg-id <KEY_ID> to choose \
             the GPG key to encrypt to (see `gpg --list-secret-keys`)",
            store.root().display()
        ),
    };
    entry.write_token(&token, &recipients).await?;

    let path = config::config_path(config_path)?;
    let mut removed_plaintext = false;
    config::update_file(&path, |obj| {
        obj.insert("url".into(), url.clone().into());
        obj.insert("pass_entry".into(), entry_name.clone().into());
        removed_plaintext = obj.remove("token").is_some();
    })?;

    eprintln!(
        "Logged in to {url} as {username}. Token encrypted to {} at {}.",
        recipients.join(", "),
        entry.path().display()
    );
    eprintln!(
        "Updated {} (\"pass_entry\": \"{entry_name}\").",
        path.display()
    );
    if removed_plaintext {
        eprintln!("Removed the plaintext \"token\" from the config file.");
    }
    if std::env::var("GITLAB_TOKEN").is_ok_and(|v| !v.trim().is_empty()) {
        eprintln!("Note: GITLAB_TOKEN is set in this environment and takes precedence.");
    }
    Ok(())
}

/// Delete the stored token. The config's `pass_entry` is left in place, so
/// the server then reports "run `gitlab-mcp login`" rather than "not set".
pub fn logout(config_path: Option<&Path>, args: &EntryArgs) -> anyhow::Result<()> {
    let cfg = Config::load(config_path)?;
    let name = args
        .entry
        .as_deref()
        .or_else(|| cfg.pass_entry())
        .context("no pass entry: pass --entry or set \"pass_entry\" in the config file")?;
    let entry = cfg.pass_store()?.entry(name)?;
    entry.remove()?;
    eprintln!("Removed {}.", entry.path().display());
    Ok(())
}

/// Get the token's passphrase into gpg-agent's cache, prompting if needed.
pub async fn unlock(config_path: Option<&Path>) -> anyhow::Result<()> {
    let TokenSource::Pass(entry) = Config::load(config_path)?.resolve_token()? else {
        bail!("the token is not stored in pass, so there is nothing to unlock");
    };
    match entry.read_token().await {
        Ok(_) => {
            eprintln!("Already unlocked.");
            return Ok(());
        }
        Err(PassError::Locked { .. }) => {}
        Err(e) => return Err(e.into()),
    }
    entry.unlock_interactive().await?;
    // Confirm the way the server will read it: non-interactively.
    entry.read_token().await?;
    eprintln!(
        "Unlocked. The server can now read the token until gpg-agent's cache expires \
         (default-cache-ttl / max-cache-ttl in gpg-agent.conf); a running server keeps \
         it in memory after its first successful read."
    );
    Ok(())
}

/// Report where the token comes from and, for pass, whether it's unlocked —
/// without decrypting it.
pub async fn status(config_path: Option<&Path>) -> anyhow::Result<()> {
    let cfg = Config::load(config_path)?;
    match cfg.resolve_url() {
        Ok(url) => eprintln!("GitLab URL:   {url}"),
        Err(e) => eprintln!("GitLab URL:   error: {e}"),
    }
    let source = match cfg.resolve_token() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Token source: error: {e}");
            return Ok(());
        }
    };
    eprintln!("Token source: {}", source.describe());
    let TokenSource::Pass(entry) = source else {
        return Ok(());
    };
    if !entry.path().exists() {
        eprintln!("Entry:        missing (run `gitlab-mcp login`)");
        return Ok(());
    }
    let recipients = match entry.recipients() {
        Ok(Some(r)) => r,
        Ok(None) => {
            eprintln!("Recipients:   unknown (no .gpg-id in the store)");
            return Ok(());
        }
        Err(e) => {
            eprintln!("Recipients:   error: {e}");
            return Ok(());
        }
    };
    eprintln!("Recipients:   {}", recipients.join(", "));
    eprintln!(
        "gpg-agent:    {}",
        agent_state(entry.store(), &recipients).await
    );
    Ok(())
}

#[cfg(unix)]
async fn agent_state(store: &crate::credentials::PassStore, recipients: &[String]) -> String {
    use crate::credentials::agent;
    let result = async {
        let grips = agent::encryption_keygrips(store.gpg(), recipients).await?;
        if grips.is_empty() {
            return Ok(agent::AgentState::NoSecretKey);
        }
        let socket = agent::agent_socket(&store.gpgconf()).await?;
        Ok::<_, String>(
            agent::query_socket(&socket, &grips)
                .await?
                .map_or(agent::AgentState::NotRunning, |infos| {
                    agent::summarize(&infos)
                }),
        )
    }
    .await;
    result.map_or_else(|e| format!("unknown ({e})"), |s| s.to_string())
}

#[cfg(not(unix))]
async fn agent_state(_: &crate::credentials::PassStore, _: &[String]) -> String {
    "unknown (gpg-agent query is only supported on Unix)".into()
}

/// `gitlab-mcp/<host>` (with `_<port>` for a non-default port).
fn default_entry_name(url: &str) -> anyhow::Result<String> {
    let parsed = url::Url::parse(url).with_context(|| format!("invalid GitLab URL: {url}"))?;
    let host = parsed
        .host_str()
        .with_context(|| format!("GitLab URL has no host: {url}"))?
        .trim_matches(['[', ']'])
        .replace(':', "_");
    Ok(parsed.port().map_or_else(
        || format!("gitlab-mcp/{host}"),
        |port| format!("gitlab-mcp/{host}_{port}"),
    ))
}

fn read_token_input(from_stdin: bool) -> anyhow::Result<String> {
    let token = if from_stdin {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        line
    } else if std::io::stdin().is_terminal() {
        rpassword::prompt_password("GitLab personal access token: ")?
    } else {
        bail!("stdin is not a terminal; pass --token-stdin to read the token from it");
    };
    let token = token.trim().to_string();
    if token.is_empty() {
        bail!("empty token");
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::default_entry_name;

    #[test]
    fn default_entry_name_uses_host_and_port() {
        assert_eq!(
            default_entry_name("https://gitlab.com").unwrap(),
            "gitlab-mcp/gitlab.com"
        );
        assert_eq!(
            default_entry_name("https://gitlab.example.com:8443/sub").unwrap(),
            "gitlab-mcp/gitlab.example.com_8443"
        );
        assert_eq!(
            default_entry_name("http://[::1]:8080").unwrap(),
            "gitlab-mcp/__1_8080"
        );
    }
}
