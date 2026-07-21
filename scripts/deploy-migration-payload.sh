#!/usr/bin/env bash
#
# deploy-migration-payload.sh — publish a 0.3.5.1 -> 0.4.0 migration OTA
# payload (built by `make start-os-migration-squashfs`, assembled by
# projects/start-os/build/assemble-migration-payload.sh) to a legacy (Haskell)
# 0.3.5.1 registry over ssh.
#
# Usage: ./scripts/deploy-migration-payload.sh [options] <ssh-host> <payload.migration.squashfs>...
#
# How 0.3.5.1's frozen updater pulls an update, and therefore what this script
# sets up on the registry:
#   1. GET /eos/v0/latest?os.version=..&os.arch=<platform> — the Haskell app
#      serves the max os_version DB row for the box's *exact* arch (the
#      -nonfree variants are distinct arches; os.compat is ignored).
#   2. rsync <host>::<version>/<platform>/ (daemon protocol, port 873) into
#      the box's `next` subvolume.
# So per payload we: upload the squashfs, loop-mount it (fstab-persisted — the
# old registries' ad-hoc mounts died on reboot) at /srv/rsync/<version>/<platform>,
# expose a [<version>] rsyncd module, and verify an external pull with the
# updater's exact flags. Only then, as the go-live switch, insert the
# os_version row and verify /eos/v0/latest advertises it.
#
# The advertised version must be plain emver (major.minor.patch[.revision]):
# 0.3.5.1's parser has no prerelease syntax, so 0.4.0-beta.N is advertised as
# 0.4.0 (the default; override with --version). Boxes on other arches are
# untouched until a payload for their arch is deployed.
#
# Assumes the standard legacy registry droplet: root ssh, the Haskell app as
# registry.service (its Environment supplies RESOURCES_PATH / PG_DATABASE /
# REGISTRY_HOSTNAME), postgres peer auth, and Debian/Ubuntu's rsync.service.

set -euo pipefail

usage() {
    >&2 cat <<EOF
usage: $0 [options] <ssh-host> <payload.migration.squashfs>...

options:
  --version X.Y.Z[.R]  version advertised to 0.3.5.1 boxes (default: payload
                       version with any prerelease suffix stripped)
  --headline TEXT      os_version headline (default: "StartOS <version>")
  --notes TEXT         os_version release notes shown in the update prompt
  --hostname HOST      registry public hostname (default: REGISTRY_HOSTNAME
                       from registry.service on the target)
  --resources DIR      resources dir on the target (default: RESOURCES_PATH
                       from registry.service)
  --skip-publish       stage + verify everything but skip the os_version DB
                       insert (the go-live switch); rerun without this to
                       publish
EOF
    exit 1
}

ADVERTISED=
EXPLICIT_VERSION=
HEADLINE=
NOTES=
HOSTNAME_OVERRIDE=
RESOURCES_OVERRIDE=
SKIP_PUBLISH=
TARGET=
PAYLOADS=()

while [ $# -gt 0 ]; do
    case "$1" in
        --version)      ADVERTISED="$2"; EXPLICIT_VERSION=1; shift 2 ;;
        --headline)     HEADLINE="$2"; shift 2 ;;
        --notes)        NOTES="$2"; shift 2 ;;
        --hostname)     HOSTNAME_OVERRIDE="$2"; shift 2 ;;
        --resources)    RESOURCES_OVERRIDE="$2"; shift 2 ;;
        --skip-publish) SKIP_PUBLISH=1; shift ;;
        -*)             usage ;;
        *)
            if [ -z "$TARGET" ]; then TARGET="$1"; else PAYLOADS+=("$1"); fi
            shift ;;
    esac
done
[ -n "$TARGET" ] && [ "${#PAYLOADS[@]}" -gt 0 ] || usage

# startos-<version>-<hash>[~env]_<platform>.migration.squashfs
PLATFORMS=()
for payload in "${PAYLOADS[@]}"; do
    [ -f "$payload" ] || { >&2 echo "no such file: $payload"; exit 1; }
    base="$(basename "$payload")"
    stem="${base%.migration.squashfs}"
    [ "$stem" != "$base" ] || { >&2 echo "$base: not a .migration.squashfs"; exit 1; }
    # suffix-match against the known set: platforms themselves contain '_'
    platform=
    for p in x86_64-nonfree aarch64-nonfree x86_64 aarch64 raspberrypi; do
        case "$stem" in *_"$p") platform="$p"; break ;; esac
    done
    case "$platform" in
        raspberrypi) >&2 echo "raspberrypi has no in-place migration — reflash required (#3443)"; exit 1 ;;
        "") >&2 echo "$base: unrecognized platform"; exit 1 ;;
    esac
    PLATFORMS+=("$platform")
    verhash="${stem#startos-}"; verhash="${verhash%_"$platform"}"
    version="${verhash%-*}" # drop the trailing -<hash>[~env] segment
    derived="${version%%-*}"
    if [ -n "$EXPLICIT_VERSION" ]; then
        : # --version wins for all payloads
    elif [ -z "$ADVERTISED" ]; then
        ADVERTISED="$derived"
    elif [ "$derived" != "$ADVERTISED" ]; then
        >&2 echo "$base: derives version $derived but earlier payloads derive $ADVERTISED; pass --version to force one"
        exit 1
    fi
done
[[ "$ADVERTISED" =~ ^[0-9]+\.[0-9]+\.[0-9]+(\.[0-9]+)?$ ]] \
    || { >&2 echo "advertised version '$ADVERTISED' is not plain emver (0.3.5.1 cannot parse prerelease tags); pass --version"; exit 1; }
HEADLINE="${HEADLINE:-StartOS $ADVERTISED}"
NOTES="${NOTES:-Major StartOS release. Updates your server in place from 0.3.5.1: the OS is replaced and your services and data are preserved. Downloads ~3 GB and reboots when complete.}"

echo "== target: $TARGET   advertised version: $ADVERTISED   platforms: ${PLATFORMS[*]}"

[ "$(ssh "$TARGET" id -u)" = 0 ] || { >&2 echo "$TARGET: root ssh required"; exit 1; }

# Values that can contain spaces (MARKETPLACE_NAME) would split wrong here, but
# the three we read are a path, a db name, and a hostname.
REG_ENV="$(ssh "$TARGET" systemctl show registry.service -p Environment --value | tr ' ' '\n' | tr -d "'")"
reg_env() { echo "$REG_ENV" | grep "^$1=" | head -n1 | cut -d= -f2-; }
RESOURCES="${RESOURCES_OVERRIDE:-$(reg_env RESOURCES_PATH)}"
PG_DB="$(reg_env PG_DATABASE)"
REG_HOST="${HOSTNAME_OVERRIDE:-$(reg_env REGISTRY_HOSTNAME)}"
[ -n "$RESOURCES" ] && [ -n "$PG_DB" ] && [ -n "$REG_HOST" ] \
    || { >&2 echo "$TARGET: could not read RESOURCES_PATH/PG_DATABASE/REGISTRY_HOSTNAME from registry.service — is this a legacy registry? (--resources/--hostname to override)"; exit 1; }
echo "== registry: $REG_HOST   resources: $RESOURCES   db: $PG_DB"

for i in "${!PAYLOADS[@]}"; do
    payload="${PAYLOADS[$i]}" platform="${PLATFORMS[$i]}"
    base="$(basename "$payload")"

    size="$(wc -c < "$payload")"
    avail="$(ssh "$TARGET" df --output=avail -B1 "$RESOURCES" | tail -1 | tr -d ' ')"
    [ "$avail" -gt "$size" ] || { >&2 echo "$TARGET: $RESOURCES has $avail bytes free, payload is $size"; exit 1; }

    echo "== uploading $base"
    ssh "$TARGET" mkdir -p "$RESOURCES/eos/$ADVERTISED"
    rsync -a --partial --info=progress2 "$payload" "$TARGET:$RESOURCES/eos/$ADVERTISED/"
    local_sum="$(sha256sum "$payload" | cut -d' ' -f1)"
    remote_sum="$(ssh "$TARGET" sha256sum "$RESOURCES/eos/$ADVERTISED/$base" | cut -d' ' -f1)"
    [ "$local_sum" = "$remote_sum" ] || { >&2 echo "sha256 mismatch after upload: $local_sum != $remote_sum"; exit 1; }

    echo "== mounting at /srv/rsync/$ADVERTISED/$platform"
    # printf %q: ssh joins args into one remote shell string, so quote for it
    ssh "$TARGET" "bash -es -- $(printf '%q ' "$ADVERTISED" "$platform" "$RESOURCES/eos/$ADVERTISED/$base")" <<'REMOTE'
set -euo pipefail
VERSION="$1" PLATFORM="$2" SQFS="$3"
MNT="/srv/rsync/$VERSION/$PLATFORM"
mkdir -p "$MNT"
# a replaced payload keeps the old inode under the live loop mount; remount
if mountpoint -q "$MNT"; then umount "$MNT"; fi
awk -v mnt="$MNT" '$2 != mnt' /etc/fstab > /etc/fstab.new && mv /etc/fstab.new /etc/fstab
echo "$SQFS $MNT squashfs loop,ro,nofail 0 0" >> /etc/fstab
systemctl daemon-reload
mount "$MNT"
test -f "$MNT/.startos-migration" && test -f "$MNT/usr/lib/startos/migration-boot" \
    || { >&2 echo "$SQFS does not look like a migration payload (missing sentinel/migration-boot)"; exit 1; }
REMOTE
done

echo "== exposing rsyncd module [$ADVERTISED]"
# rsyncd rereads its conf per connection, so no restart on module changes
ssh "$TARGET" "bash -es -- $(printf '%q ' "$ADVERTISED")" <<'REMOTE'
set -euo pipefail
VERSION="$1"
grep -qxF "[$VERSION]" /etc/rsyncd.conf \
    || printf '\n[%s]\npath = /srv/rsync/%s\nread only = yes\n' "$VERSION" "$VERSION" >> /etc/rsyncd.conf
systemctl enable --now rsync
REMOTE

for platform in "${PLATFORMS[@]}"; do
    echo "== verifying external pull of ${REG_HOST}::$ADVERTISED/$platform/"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    # exactly the frozen updater's flags (core/helpers/src/rsync.rs @ v0.3.5.1)
    rsync --delete --force -actAXH --no-inc-recursive \
        "${REG_HOST}::$ADVERTISED/$platform/usr/lib/startos/" "$tmp/"
    test -f "$tmp/migration-boot" || { >&2 echo "pull verification failed: no migration-boot"; exit 1; }
    rm -rf "$tmp"
done

if [ -n "$SKIP_PUBLISH" ]; then
    echo "== staged and verified; skipping os_version insert (--skip-publish). Rerun without it to go live."
    exit 0
fi

for platform in "${PLATFORMS[@]}"; do
    echo "== publishing os_version $ADVERTISED/$platform (go-live)"
    ssh "$TARGET" "bash -es -- $(printf '%q ' "$PG_DB" "$ADVERTISED" "$platform" "$HEADLINE" "$NOTES")" <<'REMOTE'
set -euo pipefail
DB="$1" VERSION="$2" PLATFORM="$3" HEADLINE="${4//\'/\'\'}" NOTES="${5//\'/\'\'}"
cd /tmp # postgres user can't read root's cwd
if [ "$(sudo -u postgres psql -tA -d "$DB" -c "select count(*) from os_version where number='$VERSION' and arch='$PLATFORM'")" != 0 ]; then
    echo "os_version $VERSION/$PLATFORM already present — skipping insert"
    exit 0
fi
sudo -u postgres psql -d "$DB" -c "insert into os_version (id, created_at, updated_at, number, headline, release_notes, arch) \
    values (nextval('os_version_id_seq'), now(), now(), '$VERSION', '$HEADLINE', '$NOTES', '$PLATFORM')"
REMOTE

    latest="$(curl -fsSk "https://$REG_HOST/eos/v0/latest?os.version=0.3.5.1&os.arch=$platform")"
    echo "$latest" | grep -qF "\"version\":\"$ADVERTISED\"" \
        || { >&2 echo "/eos/v0/latest for $platform did not advertise $ADVERTISED: $latest"; exit 1; }
    echo "== $platform live: $latest"
done

echo "== done. 0.3.5.1 boxes on ${PLATFORMS[*]} pointed at $REG_HOST will now be offered $ADVERTISED."
