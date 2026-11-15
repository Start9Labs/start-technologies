#!/bin/bash
set -euo pipefail

directory=${UPDATE_DIRECTORY:?Set UPDATE_DIRECTORY to the verified update artifact directory}
report="$directory/report.json"
candidate=$(jq -er '.candidate' "$report")
base=$(git rev-parse HEAD)
branch="automation/nut-$(printf '%s' "$candidate" | sha256sum | cut -c 1-16)"

# Preserve maintainer edits across interrupted proposal runs.
existing=$(gh pr list --repo "$GITHUB_REPOSITORY" --state all --head "$branch" --json url --jq '.[0].url // empty')
if [ -n "$existing" ]; then
    printf 'NUT %s already proposed: %s\n' "$candidate" "$existing"
    exit 0
fi

git apply "$directory/update.patch"
mapfile -t files < <(jq -r '.files[]' "$report")
git add -- "${files[@]}"
# GitHub signs commits created by the shared API helper.
remote=$(git ls-remote origin "refs/heads/$branch")
if [ -z "$remote" ]; then
    gh api "repos/$GITHUB_REPOSITORY/git/refs" -f "ref=refs/heads/$branch" -f "sha=$base" >/dev/null
    remote_head=$base
else
    git fetch --no-tags origin "refs/heads/$branch"
    remote_head=$(git rev-parse FETCH_HEAD)
fi
if [ "$remote_head" = "$base" ]; then
    printf 'chore(start-os): update NUT to %s\n' "$candidate" > "$directory/commit-message.txt"
    node scripts/commit-staged.mjs "$directory/commit-message.txt" "$branch" "$base"
else
    # Resume publication after a committed proposal.
    while IFS= read -r path; do
        allowed=false
        for file in "${files[@]}"; do if [ "$path" = "$file" ]; then allowed=true; fi; done
        if [ "$allowed" != true ]; then
            echo "::error::Existing $branch carries changes outside the verified update; leaving it for manual review."
            exit 1
        fi
    done < <(git diff --name-only "$base" FETCH_HEAD)
    if ! git diff --quiet FETCH_HEAD -- "${files[@]}"; then
        echo "::error::Existing $branch has different content; leaving it for manual review."
        exit 1
    fi
fi

draft=()
if [ "$(cat "$directory/release-aligned")" != true ]; then draft=(--draft); fi
node projects/start-os/build/nut/render-update-pr.mjs "$directory"

gh pr create --repo "$GITHUB_REPOSITORY" --base master --head "$branch" \
    --title "chore(start-os): update NUT to $candidate" \
    --label StartOS --label repo --label patch "${draft[@]}" \
    --body-file "$directory/pr-body.md"
