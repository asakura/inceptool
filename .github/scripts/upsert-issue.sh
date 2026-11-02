#!/usr/bin/env bash
# Usage: upsert-issue.sh <comment|replace|close> <title>
# Body comes from $BODY; GH_TOKEN and GH_REPO come from the environment.
#   comment  open an issue, or comment on the open one
#   replace  open an issue, or replace the body of the open one
#   close    close the open issue with $BODY as the comment; no-op if none
set -euo pipefail

mode=${1:?usage: upsert-issue.sh <comment|replace|close> <title>}
title=${2:?usage: upsert-issue.sh <comment|replace|close> <title>}
body=${BODY:?BODY must be set}

existing=$(gh issue list --state open --search "\"${title}\" in:title" \
	--json number --jq '.[0].number // empty')

if [ "$mode" = close ]; then
	if [ -n "$existing" ]; then
		gh issue close "$existing" --comment "$body"
	fi
	exit 0
fi

if [ -z "$existing" ]; then
	gh issue create --title "$title" --body "$body"
	exit 0
fi

case "$mode" in
comment) gh issue comment "$existing" --body "$body" ;;
replace) gh issue edit "$existing" --body "$body" ;;
*)
	echo "unknown mode: $mode" >&2
	exit 2
	;;
esac
