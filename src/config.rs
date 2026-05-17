use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow, bail};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    url: Option<String>,
    token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    file_url: Option<String>,
    file_token: Option<String>,
}

impl Config {
    /// Load configuration from path (default: `~/.gitlab_mcp.json`).
    /// A missing file is not an error.
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let resolved = match path {
            Some(p) => p.to_path_buf(),
            None => default_config_path()?,
        };

        if !resolved.exists() {
            return Ok(Config {
                file_url: None,
                file_token: None,
            });
        }

        check_file_permissions(&resolved)?;

        let contents = std::fs::read_to_string(&resolved)
            .with_context(|| format!("reading config file {}", resolved.display()))?;
        let raw: RawConfig = serde_json::from_str(&contents)
            .with_context(|| format!("parsing config file {}", resolved.display()))?;

        Ok(Config {
            file_url: raw.url,
            file_token: raw.token,
        })
    }

    /// Resolve the GitLab base URL: GITLAB_URL env var, then config file, then error.
    pub fn resolve_url(&self) -> anyhow::Result<String> {
        let url = std::env::var("GITLAB_URL")
            .ok()
            .or_else(|| self.file_url.clone())
            .ok_or_else(|| {
                anyhow!("GitLab URL not set: provide GITLAB_URL or set \"url\" in config file")
            })?;
        enforce_https(&url)?;
        Ok(url)
    }

    /// Resolve the GitLab personal access token: GITLAB_TOKEN env var, then config file, then error.
    pub fn resolve_token(&self) -> anyhow::Result<String> {
        std::env::var("GITLAB_TOKEN")
            .ok()
            .or_else(|| self.file_token.clone())
            .ok_or_else(|| {
                anyhow!(
                    "GitLab token not set: provide GITLAB_TOKEN or set \"token\" in config file"
                )
            })
    }
}

fn default_config_path() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .context("cannot determine home directory")?;
    Ok(PathBuf::from(home).join(".gitlab_mcp.json"))
}

#[allow(unused_variables)]
fn check_file_permissions(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(path)
            .with_context(|| format!("checking permissions of {}", path.display()))?;
        if meta.permissions().mode() & 0o004 != 0 {
            bail!(
                "config file {} is world-readable; run: chmod o-r {}",
                path.display(),
                path.display()
            );
        }
    }
    Ok(())
}

fn enforce_https(url: &str) -> anyhow::Result<()> {
    if url.starts_with("https://") {
        return Ok(());
    }
    let is_local = url.starts_with("http://localhost") || url.starts_with("http://127.0.0.1");
    if is_local {
        return Ok(());
    }
    bail!(
        "GitLab URL must use HTTPS, got: {}  \
         (use https:// to prevent token from being sent in plaintext)",
        url
    );
}
