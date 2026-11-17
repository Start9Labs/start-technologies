#!/bin/bash

profile=${1:-release}
case "$profile" in
  release|debug) ;;
  *)
    >&2 echo "Unknown profile $profile: falling back to debug..."
    profile=debug
    ;;
esac
printf '%s\n' "$profile"
