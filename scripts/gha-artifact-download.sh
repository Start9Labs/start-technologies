#!/bin/bash
# Saves a run's unexpired artifacts whose names match a glob into a directory,
# each as a file named for its artifact: <repo> <run-id> <name-glob> <dir>.
# For artifacts uploaded with `archive: false`, which `gh run download` cannot
# fetch (cli/cli#13012). Fails when nothing matches.

set -euo pipefail

repo=$1
run_id=$2
glob=$3
dir=$4
rows=$(gh api --paginate "repos/$repo/actions/runs/$run_id/artifacts?per_page=100" \
    --jq '.artifacts[] | select(.expired | not) | [.id, .name] | @tsv')
mkdir -p "$dir"
found=0
while IFS=$'\t' read -r id name; do
    # shellcheck disable=SC2053
    [[ -n $name && $name == $glob ]] || continue
    gh api "repos/$repo/actions/artifacts/$id/zip" > "$dir/$name.part" < /dev/null
    mv -f "$dir/$name.part" "$dir/$name"
    found=1
done <<< "$rows"
if [ "$found" = 0 ]; then
    >&2 echo "Run $run_id has no unexpired artifact matching $glob"
    exit 1
fi
