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
                       (markdown; default: the pre-update steps of the 0.4.0
                       migration guide)
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
# 0.3.5.1's OS-update modal renders these notes as markdown (marked + DOMPurify;
# GitHub-style `> [!WARNING]` admonitions are NOT supported) directly above its
# "Begin Update" button. The default carries the pre-update portion of the 0.4.0
# migration guide (projects/start-os/docs/src/update-040.md), adapted to the OTA
# path (no USB steps) — keep the two in sync.
if [ -z "$NOTES" ]; then
    NOTES="$(cat <<'EOF'
StartOS 0.4.0 is a completely new operating system. This update replaces StartOS in place: it downloads ~3 GB, reboots your server, and migrates your services and data to the new format. It requires StartOS 0.3.5.1.

**This is a major migration, not a routine update.** Complete every preparation step below before tapping "Begin Update". Skipping the service-update or backup steps can result in **permanent data loss**.

**1. Review services that need special handling**

- **Embassy Pages** — retired and replaced by **Start9 Pages**. Embassy Pages will survive the update but will no longer receive updates. Uninstall it, then after the update install Start9 Pages from the marketplace and re-add your content.
- **Ghost** — completely redesigned for 0.4.0 and incompatible with the old version. Before updating, open your Ghost admin UI and use Ghost's built-in **Export** tool to download your content. After the update, install the new Ghost and use its **Import** tool to restore your content.
- **Synapse** — the old Synapse was Tor-only; the new Synapse is clearnet-only. These are different services now, with no migration path.
- **Jam** — Jam's backend (JoinMarket) is being replaced, and Jam is unavailable on 0.4.0 until its new backend matures. Back up your seed, move out any spendable funds (fidelity-bond funds stay locked until expiry), and uninstall Jam before updating.

**2. Prepare for new service addresses**

On 0.4.0, each service no longer has its own `.local` address (e.g. `longexamplepublickey.local`); services are reached on unique ports of your server's main `.local` address (e.g. `adjective-noun.local:4545`). Your old per-service `.local` addresses will no longer exist after the update. If you use a password manager, give your saved passwords clear names now — not just the old `.local` URLs — so you can identify them later and save the new URLs.

**3. Update ALL services**

Update every installed service to its latest version, starting at the base of the dependency tree and working upward — Bitcoin before LND, LND before RTL. Bitcoin may safely remain at 28.x or 29.x, but you MUST update to the latest **minor** version of your selected major version. **This step is required:** services that are not on their latest version may fail to migrate, potentially requiring a rollback to 0.3.5.1 or losing data entirely.

**4. (Recommended) Add an SSH key**

If you haven't already, add an SSH key to your server. If something goes wrong during the migration, SSH access makes it much easier to debug.

**5. Uninstall services you don't use**

Every installed service must be migrated, and each adds to the total migration time. Uninstalling unused services now is much faster than migrating them; you can reinstall fresh on 0.4.0.

**6. Stop all services**

Stop every remaining service and wait for each to fully stop, so no new data is written before your backup.

**7. Create a full system backup**

With all services stopped, create a full system backup covering every service. **Do not skip this step.** Backups made on 0.3.5.1 cannot be restored onto 0.4.0 — this backup is your safety net for returning to 0.3.5.1 if the migration fails.

**What to expect**

After you begin, the update downloads (~3 GB), your server reboots, and the migration runs. It can take **hours**, depending on how much data you have — be patient, and do not power off or unplug your server. When it completes: sign in, update **all** of your services from the marketplace before doing anything else (the 0.4.0 versions are repackaged for the new system even when the app version looks the same), start them, and create a fresh backup.

Full guide: https://docs.start9.com/start-os/update-040.html
EOF
)"
fi

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
