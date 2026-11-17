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

# Normalizes PROFILE to one of the given profiles, the first by default and debug when unknown, and
# sets BUILD_FLAGS to its cargo flags. Every list must include debug.
select_profile() {
  PROFILE=${PROFILE:-$1}
  if [[ " $* " != *" $PROFILE "* ]]; then
    >&2 echo "Unknown profile $PROFILE: falling back to debug..."
    PROFILE=debug
  fi
  case "$PROFILE" in
    release) BUILD_FLAGS=(--release) ;;
    dev|debug) BUILD_FLAGS=() ;;
    *) BUILD_FLAGS=(--profile "$PROFILE") ;;
  esac
}
