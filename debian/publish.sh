#!/bin/bash
#
# Publish .deb files to an S3-hosted apt repository.
#
# Usage: publish-deb.sh <deb-file-or-directory> [<deb-file-or-directory> ...]
#
# Environment variables:
#   GPG_PRIVATE_KEY  - Armored GPG private key (imported if set)
#   GPG_KEY_ID       - GPG key ID for signing
#   S3_ACCESS_KEY    - S3 access key
#   S3_SECRET_KEY    - S3 secret key
#   S3_ENDPOINT      - S3 endpoint (default: https://nyc3.digitaloceanspaces.com)
#   S3_BUCKET        - S3 bucket name (default: start9-debs)
#   SUITE            - Apt suite name (default: stable). Each suite owns a
#                      separate pool, so suites never evict each other.
#   COMPONENT        - Apt component name (default: main)

set -e

if [ $# -eq 0 ]; then
    echo "Usage: $0 <deb-file-or-directory> [...]" >&2
    exit 1
fi

BUCKET="${S3_BUCKET:-start9-debs}"
ENDPOINT="${S3_ENDPOINT:-https://nyc3.digitaloceanspaces.com}"
GPG_KEY_ID="${GPG_KEY_ID:-5259ADFC2D63C217}"
SUITE="${SUITE:-stable}"
COMPONENT="${COMPONENT:-main}"
REPO_DIR="$(mktemp -d)"

# Every suite owns its own pool, so publishing to one can never index or evict
# another's .deb — the eviction below, the Packages scan, and both syncs are all
# scoped to POOL_ROOT. The roots must be **siblings**, not nested: `stable`
# scans its whole root, so an `alpha` pool living under `pool/` would show up in
# `dists/stable/.../Packages` as `Filename: pool/alpha/...`.
#
# `stable` keeps the historical `pool/` path because the published repo is
# already laid out that way — moving it would break `stable` for the window
# between the delete and the re-upload.
if [ "$SUITE" = stable ]; then
    POOL_ROOT="pool"
else
    POOL_ROOT="pool-${SUITE}"
fi

cleanup() {
    rm -rf "$REPO_DIR"
}
trap cleanup EXIT

# Import GPG key if provided
if [ -n "$GPG_PRIVATE_KEY" ]; then
    echo "$GPG_PRIVATE_KEY" | gpg --batch --import 2>/dev/null
fi

# Configure s3cmd
if [ -n "$S3_ACCESS_KEY" ] && [ -n "$S3_SECRET_KEY" ]; then
    S3CMD_CONFIG="$(mktemp)"
    cat > "$S3CMD_CONFIG" <<EOF
[default]
access_key = ${S3_ACCESS_KEY}
secret_key = ${S3_SECRET_KEY}
host_base = $(echo "$ENDPOINT" | sed 's|https://||')
host_bucket = %(bucket)s.$(echo "$ENDPOINT" | sed 's|https://||')
use_https = True
EOF
    s3() {
        s3cmd -c "$S3CMD_CONFIG" "$@"
    }
else
    # Fall back to default ~/.s3cfg
    S3CMD_CONFIG=""
    s3() {
        s3cmd "$@"
    }
fi

# Sync down only what this suite owns — its pool and its dists/ directory.
# Everything below (the eviction, the Packages scan, the --delete-removed upload)
# then operates on a tree that physically cannot contain another suite's files,
# so publishing `alpha` from CI can neither index nor delete anything in
# `stable`. Trust is already separated by the per-suite Release signature; this
# separates write access too.
#
# A failed or partial download must NOT be swallowed: the upload below prunes
# whatever is missing from this mirror, so proceeding on half a mirror would
# delete the other half of the suite. An empty prefix is the one benign case
# (first publish), and it is detected explicitly rather than inferred from an
# error.
sync_down() {
    local prefix="$1" dest="$2" listing
    mkdir -p "$dest"
    listing=$(s3 ls --recursive "s3://${BUCKET}/${prefix}")
    if [ -z "$listing" ]; then
        echo "  s3://${BUCKET}/${prefix} is empty — first publish into it."
        return 0
    fi
    s3 sync --no-mime-magic "s3://${BUCKET}/${prefix}" "$dest"
}

echo "Syncing the ${SUITE} suite from s3://${BUCKET}/ ..."
sync_down "${POOL_ROOT}/" "$REPO_DIR/${POOL_ROOT}/"
sync_down "dists/${SUITE}/" "$REPO_DIR/dists/${SUITE}/"

# Collect all .deb files from arguments
DEB_FILES=()
for arg in "$@"; do
    if [ -d "$arg" ]; then
        while IFS= read -r -d '' f; do
            DEB_FILES+=("$f")
        done < <(find "$arg" -name '*.deb' -print0)
    elif [ -f "$arg" ]; then
        DEB_FILES+=("$arg")
    else
        echo "Warning: $arg is not a file or directory, skipping" >&2
    fi
done

if [ ${#DEB_FILES[@]} -eq 0 ]; then
    echo "No .deb files found" >&2
    exit 1
fi

# Copy each deb to the pool, removing old versions of the same package+arch
for deb in "${DEB_FILES[@]}"; do
    PKG_NAME="$(dpkg-deb --field "$deb" Package)"
    PKG_ARCH="$(dpkg-deb --field "$deb" Architecture)"
    POOL_DIR="$REPO_DIR/${POOL_ROOT}/${COMPONENT}/${PKG_NAME:0:1}/${PKG_NAME}"
    mkdir -p "$POOL_DIR"
    # Remove old versions for the same architecture
    for old in "$POOL_DIR"/${PKG_NAME}_*_${PKG_ARCH}.deb; do
        [ -f "$old" ] && rm -v "$old"
    done
    cp "$deb" "$POOL_DIR/"
    dpkg-name -o "$POOL_DIR/$(basename "$deb")" 2>/dev/null || true
    echo "Added: $(basename "$deb") -> ${POOL_ROOT}/${COMPONENT}/${PKG_NAME:0:1}/${PKG_NAME}/"
done

# Generate Packages indices for each architecture
for arch in amd64 arm64 riscv64; do
    BINARY_DIR="$REPO_DIR/dists/${SUITE}/${COMPONENT}/binary-${arch}"
    mkdir -p "$BINARY_DIR"
    (
        cd "$REPO_DIR"
        dpkg-scanpackages --multiversion --arch "$arch" "$POOL_ROOT/" > "$BINARY_DIR/Packages"
        gzip -k -f "$BINARY_DIR/Packages"
    )
    echo "Generated Packages index for ${arch}"
done

# Generate Release file
(
    cd "$REPO_DIR/dists/${SUITE}"
    apt-ftparchive release \
        -o "APT::FTPArchive::Release::Origin=Start9" \
        -o "APT::FTPArchive::Release::Label=Start9" \
        -o "APT::FTPArchive::Release::Suite=${SUITE}" \
        -o "APT::FTPArchive::Release::Codename=${SUITE}" \
        -o "APT::FTPArchive::Release::Architectures=amd64 arm64 riscv64" \
        -o "APT::FTPArchive::Release::Components=${COMPONENT}" \
        . > Release
)
echo "Generated Release file"

# Sign if GPG key is available
if [ -n "$GPG_KEY_ID" ]; then
    (
        cd "$REPO_DIR/dists/${SUITE}"
        gpg --default-key "$GPG_KEY_ID" --batch --yes --detach-sign -o Release.gpg Release
        gpg --default-key "$GPG_KEY_ID" --batch --yes --clearsign -o InRelease Release
    )
    echo "Signed Release file with key ${GPG_KEY_ID}"
else
    echo "Warning: GPG_KEY_ID not set, Release file is unsigned" >&2
fi

# Upload to S3, one prefix at a time so --delete-removed can only ever prune
# within this suite. --delete-after keeps that pruning from running before the
# new pool files land, so the Packages index published above never points at an
# object that has been deleted but not yet re-uploaded. The pool goes first for
# the same reason: the index must never be newer than what it indexes.
echo "Uploading the ${SUITE} suite to s3://${BUCKET}/ ..."
s3 sync --acl-public --no-mime-magic --delete-removed --delete-after \
    "$REPO_DIR/${POOL_ROOT}/" "s3://${BUCKET}/${POOL_ROOT}/"
s3 sync --acl-public --no-mime-magic --delete-removed --delete-after \
    "$REPO_DIR/dists/${SUITE}/" "s3://${BUCKET}/dists/${SUITE}/"

[ -n "$S3CMD_CONFIG" ] && rm -f "$S3CMD_CONFIG"
echo "Done."
