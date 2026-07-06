#!/usr/bin/env bash
# Bootstrap launcher for the gitlab-mcp Claude Code plugin.
#
# On first run (and after every plugin update) this downloads the release
# binary matching the plugin's version from GitHub releases, verifies its
# SHA-256 against the release's checksums.txt, caches it under
# ~/.cache/gitlab-mcp/<version>/, and execs it. Subsequent runs exec the
# cached binary directly. If the download fails, a gitlab-mcp already on
# PATH is used as a fallback.
#
# All diagnostics go to stderr: stdout is the MCP stdio transport.
set -euo pipefail

REPO="3kirt/gitlab-mcp"
PLUGIN_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

say() { echo "gitlab-mcp plugin: $*" >&2; }

# The binary version is pinned to the plugin version, so a plugin update
# (which ships a new plugin.json) also rolls the binary forward.
version="$(sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
  "$PLUGIN_ROOT/.claude-plugin/plugin.json" | head -n 1)"
if [ -z "$version" ]; then
  say "cannot read version from $PLUGIN_ROOT/.claude-plugin/plugin.json"
  exit 1
fi

case "$(uname -s)" in
  Darwin) os="darwin" ;;
  Linux) os="linux" ;;
  *)
    say "unsupported OS: $(uname -s) (prebuilt binaries cover macOS and Linux)"
    say "build from source instead: cargo install --git https://github.com/$REPO"
    exit 1
    ;;
esac
case "$(uname -m)" in
  arm64 | aarch64) arch="arm64" ;;
  x86_64 | amd64) arch="amd64" ;;
  *)
    say "unsupported architecture: $(uname -m)"
    say "build from source instead: cargo install --git https://github.com/$REPO"
    exit 1
    ;;
esac
asset="gitlab-mcp-$os-$arch"

cache_dir="${XDG_CACHE_HOME:-$HOME/.cache}/gitlab-mcp/v$version"
bin="$cache_dir/gitlab-mcp"

if [ ! -x "$bin" ]; then
  base="https://github.com/$REPO/releases/download/v$version"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT

  say "downloading $asset v$version from GitHub releases..."
  if ! curl -fsSL --retry 2 -o "$tmp/$asset" "$base/$asset" ||
    ! curl -fsSL --retry 2 -o "$tmp/checksums.txt" "$base/checksums.txt"; then
    if command -v gitlab-mcp >/dev/null 2>&1; then
      say "download failed; falling back to $(command -v gitlab-mcp)"
      exec gitlab-mcp "$@"
    fi
    say "download failed (no release asset for $asset v$version?)"
    say "install manually with: cargo install --git https://github.com/$REPO"
    exit 1
  fi

  expected="$(awk -v a="$asset" '$2 == a { print $1 }' "$tmp/checksums.txt")"
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$tmp/$asset" | awk '{ print $1 }')"
  else
    actual="$(shasum -a 256 "$tmp/$asset" | awk '{ print $1 }')"
  fi
  if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then
    say "checksum mismatch for $asset (expected ${expected:-<missing>}, got $actual); aborting"
    exit 1
  fi

  chmod +x "$tmp/$asset"
  mkdir -p "$cache_dir"
  mv "$tmp/$asset" "$bin"
  say "installed $bin"
fi

exec "$bin" "$@"
