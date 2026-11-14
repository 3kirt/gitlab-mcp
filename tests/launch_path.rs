//! End-to-end tests of the *built binary*, launched the way an MCP client
//! launches it: a spawned process speaking MCP over stdio, configured only
//! through its environment and config file.
//!
//! Unit tests call domain functions directly and so can't see what a client
//! puts in the process environment. v0.39.0 shipped a bug that lived exactly
//! there: for `"GITLAB_TOKEN": "${GITLAB_TOKEN}"` with the variable unset,
//! Claude Code passes the placeholder text through literally, the server took
//! it as a real token, and a configured `pass_entry` was never used (every
//! call got a 401). These tests drive each way a client can hand over
//! `GITLAB_TOKEN` and assert the request reaching GitLab carries the token
//! decrypted from the pass entry.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PASS_TOKEN: &str = "glpat-from-pass-entry";
const STEP_TIMEOUT: Duration = Duration::from_secs(20);

/// A stand-in for `gpg --decrypt` that prints the token with the status lines
/// real GnuPG emits, so the server's status-code classification is exercised.
fn write_fake_gpg(dir: &Path) -> std::path::PathBuf {
    let gpg = dir.join("gpg");
    std::fs::write(
        &gpg,
        format!(
            "#!/bin/sh\n\
             printf '%s\\n' '{PASS_TOKEN}'\n\
             echo '[GNUPG:] BEGIN_DECRYPTION' >&2\n\
             echo '[GNUPG:] DECRYPTION_OKAY' >&2\n"
        ),
    )
    .unwrap();
    set_mode(&gpg, 0o755);
    gpg
}

fn set_mode(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
}

/// How the client hands `GITLAB_TOKEN` to the server.
#[derive(Clone, Copy, Debug)]
enum TokenEnv {
    /// Not in the environment at all.
    Unset,
    /// `"${GITLAB_TOKEN:-}"` with the variable unset.
    Empty,
    /// `"${GITLAB_TOKEN}"` with the variable unset: Claude Code's literal
    /// passthrough, the v0.39.0 bug.
    LiteralPlaceholder,
}

/// Spawn the server against `gitlab` with a pass-entry config, call one tool
/// over MCP stdio, and return the `tools/call` result.
async fn call_through_binary(gitlab: &MockServer, token_env: TokenEnv) -> Value {
    let dir = tempfile::tempdir().unwrap();
    let gpg = write_fake_gpg(dir.path());
    let store = dir.path().join("store");
    std::fs::create_dir_all(store.join("gitlab-mcp")).unwrap();
    // Content is irrelevant: the fake gpg ignores its input.
    std::fs::write(store.join("gitlab-mcp/test.gpg"), "ciphertext").unwrap();
    let config = dir.path().join("gitlab_mcp.json");
    std::fs::write(
        &config,
        json!({
            "url": gitlab.uri(),
            "pass_entry": "gitlab-mcp/test",
            "password_store_dir": store,
            "gpg": gpg,
        })
        .to_string(),
    )
    .unwrap();
    set_mode(&config, 0o600);

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gitlab-mcp"));
    cmd.arg("--config")
        .arg(&config)
        // Isolate from the developer's own setup.
        .env("HOME", dir.path())
        .env_remove("GITLAB_URL")
        .env_remove("GITLAB_TOKEN")
        .env_remove("PASSWORD_STORE_DIR")
        .env_remove("GNUPGHOME")
        .env_remove("RUST_LOG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    match token_env {
        TokenEnv::Unset => {}
        TokenEnv::Empty => {
            cmd.env("GITLAB_TOKEN", "");
        }
        TokenEnv::LiteralPlaceholder => {
            cmd.env("GITLAB_TOKEN", "${GITLAB_TOKEN}");
        }
    }
    let mut child = cmd.spawn().expect("spawn gitlab-mcp");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();

    let mut send = async |msg: Value| {
        stdin
            .write_all(format!("{msg}\n").as_bytes())
            .await
            .unwrap();
    };
    send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "launch-path-test", "version": "0"}
        }}),
    )
    .await;
    read_response(&mut stdout, 1).await;
    send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).await;
    send(
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "name": "gitlab_metadata_get", "arguments": {}
        }}),
    )
    .await;
    let response = read_response(&mut stdout, 2).await;
    response["result"].clone()
}

/// Read stdout lines until the JSON-RPC response with `id` arrives.
async fn read_response<R>(lines: &mut tokio::io::Lines<R>, id: u64) -> Value
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    tokio::time::timeout(STEP_TIMEOUT, async {
        loop {
            let line = lines
                .next_line()
                .await
                .unwrap()
                .expect("server closed stdout before responding");
            let msg: Value = serde_json::from_str(&line).unwrap();
            if msg["id"] == id {
                return msg;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no response to request {id} within {STEP_TIMEOUT:?}"))
}

async fn assert_uses_pass_token(token_env: TokenEnv) {
    let gitlab = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/metadata"))
        .and(header("PRIVATE-TOKEN", PASS_TOKEN))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"version": "19.0.0"})))
        .expect(1)
        .mount(&gitlab)
        .await;
    // Anything else (e.g. a placeholder sent as the token) is a 401, as GitLab
    // would answer.
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message": "401 Unauthorized"})),
        )
        .mount(&gitlab)
        .await;

    let result = call_through_binary(&gitlab, token_env).await;
    assert_ne!(
        result["isError"],
        json!(true),
        "{token_env:?}: tool call failed: {result}"
    );
    assert!(
        result.to_string().contains("19.0.0"),
        "{token_env:?}: unexpected result: {result}"
    );
}

#[tokio::test]
async fn pass_entry_is_used_when_token_env_is_unset() {
    assert_uses_pass_token(TokenEnv::Unset).await;
}

#[tokio::test]
async fn pass_entry_is_used_when_token_env_is_empty() {
    assert_uses_pass_token(TokenEnv::Empty).await;
}

#[tokio::test]
async fn pass_entry_is_used_when_client_passes_a_literal_placeholder() {
    assert_uses_pass_token(TokenEnv::LiteralPlaceholder).await;
}
