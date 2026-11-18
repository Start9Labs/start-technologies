#!/bin/bash
# Prints the ids of runs on a branch holding an unexpired artifact whose name
# matches a glob, newest artifact first: <repo> <name-glob> <branch>.
# Expiration does not follow listing order. Matching artifacts must be retained
# at most 14 days: the search ends at the first artifact created over 15 days ago.

set -euo pipefail

repo=$1
glob=$2
branch=$3
declare -A seen=()
for ((page = 1; ; page++)); do
    rows=$(gh api "repos/$repo/actions/artifacts?per_page=100&page=$page" \
        --jq '.artifacts[] | [.name, .expired, .workflow_run.head_branch, .workflow_run.id,
            ((.created_at | fromdateiso8601) < (now - 15 * 86400))] | @tsv')
    [ -n "$rows" ] || break
    while IFS=$'\t' read -r name expired head_branch run_id stale; do
        [ "$stale" = false ] || exit 0
        # shellcheck disable=SC2053
        [[ $name == $glob ]] || continue
        [ "$expired" = false ] || continue
        if [ "$head_branch" = "$branch" ] && [ -z "${seen[$run_id]:-}" ]; then
            seen[$run_id]=1
            echo "$run_id"
        fi
    done <<< "$rows"
done
