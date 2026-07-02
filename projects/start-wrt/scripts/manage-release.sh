#!/bin/bash

# Local, human-run release toolkit for StartWRT, modeled on start-os's
# scripts/manage-release.sh. CI (.github/workflows/start-wrt.yaml, the `deploy`
# job — run via workflow_dispatch with deploy=release) builds the image, uploads
# the images to S3, and cuts a GitHub Release.
# Registering and indexing into the registry (and signing) are deliberately
# user-initiated and live here, so they sit as a human gate between "built +
# uploaded" and "live in the registry".
#
# Requirements on the workstation that runs this:
#   - start-cli on PATH (download from Start9Labs/start-os releases)
#   - the developer signing key at ~/.startos/developer.key.pem (registry auth)
#   - gh (authenticated), gpg, jq, sha256sum, b3sum
#   - s3cmd configured for DigitalOcean Spaces (nyc3) in ~/.s3cfg ONLY for the
#     manual 'upload' fallback; the normal flow relies on the CI S3 upload

set -e

# The monorepo hosts every product's releases; StartWRT's are tagged
# startwrt/v<version> (see start-wrt.yaml's deploy job) so per-product release
# cadences never collide on bare v* tags.
REPO="Start9Labs/start-technologies"
REGISTRY="https://startwrt-registry.start9.com"
S3_BUCKET="s3://startwrt-images"
S3_CDN="https://startwrt-images.nyc3.cdn.digitaloceanspaces.com"
PLATFORM="spacemit,k1-x"

# Registry asset slots this board publishes (as start-cli knows them). The
# sdcard image lives in the "img" slot; the OTA payload lives in the "squashfs"
# slot (see cmd_index for why a gzipped img goes there).
REGISTRY_SLOTS="img squashfs"

# StartWRT release signing key: the shared Start9 org key (uid
# "Start9 <dev@start9.com>", same key as start-os's START9_GPG_KEY and the one
# bundled at start-os/apt/start9.gpg). Its private half lives only in the
# releaser's local gpg keyring and never touches CI. The full fingerprint (not a
# short key id) is the canonical, collision-resistant identifier; the private
# half + passphrase are the secret, the fingerprint is public. The matching
# public key for out-of-band verification is committed at
# scripts/start9.key.asc. Override per-run via STARTWRT_GPG_KEY.
SIGNING_KEY="${STARTWRT_GPG_KEY:-5456DBFF1B9DF905041FA7765259ADFC2D63C217}"

# Name of the GitHub Actions build artifact that holds the images (see
# start-wrt.yaml, the `image` job's upload-artifact step). Must contain BOTH the
# sdcard .img and the sysupgrade .img.gz.
BUILD_ARTIFACT="startwrt-openwrt-image"

# The two image artifacts StartWRT publishes. build.mk renames the raw OpenWrt
# outputs to the startos naming convention —
# startwrt-<version>-<githash7>_spacemit-k1-{sdcard.img,sysupgrade.img.gz} —
# and these globs match on the role suffixes.
#   sdcard .img        -> fresh-install image (analog of start-os's .iso)
#   sysupgrade .img.gz -> OTA update image    (analog of start-os's .squashfs)
SDCARD_GLOB="*-sdcard.img"
SYSUPGRADE_GLOB="*-sysupgrade.img.gz"

parse_run_id() {
    local val="$1"
    if [[ "$val" =~ /actions/runs/([0-9]+) ]]; then
        echo "${BASH_REMATCH[1]}"
    else
        echo "$val"
    fi
}

require_version() {
    if [ -z "${VERSION:-}" ]; then
        read -rp "VERSION: " VERSION
        if [ -z "$VERSION" ]; then
            >&2 echo '$VERSION required'
            exit 2
        fi
    fi
    # Accept both "0.1.0-beta.3" and "v0.1.0-beta.3"
    VERSION="${VERSION#v}"
}

release_dir() {
    echo "$HOME/Downloads/startwrt-v$VERSION"
}

ensure_release_dir() {
    local dir
    dir=$(release_dir)
    if [ "$CLEAN" = "1" ]; then
        rm -rf "$dir"
    fi
    mkdir -p "$dir"
    cd "$dir"
}

enter_release_dir() {
    local dir
    dir=$(release_dir)
    if [ ! -d "$dir" ]; then
        >&2 echo "Release directory $dir does not exist. Run 'download' or 'pull' first."
        exit 1
    fi
    cd "$dir"
}

# Echo the image files present in the current (release) directory.
release_files() {
    for file in $SDCARD_GLOB $SYSUPGRADE_GLOB; do
        [ -f "$file" ] && echo "$file"
    done
}

resolve_gh_user() {
    GH_USER=${GH_USER:-$(gh api user -q .login 2>/dev/null || true)}
    GH_GPG_KEY=$(git config user.signingkey 2>/dev/null || true)
}

# Fetch the indexed S3/CDN URL for an asset slot from the registry index.
# Usage: registry_url <slot>   (uses $VERSION and $PLATFORM). Echoes the URL,
# or "null" if the slot isn't indexed for this version.
registry_url() {
    local slot=$1
    if [ -z "${_REGISTRY_INDEX:-}" ]; then
        _REGISTRY_INDEX=$(start-cli --registry=$REGISTRY registry os index)
    fi
    echo "$_REGISTRY_INDEX" | jq -r ".versions[\"$VERSION\"].${slot}[\"$PLATFORM\"].urls[0]"
}

# --- Subcommands ---

# Pull freshly-built images down from a GitHub Actions run so they can be
# published, indexed and signed. RUN_ID is the build workflow run.
cmd_download() {
    require_version

    if [ -z "${RUN_ID:-}" ]; then
        read -rp "RUN_ID (start-wrt build run): " RUN_ID
    fi
    RUN_ID=$(parse_run_id "${RUN_ID:-}")
    if [ -z "$RUN_ID" ]; then
        >&2 echo '$RUN_ID required'
        exit 2
    fi

    ensure_release_dir

    echo "Downloading build artifact '$BUILD_ARTIFACT' from run $RUN_ID..."
    while ! gh run download -R $REPO "$RUN_ID" -n "$BUILD_ARTIFACT" -D "$(pwd)"; do sleep 1; done

    echo "Images:"
    release_files | sed 's/^/  /'
}

# Pull an already-published version back from the registry and verify it end to
# end, so it can be inspected or co-signed. One verified download per image:
# `registry os asset get` validates the ed25519 registry signature + blake3
# commitment as it streams (--reverify re-checks the saved file). start-cli names
# the output startos-<ver>_<platform>.<ext>, so we move each into place under its
# published name (the basename of the indexed URL) to match release_files().
cmd_pull() {
    require_version
    ensure_release_dir

    for slot in $REGISTRY_SLOTS; do
        local url published tmp
        url=$(registry_url "$slot")
        if [ -z "$url" ] || [ "$url" = "null" ]; then
            >&2 echo "  (no '$slot' asset indexed for v$VERSION, skipping)"
            continue
        fi
        published=$(basename "$url")
        echo "Fetching + verifying $slot -> $published ..."
        tmp=$(mktemp -d "$(pwd)/.get-$slot.XXXXXX")
        start-cli --registry=$REGISTRY registry os asset get "$slot" "$VERSION" "$PLATFORM" -d "$tmp" --reverify
        mv -f "$tmp"/* "$published"
        rmdir "$tmp"
    done

    echo "Downloading signatures from the GitHub Release..."
    gh release download -R $REPO "startwrt/v$VERSION" -p "signatures.tar.gz" -D "$(pwd)" --clobber
    mkdir -p signatures
    tar -xzf signatures.tar.gz -C signatures

    cmd_verify
}

# Verify each pulled image against the StartWRT org GPG signature in signatures/.
# The ed25519 registry signature + blake3 integrity are already verified by
# 'pull' (asset get --reverify); this adds the human-provenance (GPG) layer.
# Exits nonzero if any image lacks a good signature.
cmd_verify() {
    require_version
    enter_release_dir

    [ -d signatures ] || { >&2 echo "No signatures/ dir; run 'pull' first."; exit 1; }

    # The org public key must already be in your keyring with its fingerprint
    # confirmed OUT OF BAND. gpg --verify reports success for any cryptographically
    # valid signature, even from an untrusted key, so blindly trusting the bundled
    # startwrt.key.asc would be TOFU. Print the fingerprint to compare.
    if [ -f signatures/startwrt.key.asc ]; then
        echo "Bundled org key fingerprint (confirm against known-good!):"
        gpg --show-keys --with-fingerprint signatures/startwrt.key.asc 2>/dev/null | sed 's/^/  /'
    fi

    local failed=0
    for file in $(release_files); do
        local sig="signatures/${file}.startwrt.asc"
        if [ ! -f "$sig" ]; then
            >&2 echo "MISSING  $sig"; failed=1; continue
        fi
        if gpg --verify "$sig" "$file" 2>/dev/null; then
            echo "OK   $file"
        else
            >&2 echo "BAD  $file (org signature did not verify)"; failed=1
        fi
    done

    [ "$failed" -eq 0 ] || { >&2 echo "Verification FAILED."; exit 1; }
    echo "All images verified (ed25519 registry sig via pull + org GPG sig)."
}

# Manual fallback: upload the images to S3 (the registry CDN serves them from
# here). The normal flow uploads via CI (start-wrt.yaml `deploy` job); use this
# only to re-publish a corrected image without a fresh CI run. Needs ~/.s3cfg.
cmd_upload() {
    require_version
    enter_release_dir

    for file in $(release_files); do
        echo "Uploading $file..."
        s3cmd put -P "$file" "$S3_BUCKET/v$VERSION/$file"
    done
}

# Register the version in the registry. The compat range is the set of
# installed versions allowed to upgrade to this one; the explicit beta floor
# (not the `^0.1.0` caret) keeps beta prerelease tags in range.
cmd_register() {
    require_version
    enter_release_dir
    start-cli --registry=$REGISTRY registry os version add "$VERSION" "v$VERSION" '' ">=0.1.0-beta.1 <=$VERSION"
}

# Index the images in the registry, pointing at their S3/CDN URLs. start-cli
# infers the asset kind from the file extension and only accepts iso/img/squashfs:
#   *-sdcard.img        -> "img" slot      (fresh-install image)
#   *-sysupgrade.img.gz -> "squashfs" slot (OTA update payload; start-os's
#       update-asset slot). The file is a gzipped raw image, not a real squashfs,
#       so we present it to start-cli under a .squashfs hardlink purely so the
#       kind resolves to squashfs. The indexed URL still points at the honestly
#       -named .img.gz on S3 (the registry only requires the URL's bytes to match
#       the signed blake3 commitment, which the hardlink shares).
cmd_index() {
    require_version
    enter_release_dir

    for file in $(release_files); do
        local index_file="$file"
        case "$file" in
            *.img.gz)
                index_file="${file%.img.gz}.squashfs"
                ln -f "$file" "$index_file"
                ;;
        esac
        echo "Indexing $file for platform $PLATFORM..."
        start-cli --registry=$REGISTRY registry os asset add --platform="$PLATFORM" --version="$VERSION" "$index_file" "$S3_CDN/v$VERSION/$file"
    done
}

cmd_sign() {
    require_version
    enter_release_dir
    resolve_gh_user

    if [ -z "$SIGNING_KEY" ]; then
        >&2 echo 'Error: signing key not set. Import the Start9 org key into your gpg'
        >&2 echo 'keyring and set STARTWRT_GPG_KEY (or the SIGNING_KEY constant in this script).'
        exit 1
    fi

    mkdir -p signatures

    for file in $(release_files); do
        gpg -u "$SIGNING_KEY" --detach-sign --armor -o "signatures/${file}.startwrt.asc" "$file"
        if [ -n "$GH_USER" ] && [ -n "$GH_GPG_KEY" ]; then
            gpg -u "$GH_GPG_KEY" --detach-sign --armor -o "signatures/${file}.${GH_USER}.asc" "$file"
        fi
    done

    gpg --export -a "$SIGNING_KEY" > signatures/startwrt.key.asc
    if [ -n "$GH_USER" ] && [ -n "$GH_GPG_KEY" ]; then
        gpg --export -a "$GH_GPG_KEY" > "signatures/${GH_USER}.key.asc"
    else
        >&2 echo 'Warning: could not determine GitHub user or GPG signing key, skipping personal signature'
    fi
    tar -czvf signatures.tar.gz -C signatures .

    gh release upload -R $REPO "startwrt/v$VERSION" signatures.tar.gz --clobber
}

cmd_cosign() {
    require_version
    enter_release_dir
    resolve_gh_user

    if [ -z "$GH_USER" ] || [ -z "$GH_GPG_KEY" ]; then
        >&2 echo 'Error: could not determine GitHub user or GPG signing key'
        >&2 echo "Set GH_USER and/or configure git user.signingkey"
        exit 1
    fi

    echo "Downloading existing signatures..."
    gh release download -R $REPO "startwrt/v$VERSION" -p "signatures.tar.gz" -D "$(pwd)" --clobber
    mkdir -p signatures
    tar -xzf signatures.tar.gz -C signatures

    echo "Adding personal signatures as $GH_USER..."
    for file in $(release_files); do
        gpg -u "$GH_GPG_KEY" --detach-sign --armor -o "signatures/${file}.${GH_USER}.asc" "$file"
    done

    gpg --export -a "$GH_GPG_KEY" > "signatures/${GH_USER}.key.asc"

    echo "Re-packing signatures..."
    tar -czvf signatures.tar.gz -C signatures .

    gh release upload -R $REPO "startwrt/v$VERSION" signatures.tar.gz --clobber
    echo "Done. Personal signatures for $GH_USER added to v$VERSION."
}

cmd_notes() {
    require_version
    enter_release_dir

    local sdcard sysupgrade
    sdcard=$(release_files | grep -E -- "-sdcard\.img$" | head -1)
    sysupgrade=$(release_files | grep -E -- "-sysupgrade\.img\.gz$" | head -1)

    cat << EOF
# Image Downloads

- [SD card image (fresh install)]($S3_CDN/v$VERSION/$sdcard "Write to microSD/eMMC to flash a new device")
- [Sysupgrade image (OTA update)]($S3_CDN/v$VERSION/$sysupgrade "In-place upgrade via OpenWrt sysupgrade")

EOF
    cat << 'EOF'
# StartWRT Checksums

## SHA-256
```
EOF
    sha256sum $(release_files)
    cat << 'EOF'
```

## BLAKE-3
```
EOF
    b3sum $(release_files)
    cat << 'EOF'
```
EOF
}

# CI already uploaded the images to S3, so the local gate is just:
# download (to sign/index locally) -> register -> index -> sign -> notes.
cmd_full_release() {
    cmd_download
    cmd_register
    cmd_index
    cmd_sign
    cmd_notes
}

usage() {
    cat << 'EOF'
Usage: manage-release.sh <subcommand>

Subcommands:
  download        Download freshly-built images from a start-wrt GH Actions run
                  Requires: RUN_ID
  pull            Download + fully verify an already-published version (registry
                  ed25519 sig + blake3 via asset get, then org GPG sig) and fetch
                  its signatures from the GitHub Release
  verify          Re-verify already-pulled images against the org GPG signature
  upload          Fallback: upload images to S3 (CI normally does this) — needs ~/.s3cfg
  register        Register the version in the StartWRT registry
  index           Add the images to the registry index (points at the CDN URLs)
  sign            Sign images with the StartWRT release signing key (STARTWRT_GPG_KEY)
                  plus your personal key if available, and upload signatures.tar.gz
                  to the GitHub Release
  cosign          Add a personal GPG signature to an existing release's signatures
                  (run 'pull' first so you can verify the images before signing)
  notes           Print release notes with download links and checksums
  full-release    Run: download -> register -> index -> sign -> notes
                  (CI already uploaded the images to S3)

Environment variables:
  VERSION          (required) Release version, e.g. 0.1.0-beta.3 (a leading 'v' is stripped)
  RUN_ID           GitHub Actions run ID for the start-wrt build workflow (download subcommand)
  STARTWRT_GPG_KEY GPG key id/fingerprint for the StartWRT release signature (sign subcommand)
  GH_USER          Override GitHub username (default: autodetected via gh cli)
  CLEAN            Set to 1 to wipe and recreate the release directory

Prerequisites: start-cli, ~/.startos/developer.key.pem, gh (authenticated), gpg,
jq, sha256sum, b3sum. s3cmd (DigitalOcean Spaces, nyc3) only for the 'upload'
fallback; the normal flow relies on the CI S3 upload.
EOF
}

case "${1:-}" in
    download)     cmd_download ;;
    pull)         cmd_pull ;;
    verify)       cmd_verify ;;
    upload)       cmd_upload ;;
    register)     cmd_register ;;
    index)        cmd_index ;;
    sign)         cmd_sign ;;
    cosign)       cmd_cosign ;;
    notes)        cmd_notes ;;
    full-release) cmd_full_release ;;
    *)            usage; exit 1 ;;
esac
