#!/usr/bin/env bash
# Decide whether CodeQL has anything to analyse and write `scan=true|false` to
# $GITHUB_OUTPUT. The analyze job always runs (it is a required check); only
# its expensive steps are gated on this output.
#
# Env: EVENT_NAME (github.event_name), BEFORE (github.event.before, push only).
#
#   pull_request                      diff against HEAD^1 (the PR's base side)
#   push                              diff against $BEFORE, so every commit of a
#                                     rebased or multi-commit push is covered
#   push without usable $BEFORE,      always scan
#   schedule, anything else
set -euo pipefail

event=${EVENT_NAME:?EVENT_NAME must be set}
before=${BEFORE:-}
out=${GITHUB_OUTPUT:-/dev/stdout}
summary=${GITHUB_STEP_SUMMARY:-/dev/null}

pattern='\.rs$|(^|/)Cargo\.(toml|lock)$|^rust-toolchain\.toml$|^\.github/(workflows/codeql\.yml|scripts/codeql-scope\.sh|codeql/)'
zero_sha=0000000000000000000000000000000000000000

case "$event" in
pull_request) base=HEAD^1 ;;
push)
	if [ -z "$before" ] || [ "$before" = "$zero_sha" ] || ! git cat-file -e "${before}^{commit}" 2>/dev/null; then
		base=
	else
		base=$before
	fi
	;;
*) base= ;;
esac

if [ -z "$base" ]; then
	echo "scan=true" >>"$out"
	exit 0
fi

# Collect first: `grep -q` exiting early would SIGPIPE `git diff` under pipefail.
changed=$(git diff --name-only "$base" HEAD)

if grep -qE "$pattern" <<<"$changed"; then
	echo "scan=true" >>"$out"
else
	echo "scan=false" >>"$out"
	echo "No Rust-relevant changes; skipping analysis." >>"$summary"
fi
