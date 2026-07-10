#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")"

if [ "$GIT_BRANCH_AS_HASH" != 1 ]; then
    # git status (not diff-index): builds touch tracked files (e.g. the make
    # recipe for package-lock.json) without changing content, and stat-only
    # diff-index misreads that as a dirty tree. status compares content.
    GIT_HASH="$(git rev-parse HEAD)$(if [ -n "$(git status --porcelain --untracked-files=no)" ]; then echo '-modified'; fi)"
else
    GIT_HASH="@$(git rev-parse --abbrev-ref HEAD)"
fi

if ! [ -f ./GIT_HASH.txt ] || [ "$(cat ./GIT_HASH.txt)" != "$GIT_HASH" ]; then
    >&2 echo Git hash changed from "$([ -f ./GIT_HASH.txt ] && cat ./GIT_HASH.txt)" to "$GIT_HASH"
    echo -n "$GIT_HASH" > ./GIT_HASH.txt
fi

echo -n ./build/env/GIT_HASH.txt