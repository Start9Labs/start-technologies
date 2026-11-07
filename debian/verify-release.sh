#!/usr/bin/env bash
#
# Usage: verify-release.sh <keyring> <InRelease> <output-Release>
#
# Writes the Release signed inside an InRelease. Fails unless the keyring holds
# the signer.
set -euo pipefail

keyring=$1
inrelease=$2
release=$3

# gpg --decrypt exits 0 on unsigned OpenPGP data.
status=$(gpg --no-default-keyring --keyring "$keyring" --batch --yes --status-fd 1 \
    --output "$release" --decrypt "$inrelease")
if ! grep -q '^\[GNUPG:\] VALIDSIG ' <<< "$status"; then
    echo "no valid signature in $inrelease" >&2
    exit 1
fi
