#!/usr/bin/env python3
"""Smoke-test a built gitlab-mcp binary: `make smoke BIN=path`.

Runs the binary the way a release user would get it, with throwaway dummy
config and no network access:

  1. `<bin> --version` names the expected version (Cargo.toml's, or
     SMOKE_VERSION, e.g. the release tag);
  2. an MCP stdio session (initialize -> notifications/initialized ->
     tools/list, following nextCursor) returns exactly EXPECTED_TOOLS tools.

tools/list never contacts GitLab, so a dummy URL/token is enough; the tool
count catches a router that silently lost (or duplicated) tools in the
release build. Stdlib only, so it runs on every CI runner as-is.

Shared shape with netbox-mcp and proxmox-mcp; the REPO PARAMETERS block is
the only per-repo part.
"""

import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

# --- REPO PARAMETERS ---------------------------------------------------------
BIN_NAME = "gitlab-mcp"
# Must change together with `combined_router_exposes_every_tool` in
# src/tools/mod.rs.
EXPECTED_TOOLS = 191
# No real host/token; must not match `make secrets-scan`.
DUMMY_ENV = {"GITLAB_URL": "https://localhost:1", "GITLAB_TOKEN": "dummy-smoke"}
# -----------------------------------------------------------------------------

TIMEOUT = 30
ROOT = Path(__file__).resolve().parent.parent


def fail(msg: str) -> None:
    print(f"smoke: FAIL: {msg}", file=sys.stderr)
    sys.exit(1)


def expected_version() -> str:
    if v := os.environ.get("SMOKE_VERSION", "").removeprefix("v"):
        return v
    m = re.search(r'^version = "([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.M)
    if not m:
        fail("no version in Cargo.toml")
    return m.group(1)


def env_for(home: str) -> dict:
    # A clean environment: HOME points at an empty dir so the user's
    # ~/.gitlab_mcp.json can't leak in. Keep PATH (and the arch for Rosetta).
    env = {k: v for k, v in os.environ.items() if k in ("PATH", "TMPDIR", "LANG")}
    env.update(DUMMY_ENV, HOME=home)
    return env


def check_version(binary: str, env: dict) -> None:
    out = subprocess.run(
        [binary, "--version"], env=env, capture_output=True, text=True, timeout=TIMEOUT
    )
    want = f"{BIN_NAME} {expected_version()}"
    if out.returncode != 0 or out.stdout.strip() != want:
        fail(f"--version: want {want!r}, got {out.stdout.strip()!r} {out.stderr.strip()!r}")
    print(f"smoke: {want}")


def check_tools(binary: str, env: dict) -> None:
    proc = subprocess.Popen(
        [binary],
        env=env,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    next_id = 0

    def request(method: str, params: dict) -> dict:
        nonlocal next_id
        next_id += 1
        send({"jsonrpc": "2.0", "id": next_id, "method": method, "params": params})
        while True:
            line = proc.stdout.readline()
            if not line:
                fail(f"server closed stdout during {method}: {proc.stderr.read()}")
            msg = json.loads(line)
            if msg.get("id") == next_id:
                if "error" in msg:
                    fail(f"{method}: {msg['error']}")
                return msg["result"]

    def send(msg: dict) -> None:
        proc.stdin.write(json.dumps(msg) + "\n")
        proc.stdin.flush()

    try:
        init = request(
            "initialize",
            {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "smoke", "version": "0"},
            },
        )
        if "tools" not in init.get("capabilities", {}):
            fail(f"initialize: no tools capability in {init.get('capabilities')}")
        send({"jsonrpc": "2.0", "method": "notifications/initialized"})

        tools, cursor = [], None
        while True:
            page = request("tools/list", {"cursor": cursor} if cursor else {})
            tools += page["tools"]
            cursor = page.get("nextCursor")
            if not cursor:
                break
    finally:
        proc.stdin.close()
        try:
            proc.wait(timeout=TIMEOUT)
        except subprocess.TimeoutExpired:
            proc.kill()

    names = [t["name"] for t in tools]
    if len(set(names)) != len(names):
        fail("tools/list returned duplicate tool names")
    if len(tools) != EXPECTED_TOOLS:
        fail(f"tools/list: want {EXPECTED_TOOLS} tools, got {len(tools)}")
    print(f"smoke: initialize + tools/list OK ({len(tools)} tools)")


def main() -> None:
    if len(sys.argv) != 2:
        fail("usage: smoke.py <binary>")
    binary = str(Path(sys.argv[1]).resolve())
    if not os.access(binary, os.X_OK):
        fail(f"{binary} is not an executable file")
    with tempfile.TemporaryDirectory() as home:
        env = env_for(home)
        check_version(binary, env)
        check_tools(binary, env)


if __name__ == "__main__":
    main()
