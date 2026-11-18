#!/bin/bash

# Uploads a public object to a commit-derived key. An existing key fails the upload unless it holds
# the same bytes or FORCE_OVERWRITE is true; promoted registries reference published URLs verbatim.
# Usage: s3-put-once.sh <file> <s3-key>

set -uo pipefail

file=$1
key=$2
if [ "${FORCE_OVERWRITE:-false}" != true ] && [ -n "$(s3cmd ls "$key")" ]; then
  # `s3cmd put` records the MD5 that `s3cmd info` reports.
  if [ "$(s3cmd info "$key" | sed -n 's/^ *MD5 sum: *//p')" = "$(md5sum "$file" | cut -d' ' -f1)" ]; then
    echo "$key already holds $file."
    exit 0
  fi
  echo "::error::$key already exists — this commit has been published before. Overwriting it would invalidate any beta or production commitment pointing at that URL. Re-dispatch with force-overwrite to replace it deliberately."
  exit 1
fi
echo "Uploading $file..."
s3cmd put -P "$file" "$key"
