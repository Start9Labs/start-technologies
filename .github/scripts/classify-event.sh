#!/bin/bash

# Prints changed-products.mjs step outputs for the products the named workflow builds. A pull request
# classifies its files, and a push its diff from that workflow's last successful push build on the
# branch. Other events, and branches without such a build, rebuild every product.
# Usage: classify-event.sh <workflow-file>; pull requests also need PR_NUMBER.

set -euo pipefail

workflow=$1
classify() { node "$(dirname "${BASH_SOURCE[0]}")/changed-products.mjs" --workflow "$workflow" "$@"; }

case "$GITHUB_EVENT_NAME" in
  pull_request)
    gh api --paginate "repos/$GITHUB_REPOSITORY/pulls/$PR_NUMBER/files" \
      --jq '.[] | .filename, (.previous_filename // empty)' | classify --event pull_request
    ;;
  push)
    # A superseded queued push's changes still build. Ten pages bound the search.
    base=""
    page=1
    while [ "$page" -le 10 ]; do
      selection="$(gh api "repos/$GITHUB_REPOSITORY/actions/workflows/$workflow/runs?per_page=100&page=$page" \
        --jq '[(.workflow_runs | length), ([.workflow_runs[]
          | select(.event == "push" and .head_branch == env.GITHUB_REF_NAME
              and .head_repository.full_name == env.GITHUB_REPOSITORY
              and .status == "completed" and .conclusion == "success"
              and .id < (env.GITHUB_RUN_ID | tonumber))]
          | max_by(.run_number) | .head_sha)] | @tsv')"
      read -r run_count base <<< "$selection"
      if [ -n "$base" ] || [ "$run_count" -eq 0 ]; then
        break
      fi
      page=$((page + 1))
    done
    if [ -n "$base" ] && git fetch -q --no-tags --depth=1 origin "$base"; then
      git diff --no-renames --name-only "$base" "$GITHUB_SHA" | classify --event push
    else
      classify --all
    fi
    ;;
  *)
    classify --all
    ;;
esac
