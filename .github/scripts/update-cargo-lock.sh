#!/usr/bin/env bash
# Update Cargo.lock for transitive dependencies only: run `cargo update`, then
# pin every workspace direct dependency back to its previous version.
# Run inside the devShell (needs cargo and jq).
set -euo pipefail

direct_deps=$(cargo metadata --format-version 1 | jq -r '
  .workspace_members as $members |
  .packages[] | select(.id | IN($members[])).dependencies[].name
' | sort -u)

update_log=$(mktemp)
trap 'rm -f "$update_log"' EXIT

cargo update 2>&1 | tee "$update_log"

{ grep -E '^\s*(Updating|Downgrading)\s+' "$update_log" || true; } |
	while read -r _action pkg old_v _arrow _new_v; do
		if grep -qx "$pkg" <<<"$direct_deps"; then
			echo "Reverting direct dependency $pkg back to ${old_v#v}"
			cargo update -p "$pkg" --precise "${old_v#v}"
		fi
	done
