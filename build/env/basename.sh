#!/bin/bash

PROJECT=${PROJECT:-"startos"}

cd "$(dirname "${BASH_SOURCE[0]}")"

PLATFORM="$(if [ -f ./PLATFORM.txt ]; then cat ./PLATFORM.txt; else echo unknown; fi)"
VERSION="$(./version.sh "$PROJECT")"
GIT_HASH="$(cat ./GIT_HASH.txt)"
if [[ "$GIT_HASH" =~ ^@ ]]; then
  GIT_HASH=unknown
else
  GIT_HASH="$(echo -n "$GIT_HASH" |  head -c 7)"
fi
STARTOS_ENV="$(cat ./ENVIRONMENT.txt)"
VERSION_FULL="${VERSION}-${GIT_HASH}"
if [ -n "$STARTOS_ENV" ]; then
  VERSION_FULL="$VERSION_FULL~${STARTOS_ENV}"
fi

echo -n "${PROJECT}-${VERSION_FULL}_${PLATFORM}"