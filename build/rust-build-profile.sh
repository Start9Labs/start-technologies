#!/bin/bash

# Normalizes PROFILE to release, the default, or debug, and sets BUILD_FLAGS to its cargo flags.
set_build_profile() {
  PROFILE=${PROFILE:-release}
  BUILD_FLAGS=()
  case "$PROFILE" in
    release) BUILD_FLAGS=(--release) ;;
    debug) ;;
    *)
      >&2 echo "Unknown profile $PROFILE: falling back to debug..."
      PROFILE=debug
      ;;
  esac
}
