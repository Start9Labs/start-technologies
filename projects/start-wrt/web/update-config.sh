#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")"

set -e

CONFIG=$(if [ -f config.json ]; then cat config.json; else cat config-sample.json; fi)
echo "$CONFIG" \
    | jq '.useMocks = false' \
    | jq ".gitHash = \"$(cat ../build/env/GIT_HASH.txt)\"" \
    > config.json
