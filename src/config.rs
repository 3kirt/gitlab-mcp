use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow, bail};
use serde::Deserialize;

use crate::credentials::{PassEntry, PassStore};

#[derive(Default, Deserialize)]
struct RawConfig {
    url: Option<String>,
    token: Option<String>,
    /// Name of a `pass`-compatible store entry holding the GPG-encrypted token.
    pass_entry: Option<String>,
    /// Path to the gpg binary (default: `gpg` from PATH).
    gpg: Option<String>,
}

#[derive(Clone)]
pub struct Config {
    file_url: Option<String>,
    file_token: Option<String>,
    file_pass_entry: Option<String>,
    file_gpg: Option<String>,
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
            .field("file_pass_entry", &self.file_pass_entry)
            .field("file_gpg", &self.file_gpg)
            .finish()
    }
}

/// Where the GitLab token comes from.
pub enum TokenSource {
    /// `GITLAB_TOKEN` environment variable.
    Env(String),
    /// Plaintext `token` in the config file.
    File(String),
    /// GPG-encrypted `pass` entry, decrypted lazily on first use.
    Pass(PassEntry),
}

impl TokenSource {
    /// Human description, never including the secret.
    pub fn describe(&self) -> String {
        match self {
            Self::Env(_) => "GITLAB_TOKEN environment variable".into(),
            Self::File(_) => "plaintext \"token\" in the config file".into(),
            Self::Pass(e) => format!("pass entry {} ({})", e.name(), e.path().display()),
        }
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
        let resolved = config_path(path)?;

        // `try_exists` (unlike `exists`) surfaces I/O errors such as a
        // permission-denied parent directory instead of reporting "absent".
        let present = resolved
            .try_exists()
            .with_context(|| format!("checking config file {}", resolved.display()))?;
        if !present {
            if explicit {
                bail!("config file {} does not exist", resolved.display());
            }
            return Ok(Self::from_raw(RawConfig::default()));
        }

        check_file_permissions(&resolved)?;

        let contents = std::fs::read_to_string(&resolved)
            .with_context(|| format!("reading config file {}", resolved.display()))?;
        let raw: RawConfig = serde_json::from_str(&contents)
            .with_context(|| format!("parsing config file {}", resolved.display()))?;

        Ok(Self::from_raw(raw))
    }

    /// Like [`Self::load`], but a missing file — even an explicit `--config`
    /// path — is an empty config: for `gitlab-mcp login`, which creates it.
    pub fn load_or_empty(path: Option<&Path>) -> anyhow::Result<Self> {
        let resolved = config_path(path)?;
        if resolved
            .try_exists()
            .with_context(|| format!("checking config file {}", resolved.display()))?
        {
            Self::load(Some(&resolved))
        } else {
            Ok(Self::from_raw(RawConfig::default()))
        }
    }

    fn from_raw(raw: RawConfig) -> Self {
        let non_blank = |v: Option<String>| v.filter(|s| !s.trim().is_empty());
        Self {
            file_url: raw.url,
            file_token: raw.token,
            file_pass_entry: non_blank(raw.pass_entry),
            file_gpg: non_blank(raw.gpg),
        }
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

    /// Resolve the token source: GITLAB_TOKEN env var, then the config file's
    /// plaintext `token` or its `pass_entry` (setting both is an error, since
    /// either silently shadowing the other would be surprising), then error.
    pub fn resolve_token(&self) -> anyhow::Result<TokenSource> {
        let non_blank = |v: Option<String>| v.filter(|s| !s.trim().is_empty());
        if let Some(t) = non_blank(std::env::var("GITLAB_TOKEN").ok()) {
            return Ok(TokenSource::Env(t));
        }
        match (non_blank(self.file_token.clone()), &self.file_pass_entry) {
            (Some(_), Some(_)) => bail!(
                "config file sets both \"token\" and \"pass_entry\"; remove the plaintext \
                 \"token\" (gitlab-mcp login does this for you)"
            ),
            (Some(t), None) => Ok(TokenSource::File(t)),
            (None, Some(name)) => Ok(TokenSource::Pass(self.pass_store()?.entry(name)?)),
            (None, None) => bail!(
                "GitLab token not set: provide GITLAB_TOKEN, set \"token\" in config file, \
                 or run `gitlab-mcp login` to store it GPG-encrypted"
            ),
        }
    }

    pub fn pass_entry(&self) -> Option<&str> {
        self.file_pass_entry.as_deref()
    }

    /// The password store, honoring the config's `gpg` override.
    pub fn pass_store(&self) -> anyhow::Result<PassStore> {
        PassStore::from_env(self.file_gpg.as_deref())
    }
}

/// Apply `edit` to the JSON object in the config file at `path` (created if
/// absent) and write it back atomically with owner-only permissions. A
/// symlinked config is updated at its target rather than replaced.
pub fn update_file(
    path: &Path,
    edit: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
) -> anyhow::Result<()> {
    let target = if path.try_exists()? {
        check_file_permissions(path)?;
        std::fs::canonicalize(path).with_context(|| format!("resolving {}", path.display()))?
    } else {
        path.to_path_buf()
    };
    let mut obj = if target.exists() {
        let contents = std::fs::read_to_string(&target)
            .with_context(|| format!("reading config file {}", target.display()))?;
        match serde_json::from_str(&contents)
            .with_context(|| format!("parsing config file {}", target.display()))?
        {
            serde_json::Value::Object(m) => m,
            _ => bail!("config file {} is not a JSON object", target.display()),
        }
    } else {
        serde_json::Map::new()
    };
    edit(&mut obj);

    let mut tmp_name = target.clone().into_os_string();
    tmp_name.push(format!(".tmp{}", std::process::id()));
    let tmp = PathBuf::from(tmp_name);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let write = || -> anyhow::Result<()> {
        let mut file = opts.open(&tmp)?;
        let mut body = serde_json::to_string_pretty(&serde_json::Value::Object(obj))?;
        body.push('\n');
        std::io::Write::write_all(&mut file, body.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, &target)?;
        Ok(())
    };
    write()
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
        .with_context(|| format!("writing config file {}", target.display()))
}

/// The first non-blank value wins. A set-but-empty env var counts as unset:
/// MCP clients expanding `${GITLAB_TOKEN}` in a shared config pass "" when the
/// user hasn't exported the variable, and failing here with the "not set"
/// error beats starting up and sending empty auth headers.
fn first_non_empty(env_val: Option<String>, file_val: Option<String>) -> Option<String> {
    let non_blank = |v: Option<String>| v.filter(|s| !s.trim().is_empty());
    non_blank(env_val).or_else(|| non_blank(file_val))
}

/// The config file path: `path` if given, else `~/.gitlab_mcp.json`.
pub fn config_path(path: Option<&Path>) -> anyhow::Result<PathBuf> {
    match path {
        Some(p) => Ok(p.to_path_buf()),
        None => Ok(home_dir()?.join(".gitlab_mcp.json")),
    }
}

pub fn home_dir() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .context("cannot determine home directory")?;
    Ok(PathBuf::from(home))
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

pub fn enforce_https(url: &str) -> anyhow::Result<()> {
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
    use super::{Config, RawConfig, TokenSource, enforce_https, first_non_empty, update_file};

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
            file_pass_entry: None,
            file_gpg: None,
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

    fn raw(token: Option<&str>, pass_entry: Option<&str>) -> Config {
        Config::from_raw(RawConfig {
            url: None,
            token: token.map(Into::into),
            pass_entry: pass_entry.map(Into::into),
            gpg: None,
        })
    }

    // These resolve without GITLAB_TOKEN set; skip if the developer's shell
    // exports it (env vars are process-global, so tests can't unset it).
    fn gitlab_token_env_set() -> bool {
        std::env::var("GITLAB_TOKEN").is_ok_and(|v| !v.trim().is_empty())
    }

    #[test]
    fn pass_entry_resolves_to_pass_source() {
        if gitlab_token_env_set() {
            return;
        }
        let src = raw(None, Some("gitlab-mcp/gitlab.com"))
            .resolve_token()
            .unwrap();
        let TokenSource::Pass(entry) = src else {
            panic!("expected pass source");
        };
        assert_eq!(entry.name(), "gitlab-mcp/gitlab.com");
    }

    #[test]
    fn token_and_pass_entry_together_is_an_error() {
        if gitlab_token_env_set() {
            return;
        }
        let err = raw(Some("glpat-x"), Some("e"))
            .resolve_token()
            .err()
            .unwrap();
        assert!(err.to_string().contains("both"), "{err}");
    }

    #[test]
    fn missing_token_mentions_login() {
        if gitlab_token_env_set() {
            return;
        }
        let err = raw(None, None).resolve_token().err().unwrap();
        assert!(err.to_string().contains("gitlab-mcp login"), "{err}");
    }

    #[test]
    fn update_file_edits_atomically_and_privately() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg.json");
        std::fs::write(&path, r#"{"url":"https://a","token":"glpat-x","extra":1}"#).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        update_file(&path, |o| {
            o.remove("token");
            o.insert("pass_entry".into(), "e".into());
        })
        .unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"url":"https://a","extra":1,"pass_entry":"e"})
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "{mode:o}");
        }
        // Only the config file remains: no stray temp file.
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn update_file_creates_missing_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.json");
        update_file(&path, |o| {
            o.insert("url".into(), "https://a".into());
        })
        .unwrap();
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("https://a")
        );
    }
}
