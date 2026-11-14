//! Tests against the machine's *real* GnuPG, which the rest of the suite
//! replaces with `test_support::fake_gpg()` (canned status lines captured
//! from GnuPG 2.4). These catch drift in what a newer gpg actually emits:
//! the `DECRYPTION_OKAY` success marker, `ERROR … <code 85>` for a locked
//! key, and code 17 for a missing secret key.
//!
//! `#[ignore]`d so a plain `cargo test` never touches gpg; run them with
//! `make test-real-gpg` (CI's `real-gpg` job). Each test gets a throwaway
//! GnuPG home under `/tmp`, never the user's keyring. The short path matters:
//! macOS caps the gpg-agent socket path at 104 bytes, so a GnuPG home under a
//! long `$TMPDIR` fails with "No agent running".

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{PassError, PassStore};

const PASSPHRASE: &str = "real-gpg-test-passphrase";

/// A throwaway GnuPG home; kills its gpg-agent on drop so tests don't leak
/// agents.
struct GnupgHome {
    dir: tempfile::TempDir,
}

impl GnupgHome {
    fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("gmcp")
            .tempdir_in("/tmp")
            .expect("creating a GnuPG home under /tmp");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
            .expect("restricting the GnuPG home");
        Self { dir }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn gpg(&self, args: &[&str]) {
        let out = Command::new("gpg")
            .arg("--homedir")
            .arg(self.path())
            .args(["--batch", "--quiet", "--no-tty"])
            .args(args)
            .output()
            .expect("running gpg (is GnuPG installed?)");
        assert!(
            out.status.success(),
            "gpg {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Generate an encryption-capable key for `email`, protected by
    /// `passphrase` (empty = no passphrase).
    fn gen_key(&self, email: &str, passphrase: &str) {
        let uid = format!("Real GPG Test <{email}>");
        self.gpg(&[
            "--pinentry-mode",
            "loopback",
            "--passphrase",
            passphrase,
            "--quick-gen-key",
            &uid,
            "default",
            "default",
            "never",
        ]);
    }

    /// Restart the agent so it forgets any cached passphrase.
    fn kill_agent(&self) {
        let _ = Command::new("gpgconf")
            .arg("--homedir")
            .arg(self.path())
            .args(["--kill", "gpg-agent"])
            .output();
    }

    fn store(&self) -> PassStore {
        PassStore::new(self.store_root(), "gpg").with_homedir(Some(self.path().to_path_buf()))
    }

    fn store_root(&self) -> PathBuf {
        self.path().join("store")
    }
}

impl Drop for GnupgHome {
    fn drop(&mut self) {
        self.kill_agent();
    }
}

#[tokio::test]
#[ignore = "needs a real gpg binary; run with `make test-real-gpg`"]
async fn real_gpg_round_trips_with_a_passphraseless_key() {
    let home = GnupgHome::new();
    home.gen_key("open@example.invalid", "");
    let entry = home.store().entry("gitlab-mcp/example").unwrap();
    entry
        .write_token("real-gpg-token", &["open@example.invalid".to_string()])
        .await
        .unwrap();

    // Exercises the DECRYPTION_OKAY success marker from the real binary.
    assert_eq!(entry.read_token().await.unwrap(), "real-gpg-token");
}

#[tokio::test]
#[ignore = "needs a real gpg binary; run with `make test-real-gpg`"]
async fn real_gpg_locked_key_is_classified_locked_then_unlocks() {
    let home = GnupgHome::new();
    home.gen_key("locked@example.invalid", PASSPHRASE);
    let entry = home.store().entry("gitlab-mcp/example").unwrap();
    entry
        .write_token("locked-token", &["locked@example.invalid".to_string()])
        .await
        .unwrap();
    home.kill_agent();

    // No cached passphrase + --pinentry-mode error → gpg-error code 85.
    let err = entry.read_token().await.unwrap_err();
    assert!(
        matches!(err, PassError::Locked { .. }),
        "expected Locked, got {err:?}"
    );

    // Prime the agent cache the way `gitlab-mcp unlock` would (loopback
    // instead of a pinentry), then the same non-interactive read succeeds.
    // (gpg writes `<output>.part` first, so the output can't be /dev/null.)
    let plaintext = home.path().join("plaintext");
    home.gpg(&[
        "--pinentry-mode",
        "loopback",
        "--passphrase",
        PASSPHRASE,
        "--output",
        plaintext.to_str().unwrap(),
        "--decrypt",
        entry.path().to_str().unwrap(),
    ]);
    assert_eq!(entry.read_token().await.unwrap(), "locked-token");
}

#[tokio::test]
#[ignore = "needs a real gpg binary; run with `make test-real-gpg`"]
async fn real_gpg_foreign_key_is_classified_no_secret_key() {
    let ours = GnupgHome::new();
    ours.gen_key("ours@example.invalid", "");
    let theirs = GnupgHome::new();
    theirs.gen_key("theirs@example.invalid", "");

    // Encrypted in *their* keyring, read with *ours* (same store files).
    let store_root = ours.store_root();
    let theirs_store =
        PassStore::new(&store_root, "gpg").with_homedir(Some(theirs.path().to_path_buf()));
    theirs_store
        .entry("gitlab-mcp/example")
        .unwrap()
        .write_token("foreign-token", &["theirs@example.invalid".to_string()])
        .await
        .unwrap();

    let err = ours
        .store()
        .entry("gitlab-mcp/example")
        .unwrap()
        .read_token()
        .await
        .unwrap_err();
    assert!(
        matches!(err, PassError::NoSecretKey { .. }),
        "expected NoSecretKey, got {err:?}"
    );
}
