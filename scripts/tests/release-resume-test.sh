#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
source <(awk '/^SUBCOMMAND=/{exit} {print}' "$ROOT/scripts/manage-release.sh")
REPO_ROOT=$ROOT
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
HOME=$TMP/home
PROJECT=start-os
KIND=os
VERSION=1.2.3
TAG=start-os/v1.2.3
OS_PLATFORMS=raspberrypi
mkdir -p "$TMP/published"
printf 'published image\n' > "$TMP/published/image.img"
touch -t 202001010000 "$TMP/published/image.img"
gzip -c "$TMP/published/image.img" > "$TMP/published/image.img.gz"

b3sum() {
    if [ "$#" -eq 2 ]; then sha256sum "$2"; else sha256sum; fi | cut -d' ' -f1
}
load_registry_index() { :; }
resolve_os_commit() { :; }
asset_url() {
    [ "$2" != img ] || echo "$S3_CDN/v$VERSION/image.img"
}
asset_commitment_b3() { b3sum --no-names "$TMP/published/image.img"; }
start-cli() {
    cp "$TMP/published/image.img" "${@: -1}/download.img"
}
published_exists() { [ "${PUBLISHED_GZ:-1}" = 1 ]; }
curl() {
    local output="" arg
    while [ "$#" -gt 0 ]; do
        arg=$1
        shift
        if [ "$arg" = -o ]; then output=$1; shift; fi
    done
    cp "$TMP/published/image.img.gz" "$output"
}

cmd_pre_check() {
    DONE[push-gz]=1
    DONE[tag]=1
    DONE[create-gh-release]=1
    DONE[index]=1
}
cmd_sign() {
    cmp "$TMP/published/image.img.gz" "$(release_dir)/image.img.gz"
}
cmd_release
cmd_pull
ensure_img_gz
cmp "$TMP/published/image.img.gz" "$(release_dir)/image.img.gz"

rm -f "$(release_dir)/image.img.gz"
PUBLISHED_GZ=0
cmd_pull
gzip -dc "$(release_dir)/image.img.gz" | cmp "$TMP/published/image.img" -

PUBLISHED_GZ=1
rm -f "$(release_dir)/image.img.gz"
printf 'wrong image\n' | gzip -c > "$TMP/published/image.img.gz"
if (cmd_pull); then
    echo 'Mismatched published gzip was accepted' >&2
    exit 1
fi
[ ! -f "$(release_dir)/image.img.gz" ]
printf 'not a gzip\n' > "$TMP/published/image.img.gz"
if (cmd_pull); then
    echo 'Corrupt published gzip was accepted' >&2
    exit 1
fi
[ ! -f "$(release_dir)/image.img.gz" ]
echo 'Release gzip resume tests passed.'
