#!/bin/bash
# Prints the newest successful run on a branch holding an unexpired
# <platform>.squashfs: <repo> <branch> <platform>.

set -euo pipefail

repo=$1
branch=$2
platform=$3
selected_id=""
selected_at=""
# Images expire after 14 days; a listing page with no newer run ends the search.
cutoff=$(jq -nr 'now - 15 * 86400 | todate')

# Successful product-only runs have no StartOS image artifacts.
for workflow in startos-iso.yaml build-products.yaml; do
    page=1
    found=false
    while :; do
        response=$(gh api "repos/$repo/actions/workflows/$workflow/runs?per_page=100&page=$page")
        jq -e --arg cutoff "$cutoff" 'any(.workflow_runs[]; .created_at >= $cutoff)' \
            <<< "$response" > /dev/null || break
        candidates=$(jq -r --arg branch "$branch" '
            [.workflow_runs[] | select(.head_branch == $branch and .conclusion == "success")]
            | sort_by(.created_at) | reverse | .[] | [.id, .created_at] | @tsv
        ' <<< "$response")
        while IFS=$'\t' read -r run_id created_at; do
            [ -n "$run_id" ] || continue
            if [ -n "$selected_at" ] && [[ "$created_at" < "$selected_at" ]]; then
                found=true
                break
            fi
            artifacts=$(gh api --paginate --slurp "repos/$repo/actions/runs/$run_id/artifacts?per_page=100")
            if jq -e --arg name "$platform.squashfs" '
                [.[] | .artifacts[]] | any(.name == $name and (.expired | not))
            ' <<< "$artifacts" > /dev/null; then
                selected_id=$run_id
                selected_at=$created_at
                found=true
                break
            fi
        done <<< "$candidates"
        [ "$found" = false ] || break
        page=$((page + 1))
    done
done

if [ -z "$selected_id" ]; then
    >&2 echo "No successful run on $branch has an unexpired $platform.squashfs artifact"
    exit 1
fi
printf '%s\n' "$selected_id"
