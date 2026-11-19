#!/bin/bash
# Prints the ids of runs on a branch holding an unexpired artifact:
# <repo> <artifact-name> <branch>.

set -euo pipefail

# Reruns preserve run creation time; artifact expiry determines availability.
GHA_ARTIFACT_BRANCH="$3" gh api --method GET --paginate "repos/$1/actions/artifacts" \
    -f name="$2" -F per_page=100 \
    --jq '.artifacts[]
      | select((.expired | not) and .workflow_run.head_branch == env.GHA_ARTIFACT_BRANCH)
      | .workflow_run.id' | sort -nu
