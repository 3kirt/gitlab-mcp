use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow, bail};
use serde::Deserialize;

use crate::credentials::{PassEntry, PassStore, StoreLocation};

#[derive(Default, Deserialize)]
struct RawConfig {
    url: Option<String>,
    token: Option<String>,
    /// Name of a `pass`-compatible store entry holding the GPG-encrypted token.
    pass_entry: Option<String>,
    /// Path to the gpg binary (default: `gpg` from PATH).
    gpg: Option<String>,
    /// Password-store root (default: `$PASSWORD_STORE_DIR`, else `~/.password-store`).
    password_store_dir: Option<String>,
    /// GnuPG home (default: `$GNUPGHOME`, else gpg's own default).
    gnupg_home: Option<String>,
}

#[derive(Clone)]
pub struct Config {
    file_url: Option<String>,
    file_token: Option<String>,
    file_pass_entry: Option<String>,
    file_gpg: Option<String>,
    file_password_store_dir: Option<String>,
    file_gnupg_home: Option<String>,
    /// The `--config` path, when one was given (vs the default file): named
    /// in unlock/login advice so it targets this config's entry.
    explicit_path: Option<PathBuf>,
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
            .field("file_password_store_dir", &self.file_password_store_dir)
            .field("file_gnupg_home", &self.file_gnupg_home)
            .field("explicit_path", &self.explicit_path)
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
        let mut cfg = Self::load_resolved(&resolved, explicit)?;
        cfg.explicit_path = path.map(Path::to_path_buf);
        Ok(cfg)
    }

    fn load_resolved(resolved: &Path, explicit: bool) -> anyhow::Result<Self> {
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

        check_file_permissions(resolved)?;

        let contents = std::fs::read_to_string(resolved)
            .with_context(|| format!("reading config file {}", resolved.display()))?;
        let raw: RawConfig = serde_json::from_str(&contents)
            .with_context(|| format!("parsing config file {}", resolved.display()))?;

        Ok(Self::from_raw(raw))
    }

    /// Like [`Self::load`], but a missing file — even an explicit `--config`
    /// path — is an empty config: for `gitlab-mcp login`, which creates it.
    pub fn load_or_empty(path: Option<&Path>) -> anyhow::Result<Self> {
        let resolved = config_path(path)?;
        let mut cfg = if resolved
            .try_exists()
            .with_context(|| format!("checking config file {}", resolved.display()))?
        {
            Self::load_resolved(&resolved, true)?
        } else {
            Self::from_raw(RawConfig::default())
        };
        cfg.explicit_path = path.map(Path::to_path_buf);
        Ok(cfg)
    }

    fn from_raw(raw: RawConfig) -> Self {
        Self {
            file_url: raw.url,
            file_token: raw.token,
            file_pass_entry: non_blank(raw.pass_entry),
            file_gpg: non_blank(raw.gpg),
            file_password_store_dir: non_blank(raw.password_store_dir),
            file_gnupg_home: non_blank(raw.gnupg_home),
            explicit_path: None,
        }
    }

    /// The config file's own `url` (ignoring `GITLAB_URL`).
    pub fn file_url(&self) -> Option<&str> {
        self.file_url.as_deref().filter(|u| !u.trim().is_empty())
    }

    /// Resolve the GitLab base URL: GITLAB_URL env var, then config file, then error.
    pub fn resolve_url(&self) -> anyhow::Result<String> {
        let url =
            first_non_empty(env_var("GITLAB_URL"), self.file_url.clone()).ok_or_else(|| {
                anyhow!("GitLab URL not set: provide GITLAB_URL or set \"url\" in config file")
            })?;
        enforce_https(&url)?;
        Ok(url)
    }

    /// Resolve the token source: GITLAB_TOKEN env var, then the config file's
    /// plaintext `token` or its `pass_entry` (setting both is an error, since
    /// either silently shadowing the other would be surprising), then error.
    pub fn resolve_token(&self) -> anyhow::Result<TokenSource> {
        if let Some(t) = env_var("GITLAB_TOKEN") {
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

    /// The config's `pass_entry` as an entry, regardless of `GITLAB_TOKEN`.
    /// `unlock`/`status` use this: a shell exporting GITLAB_TOKEN (common for
    /// `glab` users) must not hide the entry a GUI-launched server, started
    /// without that variable, is trying to decrypt.
    pub fn configured_pass_entry(&self) -> anyhow::Result<Option<PassEntry>> {
        self.file_pass_entry
            .as_deref()
            .map(|name| Ok(self.pass_store()?.entry(name)?))
            .transpose()
    }

    /// The password store, honoring the config's `gpg`, `password_store_dir`
    /// and `gnupg_home` over the process environment.
    pub fn pass_store(&self) -> anyhow::Result<PassStore> {
        Ok(PassStore::resolve(&StoreLocation {
            gpg: self.file_gpg.as_deref(),
            store_dir: self.file_password_store_dir.as_deref(),
            gnupg_home: self.file_gnupg_home.as_deref(),
        })?
        .with_config_path(self.explicit_path.clone()))
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
    // Exclusive create: never follow or truncate something already at this
    // predictable name (e.g. a planted symlink). Only a file *we* created is
    // removed on failure.
    let mut file = crate::credentials::create_new_private(&tmp)
        .with_context(|| format!("creating temporary config file {}", tmp.display()))?;
    let write = || -> anyhow::Result<()> {
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

/// Fail early (before `login` prompts for a token or touches the password
/// store) if the config at `path` couldn't be written: its directory must
/// exist and be writable, and an existing file must pass the permission
/// check `update_file` applies. Otherwise `login` would encrypt the token
/// and write `.gpg-id`, then fail at the very last step.
pub fn check_writable(path: &Path) -> anyhow::Result<()> {
    if path
        .try_exists()
        .with_context(|| format!("checking config file {}", path.display()))?
    {
        check_file_permissions(path)?;
    }
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    let meta = std::fs::metadata(dir)
        .with_context(|| format!("config directory {} does not exist", dir.display()))?;
    if !meta.is_dir() {
        bail!("{} is not a directory", dir.display());
    }
    // Probe with a real exclusive create: permission bits alone can't see
    // ACLs, read-only mounts, or ownership.
    let probe = dir.join(format!(".gitlab_mcp.probe{}", std::process::id()));
    crate::credentials::create_new_private(&probe)
        .with_context(|| format!("config directory {} is not writable", dir.display()))?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// The first value that is actually set wins (see [`non_blank`]).
fn first_non_empty(env_val: Option<String>, file_val: Option<String>) -> Option<String> {
    non_blank(env_val).or_else(|| non_blank(file_val))
}

/// The one "is this value set?" rule for every env var and config value:
/// blank counts as unset, and so does an unexpanded `${VAR}` placeholder.
/// MCP clients substitute `${GITLAB_TOKEN}` in a shared config from the
/// user's environment, and when the variable isn't exported Claude Code
/// passes the placeholder through *literally*. Taking that as a real token
/// would shadow a `pass_entry` and fail every call with a 401; treating it
/// as unset falls through to the config file, as an empty value does.
fn non_blank(v: Option<String>) -> Option<String> {
    v.filter(|s| !is_unset_value(s))
}

fn is_unset_value(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || is_unexpanded_placeholder(t)
}

/// `${NAME}` or `${NAME:-…}` exactly, as left behind by a client that didn't
/// substitute it. No real token or URL has this shape.
fn is_unexpanded_placeholder(t: &str) -> bool {
    t.strip_prefix("${")
        .and_then(|rest| rest.strip_suffix('}'))
        .is_some_and(|inner| {
            let name = inner.split_once(":-").map_or(inner, |(n, _)| n);
            !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
}

/// An environment variable, treating blank values and unexpanded `${…}`
/// placeholders as unset.
pub fn env_var(name: &str) -> Option<String> {
    let raw = std::env::var(name).ok()?;
    if is_unexpanded_placeholder(raw.trim()) {
        tracing::warn!(
            variable = name,
            "ignoring unexpanded placeholder (the MCP client did not substitute it; \
             the variable is not set in its environment)"
        );
        return None;
    }
    non_blank(Some(raw))
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
    use super::{
        Config, RawConfig, TokenSource, check_writable, enforce_https, first_non_empty,
        is_unexpanded_placeholder, update_file,
    };

    #[test]
    fn explicit_missing_config_path_is_an_error() {
        let err =
            Config::load(Some(std::path::Path::new("/nonexistent/gitlab_mcp.json"))).unwrap_err();
        assert!(err.to_string().contains("does not exist"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn update_file_refuses_a_planted_temp_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg.json");
        let victim = dir.path().join("victim");
        std::fs::write(&victim, "precious").unwrap();
        let tmp = dir
            .path()
            .join(format!("cfg.json.tmp{}", std::process::id()));
        std::os::unix::fs::symlink(&victim, &tmp).unwrap();

        let err = update_file(&path, |obj| {
            obj.insert("url".into(), "https://a".into());
        })
        .unwrap_err();
        assert!(
            format!("{err:#}").contains("temporary config file"),
            "{err:#}"
        );
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "precious");
        assert!(
            tmp.symlink_metadata().is_ok(),
            "a file we didn't create was removed"
        );
        assert!(!path.exists());
    }

    #[test]
    fn check_writable_rejects_a_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let err = check_writable(&dir.path().join("nope").join("cfg.json")).unwrap_err();
        assert!(format!("{err:#}").contains("does not exist"), "{err:#}");
        check_writable(&dir.path().join("cfg.json")).unwrap();
        // The probe leaves nothing behind.
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn explicit_config_path_reaches_the_store_advice() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg.json");
        let cfg = Config::load_or_empty(Some(&path)).unwrap();
        let cli = cfg.pass_store().unwrap().cli();
        assert!(cli.starts_with("gitlab-mcp --config "), "{cli}");
        assert!(cli.contains("cfg.json"), "{cli}");
    }

    #[test]
    fn debug_redacts_token() {
        let cfg = Config {
            file_url: Some("https://gitlab.example.com".into()),
            file_token: Some("glpat-secret".into()),
            file_pass_entry: None,
            file_gpg: None,
            file_password_store_dir: None,
            file_gnupg_home: None,
            explicit_path: None,
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
        // What Claude Code passes for `"${GITLAB_TOKEN}"` when it's unset.
        assert_eq!(first_non_empty(s("${GITLAB_TOKEN}"), s("file")), s("file"));
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
            ..RawConfig::default()
        })
    }

    // These resolve without GITLAB_TOKEN set; skip if the developer's shell
    // exports it (env vars are process-global, so tests can't unset it).
    fn gitlab_token_env_set() -> bool {
        super::env_var("GITLAB_TOKEN").is_some()
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

    #[test]
    // `${X:-}` etc. are shell-style placeholders under test, not format args.
    #[allow(clippy::literal_string_with_formatting_args)]
    fn unexpanded_placeholders_are_recognised() {
        for p in [
            "${GITLAB_TOKEN}",
            "${GITLAB_URL:-https://gitlab.com}",
            "${A_1}",
            "${X:-}",
        ] {
            assert!(is_unexpanded_placeholder(p), "{p}");
        }
        for v in [
            "glpat-abc",
            "https://gitlab.com",
            "${}",
            "$GITLAB_TOKEN",
            "${GITLAB_TOKEN",
            "x${GITLAB_TOKEN}",
            "${not valid}",
        ] {
            assert!(!is_unexpanded_placeholder(v), "{v}");
        }
    }

    #[test]
    fn placeholder_in_config_file_counts_as_unset() {
        if gitlab_token_env_set() {
            return;
        }
        let err = raw(Some("${GITLAB_TOKEN}"), None)
            .resolve_token()
            .err()
            .unwrap();
        assert!(err.to_string().contains("not set"), "{err}");
    }

    /// Every MCP client config we ship or document must give each `${VAR}`
    /// in `env` a `:-` default. Claude Code passes a bare `${VAR}` through
    /// *literally* when the variable is unset, which shadowed `pass_entry`
    /// with a bogus token in v0.39.0. (The server now ignores such values
    /// too, but the configs shouldn't depend on that.)
    #[test]
    fn shipped_mcp_configs_default_every_placeholder() {
        let root = env!("CARGO_MANIFEST_DIR");
        let mut configs: Vec<(String, serde_json::Value)> = ["plugin/.mcp.json", ".mcp.json"]
            .iter()
            .map(|f| {
                let text = std::fs::read_to_string(format!("{root}/{f}")).unwrap();
                ((*f).to_string(), serde_json::from_str(&text).unwrap())
            })
            .collect();
        // JSON examples in the README that configure an MCP server.
        let readme = std::fs::read_to_string(format!("{root}/README.md")).unwrap();
        for (i, block) in readme.split("```json\n").skip(1).enumerate() {
            let body = block.split("```").next().unwrap();
            if body.contains("mcpServers") {
                configs.push((
                    format!("README.md json block {i}"),
                    serde_json::from_str(body).unwrap(),
                ));
            }
        }
        assert!(
            configs.len() >= 3,
            "expected plugin, repo, and README configs"
        );

        for (source, config) in &configs {
            let servers = config["mcpServers"].as_object().expect("mcpServers object");
            for (server, def) in servers {
                let Some(env) = def["env"].as_object() else {
                    continue;
                };
                for (key, value) in env {
                    let value = value.as_str().unwrap_or_default();
                    for placeholder in value.split("${").skip(1) {
                        let inner = placeholder.split('}').next().unwrap_or_default();
                        assert!(
                            inner.contains(":-"),
                            "{source}: {server}.env.{key} uses ${{{inner}}} without a `:-` default; \
                             write ${{{inner}:-}} so an unset variable expands to empty"
                        );
                    }
                }
            }
        }
    }
}
