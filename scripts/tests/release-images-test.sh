#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
source <(awk '/^SUBCOMMAND=/{exit} {print}' "$ROOT/scripts/manage-release.sh")
REPO_ROOT=$ROOT
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
PROJECT=start-os
KIND=os
VERSION=1.2.3
TAG=start-os/v1.2.3
OS_PLATFORMS=raspberrypi
mkdir -p "$TMP/published"

b3sum() {
    if [ "${1:-}" = --no-names ]; then
        sha256sum "$2" | cut -d' ' -f1
    else
        sha256sum "$@"
    fi
}
load_registry_index() { :; }
resolve_os_commit() { :; }
reusable_os_release() { :; }
asset_url() {
    [ "$2" != img ] || echo "$S3_CDN/v$VERSION/$PUBLISHED"
}
asset_commitment_b3() { b3sum --no-names "$TMP/published/$PUBLISHED"; }
# start-cli names its download for the slot, not the published file.
start-cli() {
    cp "$TMP/published/$PUBLISHED" "${@: -1}/startos-${VERSION}_raspberrypi.img"
}

cmd_pre_check() {
    DONE[tag]=1
    DONE[create-gh-release]=1
    DONE[publish-gh-release]=1
    DONE[index]=1
}
cmd_sign() {
    [ "$(release_files)" = "$PUBLISHED" ]
    cmp "$TMP/published/$PUBLISHED" "$(release_dir)/$PUBLISHED"
}
HOME=$TMP/release/home
PUBLISHED=startos-1.2.3-abc_raspberrypi.img.gz
printf 'published image\n' | gzip -c > "$TMP/published/$PUBLISHED"
cmd_release
notes=$(generated_sections)
grep -Fq "$S3_CDN/v$VERSION/$PUBLISHED" <<< "$notes"
grep -Fq "  $PUBLISHED" <<< "$notes"
grep -Fq 'gzip-compressed' <<< "$notes"

HOME=$TMP/gha/home
resolve_gha_run() { RUN_ID=1; RUN_SHA=abc; ensure_release_dir; }
SCRIPT_DIR=$TMP/bin
mkdir -p "$SCRIPT_DIR"
export CI_IMG=$TMP/published/$PUBLISHED
cat > "$SCRIPT_DIR/gha-artifact-download.sh" <<'SCRIPT'
#!/bin/bash
case "$3" in
    'startos-*_raspberrypi.img.gz') cp "$CI_IMG" "$4/startos-1.2.3-def_raspberrypi.img.gz" ;;
    'startos-*_raspberrypi.squashfs') : > "$4/startos-1.2.3-def_raspberrypi.squashfs" ;;
    *) echo "unexpected artifact $3" >&2; exit 1 ;;
esac
SCRIPT
chmod +x "$SCRIPT_DIR/gha-artifact-download.sh"
(cmd_pull_gha)
cmp "$CI_IMG" "$(release_dir)/startos-1.2.3-def_raspberrypi.img.gz"
[ ! -e "$(release_dir)/startos-1.2.3-def_raspberrypi.img" ]
echo 'Release image tests passed.'
