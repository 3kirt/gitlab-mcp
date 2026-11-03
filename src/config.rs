use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow, bail};
use serde::Deserialize;

#[derive(Default, Deserialize)]
struct RawConfig {
    url: Option<String>,
    token: Option<String>,
}

#[derive(Clone)]
pub struct Config {
    file_url: Option<String>,
    file_token: Option<String>,
}

/// Hand-written so a stray `?config` in a log line can never print the token.
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("file_url", &self.file_url)
            .field(
                "file_token",
                &self.file_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl Config {
    /// Load configuration from path (default: `~/.gitlab_mcp.json`).
    ///
    /// A missing *default* file is not an error (env vars may supply
    /// everything), but a path given explicitly via `--config` must exist —
    /// otherwise a typo would surface later as a misleading "URL not set".
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let explicit = path.is_some();
        let resolved = match path {
            Some(p) => p.to_path_buf(),
            None => default_config_path()?,
        };

        // `try_exists` (unlike `exists`) surfaces I/O errors such as a
        // permission-denied parent directory instead of reporting "absent".
        let present = resolved
            .try_exists()
            .with_context(|| format!("checking config file {}", resolved.display()))?;
        if !present {
            if explicit {
                bail!("config file {} does not exist", resolved.display());
            }
            return Ok(Self {
                file_url: None,
                file_token: None,
            });
        }

        check_file_permissions(&resolved)?;

        let contents = std::fs::read_to_string(&resolved)
            .with_context(|| format!("reading config file {}", resolved.display()))?;
        let raw: RawConfig = serde_json::from_str(&contents)
            .with_context(|| format!("parsing config file {}", resolved.display()))?;

        Ok(Self {
            file_url: raw.url,
            file_token: raw.token,
        })
    }

    /// Resolve the GitLab base URL: GITLAB_URL env var, then config file, then error.
    pub fn resolve_url(&self) -> anyhow::Result<String> {
        let url = first_non_empty(std::env::var("GITLAB_URL").ok(), self.file_url.clone())
            .ok_or_else(|| {
                anyhow!("GitLab URL not set: provide GITLAB_URL or set \"url\" in config file")
            })?;
        enforce_https(&url)?;
        Ok(url)
    }

    /// Resolve the GitLab personal access token: GITLAB_TOKEN env var, then config file, then error.
    pub fn resolve_token(&self) -> anyhow::Result<String> {
        first_non_empty(std::env::var("GITLAB_TOKEN").ok(), self.file_token.clone()).ok_or_else(
            || {
                anyhow!(
                    "GitLab token not set: provide GITLAB_TOKEN or set \"token\" in config file"
                )
            },
        )
    }
}

/// The first non-blank value wins. A set-but-empty env var counts as unset:
/// MCP clients expanding `${GITLAB_TOKEN}` in a shared config pass "" when the
/// user hasn't exported the variable, and failing here with the "not set"
/// error beats starting up and sending empty auth headers.
fn first_non_empty(env_val: Option<String>, file_val: Option<String>) -> Option<String> {
    let non_blank = |v: Option<String>| v.filter(|s| !s.trim().is_empty());
    non_blank(env_val).or_else(|| non_blank(file_val))
}

fn default_config_path() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .context("cannot determine home directory")?;
    Ok(PathBuf::from(home).join(".gitlab_mcp.json"))
}

#[cfg_attr(not(unix), expect(unused_variables))]
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
    if url.starts_with("http://") {
        let parsed = url::Url::parse(url).with_context(|| format!("invalid GitLab URL: {url}"))?;
        let host = parsed.host_str().unwrap_or("");
        // `host_str` returns IPv6 literals bracketed, e.g. "[::1]".
        if host == "localhost" || host == "127.0.0.1" || host == "[::1]" {
            return Ok(());
        }
    }
    bail!(
        "GitLab URL must use HTTPS, got: {url}  \
         (use https:// to prevent token from being sent in plaintext)"
    );
}

#[cfg(test)]
mod tests {
    use super::{Config, enforce_https, first_non_empty};

    #[test]
    fn explicit_missing_config_path_is_an_error() {
        let err =
            Config::load(Some(std::path::Path::new("/nonexistent/gitlab_mcp.json"))).unwrap_err();
        assert!(err.to_string().contains("does not exist"), "{err}");
    }

    #[test]
    fn debug_redacts_token() {
        let cfg = Config {
            file_url: Some("https://gitlab.example.com".into()),
            file_token: Some("glpat-secret".into()),
        };
        let shown = format!("{cfg:?}");
        assert!(!shown.contains("glpat-secret"), "{shown}");
        assert!(shown.contains("<redacted>"), "{shown}");
    }

    #[test]
    fn empty_env_value_falls_back_to_file_value() {
        let s = |v: &str| Some(v.to_string());
        assert_eq!(first_non_empty(s("env"), s("file")), s("env"));
        assert_eq!(first_non_empty(s(""), s("file")), s("file"));
        assert_eq!(first_non_empty(s("  "), s("file")), s("file"));
        assert_eq!(first_non_empty(None, s("file")), s("file"));
        assert_eq!(first_non_empty(s(""), s("")), None);
        assert_eq!(first_non_empty(None, None), None);
    }

    #[test]
    fn https_is_allowed() {
        assert!(enforce_https("https://gitlab.com").is_ok());
        assert!(enforce_https("https://gitlab.example.com/").is_ok());
    }

    #[test]
    fn http_localhost_is_allowed() {
        assert!(enforce_https("http://localhost").is_ok());
        assert!(enforce_https("http://localhost:8080").is_ok());
        assert!(enforce_https("http://localhost/path").is_ok());
    }

    #[test]
    fn http_loopback_is_allowed() {
        assert!(enforce_https("http://127.0.0.1").is_ok());
        assert!(enforce_https("http://127.0.0.1:8080").is_ok());
    }

    #[test]
    fn http_ipv6_loopback_is_allowed() {
        assert!(enforce_https("http://[::1]").is_ok());
        assert!(enforce_https("http://[::1]:8080").is_ok());
    }

    #[test]
    fn http_plain_host_is_rejected() {
        assert!(enforce_https("http://gitlab.com").is_err());
        assert!(enforce_https("http://internal.company.com").is_err());
    }

    #[test]
    fn non_http_scheme_is_rejected() {
        assert!(enforce_https("ftp://gitlab.com").is_err());
        assert!(enforce_https("gitlab.com").is_err());
    }

    #[test]
    fn localhost_prefix_bypass_is_rejected() {
        assert!(enforce_https("http://localhost.evil.com").is_err());
        assert!(enforce_https("http://127.0.0.1.evil.com").is_err());
    }
}
