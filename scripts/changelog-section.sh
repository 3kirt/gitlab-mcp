#!/bin/sh
# Print the CHANGELOG.md body for one version (the lines between
# `## [X.Y.Z] - YYYY-MM-DD` and the next `## [`), for release notes and the
# release workflow's verify step. Exits non-zero if the section is missing or
# empty. Usage: scripts/changelog-section.sh 0.40.0 (a leading v is ignored).
# Shared with netbox-mcp and proxmox-mcp.
set -eu
ver="${1#v}"
file="${2:-CHANGELOG.md}"
body="$(awk -v ver="$ver" '
  index($0, "## [" ver "] - ") == 1 { found = 1; next }
  found && /^## \[/ { exit }
  found { print }
' "$file" | sed -e '/./,$!d')"
if [ -z "$(printf '%s' "$body" | tr -d '[:space:]')" ]; then
  echo "changelog-section: no non-empty section for $ver in $file" >&2
  exit 1
fi
printf '%s\n' "$body"
