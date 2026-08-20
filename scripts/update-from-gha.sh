#!/bin/bash
#
# Update a StartOS server from a GitHub Actions build, without building locally.
#
# This is `make start-os-update-squashfs` with the squashfs fetched from CI
# instead of results/: prune, upload to /media/startos/images/next.rootfs, and
# run the checksummed upgrade. The server reboots into the new image.
#
# Usage: scripts/update-from-gha.sh [options] <remote>
#
#   <remote>            ssh destination of the server, e.g. root@adjective-noun.local
#
# Options:
#   --run <id|url>      GitHub Actions run to take the image from. Accepts a run
#                       id or a full run URL.
#   --branch <name>     Use the latest successful startos-iso run on this branch
#                       instead of --run. Default: master.
#   --platform <name>   Image platform (x86_64, aarch64, raspberrypi, ...).
#                       Default: whatever the server reports.
#   --keep <dir>        Download into <dir> and leave it in place. Default: a
#                       temp dir, removed on exit.
#   --yes               Skip the confirmation prompt.
#   -h, --help          Show this help.
#
# Environment variables:
#   REPO      GitHub repo to pull runs from (default: Start9Labs/start-technologies)
#   SSHPASS   ssh password, matching the Makefile's REMOTE/SSHPASS convention

set -euo pipefail

REPO="${REPO:-Start9Labs/start-technologies}"
WORKFLOW="startos-iso.yaml"
REMOTE=""
RUN_ID=""
BRANCH=""
PLATFORM=""
KEEP_DIR=""
ASSUME_YES=0

# Print the header comment block (from line 3 to the first non-comment line).
usage() {
    awk 'NR > 2 { if (!/^#/) exit; sub(/^# ?/, ""); print }' "$0"
    exit "${1:-0}"
}

while [ $# -gt 0 ]; do
    case "$1" in
        --run) RUN_ID="$2"; shift 2 ;;
        --branch) BRANCH="$2"; shift 2 ;;
        --platform) PLATFORM="$2"; shift 2 ;;
        --keep) KEEP_DIR="$2"; shift 2 ;;
        --yes | -y) ASSUME_YES=1; shift ;;
        -h | --help) usage 0 ;;
        -*) >&2 echo "Unknown option: $1"; usage 2 ;;
        *)
            if [ -n "$REMOTE" ]; then
                >&2 echo "Unexpected argument: $1"
                usage 2
            fi
            REMOTE="$1"; shift
            ;;
    esac
done

if [ -z "$REMOTE" ]; then
    >&2 echo "Must specify the server to update (e.g. root@adjective-noun.local)"
    usage 2
fi

for bin in gh b3sum; do
    if ! command -v "$bin" > /dev/null; then
        >&2 echo "Missing required tool: $bin"
        exit 1
    fi
done

if [ -n "${SSHPASS:-}" ]; then
    if ! command -v sshpass > /dev/null; then
        >&2 echo "SSHPASS is set but sshpass is not installed"
        exit 1
    fi
    remote_sh() { sshpass -p "$SSHPASS" ssh "$REMOTE" "$@"; }
else
    remote_sh() { ssh "$REMOTE" "$@"; }
fi

# --- Resolve the run -------------------------------------------------------

if [ -n "$RUN_ID" ]; then
    # Accept a run URL as well as a bare id, matching manage-release.sh.
    if [[ "$RUN_ID" =~ /actions/runs/([0-9]+) ]]; then
        RUN_ID="${BASH_REMATCH[1]}"
    fi
else
    BRANCH="${BRANCH:-master}"
    echo "Finding the latest successful $WORKFLOW run on $BRANCH..."
    RUN_ID=$(gh run list -R "$REPO" -w "$WORKFLOW" -b "$BRANCH" -s success \
        -L 1 --json databaseId -q '.[0].databaseId')
    if [ -z "$RUN_ID" ] || [ "$RUN_ID" = "null" ]; then
        >&2 echo "No successful $WORKFLOW run found on $BRANCH"
        exit 1
    fi
fi

read -r RUN_SHA RUN_BRANCH RUN_DATE < <(
    gh run view -R "$REPO" "$RUN_ID" \
        --json headSha,headBranch,createdAt \
        -q '[.headSha, .headBranch, .createdAt] | @tsv'
)

# --- Resolve the platform --------------------------------------------------

if [ -z "$PLATFORM" ]; then
    echo "Reading the platform from $REMOTE..."
    PLATFORM=$(remote_sh 'cat /usr/lib/startos/PLATFORM.txt' | tr -d '[:space:]')
    if [ -z "$PLATFORM" ]; then
        >&2 echo "Could not read /usr/lib/startos/PLATFORM.txt — pass --platform"
        exit 1
    fi
fi

CURRENT_VERSION=$(remote_sh 'cat /usr/lib/startos/VERSION.txt' 2> /dev/null | tr -d '[:space:]' || true)
CURRENT_HASH=$(remote_sh 'cat /usr/lib/startos/GIT_HASH.txt' 2> /dev/null | tr -d '[:space:]' || true)

cat << EOF

  Server:    $REMOTE
  Installed: ${CURRENT_VERSION:-unknown} (${CURRENT_HASH:-unknown})
  Platform:  $PLATFORM
  Run:       $RUN_ID — $RUN_BRANCH @ ${RUN_SHA:0:7}, $RUN_DATE
  Repo:      $REPO

This replaces the server's OS image and reboots it.
EOF

if [ "$ASSUME_YES" -ne 1 ]; then
    read -rp "Continue? [y/N] " reply
    case "$reply" in
        y | Y | yes | YES) ;;
        *) echo "Aborted."; exit 1 ;;
    esac
fi

# --- Fetch the image -------------------------------------------------------

if [ -n "$KEEP_DIR" ]; then
    mkdir -p "$KEEP_DIR"
    DOWNLOAD_DIR="$KEEP_DIR"
else
    DOWNLOAD_DIR="$(mktemp -d)"
    trap 'rm -rf "$DOWNLOAD_DIR"' EXIT
fi

echo "Downloading ${PLATFORM}.squashfs from run $RUN_ID..."
gh run download -R "$REPO" "$RUN_ID" -n "${PLATFORM}.squashfs" -D "$DOWNLOAD_DIR"

SQUASHFS=$(find "$DOWNLOAD_DIR" -name '*.squashfs' -type f | head -n1)
if [ -z "$SQUASHFS" ]; then
    >&2 echo "Run $RUN_ID has no ${PLATFORM}.squashfs artifact (expired, or that platform was not built)"
    exit 1
fi

# The upgrade script compares against the first half of the blake3 hash, the
# same truncation start-os-update-squashfs passes.
SQFS_SUM=$(b3sum --no-names "$SQUASHFS" | head -c 32)
SQFS_SIZE=$(stat -c %s "$SQUASHFS")
echo "Image: $(basename "$SQUASHFS") (${SQFS_SIZE} bytes, blake3 ${SQFS_SUM})"

# --- Install ---------------------------------------------------------------

echo "Pruning old images to make room..."
remote_sh "sudo /usr/lib/startos/scripts/prune-images $SQFS_SIZE"
remote_sh 'sudo /usr/lib/startos/scripts/prune-boot'

echo "Uploading the image..."
# Streamed through tee rather than scp'd: /media/startos/images is root-owned,
# and this needs no writable staging area on a server that was just pruned for
# exactly this file's worth of space.
remote_sh 'sudo tee /media/startos/images/next.rootfs > /dev/null' < "$SQUASHFS"

echo "Upgrading..."
# The checksum is passed twice on purpose. Servers running an older image carry
# an `upgrade` that only compares when a second positional argument is present,
# so CHECKSUM alone would be silently ignored there; the current script keys off
# CHECKSUM and ignores $2. Passing both verifies on either.
remote_sh "sudo CHECKSUM=$SQFS_SUM /usr/lib/startos/scripts/upgrade /media/startos/images/next.rootfs $SQFS_SUM"

echo
echo "Done — $REMOTE is running ${RUN_SHA:0:7} after it reboots."
