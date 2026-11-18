#!/bin/bash

set_build_profile() {
  select_profile release debug
}

set_test_profile() {
  select_profile unit-test release dev debug
}

set_generated_profile() {
  select_profile generated release dev debug
}

# Unknown profiles fall back to debug; every accepted-profile list must include debug.
select_profile() {
  PROFILE=${PROFILE:-$1}
  local allowed_profile
  for allowed_profile in "$@"; do
    if [[ "$PROFILE" == "$allowed_profile" ]]; then
      break
    fi
  done
  if [[ "$PROFILE" != "$allowed_profile" ]]; then
    >&2 echo "Unknown profile $PROFILE: falling back to debug..."
    PROFILE=debug
  fi
  case "$PROFILE" in
    release) BUILD_FLAGS=(--release) ;;
    dev|debug) BUILD_FLAGS=() ;;
    *) BUILD_FLAGS=(--profile "$PROFILE") ;;
  esac
}
