#!/bin/bash

set -euo pipefail

ROOT=$(realpath "$(dirname "${BASH_SOURCE[0]}")/../../../..")
COLLECTOR="$ROOT/projects/start-os/build/lib/scripts/gather-debug-info"
POSTINST="$ROOT/projects/start-os/debian/postinst"
TMP=$(mktemp -d)
trap 'rm -rf -- "$TMP"' EXIT
mkdir -p "$TMP/bin" "$TMP/fixture/dev" "$TMP/fixture/proc"
export COMMAND_LOG="$TMP/commands.log"
REAL_TIMEOUT=$(command -v timeout)
export REAL_TIMEOUT

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

cat > "$TMP/bin/journalctl" <<'EOF2'
#!/bin/bash
printf 'journalctl %s\n' "$*" >> "$COMMAND_LOG"
printf 'fixture journal %s\n' "$*"
EOF2
cat > "$TMP/bin/systemctl" <<'EOF2'
#!/bin/bash
printf 'systemctl %s\n' "$*" >> "$COMMAND_LOG"
printf 'RuntimeWatchdogUSec=1min\nWatchdogDevice=/dev/watchdog0\n'
EOF2
cat > "$TMP/bin/nvme" <<'EOF2'
#!/bin/bash
printf 'nvme %s\n' "$*" >> "$COMMAND_LOG"
if [ "$1" = get-feature ]; then
    printf 'APST Enabled\n'
    for row in {1..32}; do printf 'APST table row %s\n' "$row"; done
fi
EOF2
cat > "$TMP/bin/timeout" <<'EOF2'
#!/bin/bash
[ "$1" = --kill-after=2s ] && [ "$2" = 15s ] || exit 99
if [ "${TIMEOUT_TEST:-0}" = 1 ]; then
    shift 2
    exec "$REAL_TIMEOUT" --kill-after=0.1s 0.1s "$@"
fi
exec "$REAL_TIMEOUT" "$@"
EOF2
chmod +x "$TMP/bin/"*
export PATH="$TMP/bin:$PATH"

awk '/^command_exists\(\)/ { copying=1 } /^# Collecting basic system information/ { copying=0; found=1 } copying { print } END { if (!found) exit 1 }' "$COLLECTOR" > "$TMP/functions.sh"
awk '/^run_command "cat \/proc\/cmdline"/ { copying=1 } /^# Services Info/ { copying=0; found=1 } copying { print } END { if (!found) exit 1 }' "$COLLECTOR" |
    sed "s|/proc/|$TMP/fixture/proc/|g; s|/sys/|$TMP/fixture/sys/|g; s|/dev/|$TMP/fixture/dev/|g; s|/var/lib/|$TMP/fixture/var/lib/|g; s|/media/|$TMP/fixture/media/|g" > "$TMP/diagnostics.sh"
source "$TMP/functions.sh"
OUTPUT_FILE="$TMP/output.txt"

# Controller fixtures stand in for character devices without opening host devices.
test() {
    if [[ $# = 2 && $1 = -c && $2 = "$TMP/fixture/dev/"* ]]; then
        builtin test -f "$2"
    else
        builtin test "$@"
    fi
}

printf 'fixture kernel cmdline\n' > "$TMP/fixture/proc/cmdline"
: > "$OUTPUT_FILE"
source "$TMP/diagnostics.sh"
grep -q 'fixture kernel cmdline' "$OUTPUT_FILE" || fail 'kernel command line missing'
grep -q 'journalctl --no-pager -k -b 0' "$COMMAND_LOG" || fail 'current kernel log missing'
grep -q 'journalctl --no-pager -b -1' "$COMMAND_LOG" || fail 'previous boot journal missing'
grep -q 'RuntimeWatchdogUSec=1min' "$OUTPUT_FILE" || fail 'effective watchdog status missing'
grep -q 'No such file or directory' "$OUTPUT_FILE" || fail 'absent paths not reported'
if grep -q 'nvme get-feature' "$COMMAND_LOG"; then fail 'queried absent controllers'; fi

mkdir -p "$TMP/fixture/sys/fs/pstore" "$TMP/fixture/var/lib/systemd/pstore/efi/nested" \
    "$TMP/fixture/media/startos/data/main/pstore/efi/nested" "$TMP/fixture/sys/class/watchdog/watchdog0"
printf 'flat ramoops evidence\n' > "$TMP/fixture/sys/fs/pstore/dmesg-ramoops-0"
printf 'original EFI evidence\n' > "$TMP/fixture/var/lib/systemd/pstore/efi/nested/dmesg-efi-123"
printf 'persisted EFI evidence\n' > "$TMP/fixture/media/startos/data/main/pstore/efi/nested/console-efi-456"
printf 'persisted console evidence\n' > "$TMP/fixture/media/startos/data/main/pstore/console-ramoops-0"
printf 'fixture watchdog\n' > "$TMP/fixture/sys/class/watchdog/watchdog0/identity"
printf 'active\n' > "$TMP/fixture/sys/class/watchdog/watchdog0/state"
touch "$TMP/fixture/dev/nvme0" "$TMP/fixture/dev/nvme12" "$TMP/fixture/dev/nvme12n1" "$TMP/fixture/dev/nvme12n1p1"
: > "$OUTPUT_FILE"
: > "$COMMAND_LOG"
source "$TMP/diagnostics.sh"
for evidence in 'flat ramoops' 'original EFI' 'persisted EFI' 'persisted console' 'fixture watchdog' 'APST table row 32'; do
    grep -q "$evidence" "$OUTPUT_FILE" || fail "missing $evidence"
done
for controller in nvme0 nvme12; do
    grep -q "nvme get-feature $TMP/fixture/dev/$controller --feature-id=0x0c --data-len=256 --human-readable" "$COMMAND_LOG" || fail "missing $controller APST query"
done
[ "$(grep -c 'nvme get-feature' "$COMMAND_LOG")" = 2 ] || fail 'namespace or partition queried as controller'
grep -q '=== .*console-efi-456 ===' "$OUTPUT_FILE" || fail 'pstore source filename missing'

command_exists() {
    case "$1" in nvme|journalctl|systemctl) return 1 ;; esac
    command -v "$1" >/dev/null 2>&1
}
: > "$OUTPUT_FILE"
source "$TMP/diagnostics.sh"
[ "$(grep -c 'SKIPPED: Command not found' "$OUTPUT_FILE")" = 6 ] || fail 'missing tools not skipped'
source "$TMP/functions.sh"
export TIMEOUT_TEST=1
run_command "sleep 5" "Hanging command fixture"
grep -q 'status 124' "$OUTPUT_FILE" || fail 'timeout not reported'
run_command "printf 'collection continued'" "Following command fixture"
grep -q 'collection continued' "$OUTPUT_FILE" || fail 'collection stopped after timeout'
unset TIMEOUT_TEST
command_exists() { [ "$1" != timeout ] && command -v "$1" >/dev/null 2>&1; }
run_command "sleep 5" "Missing timeout fixture"
grep -q 'SKIPPED: timeout command not found' "$OUTPUT_FILE" || fail 'unbounded command ran without timeout'

awk '/^mkdir -p \/etc\/systemd\/system.conf.d$/ { copying=1 } copying { print } copying && /^EOF$/ { found=1; exit } END { if (!found) exit 1 }' "$POSTINST" |
    sed "s|/etc/|$TMP/etc/|g" > "$TMP/watchdog-install.sh"
sh "$TMP/watchdog-install.sh"
printf '[Manager]\nRuntimeWatchdogSec=60s\n' > "$TMP/expected.conf"
cmp -s "$TMP/expected.conf" "$TMP/etc/systemd/system.conf.d/10-startos-watchdog.conf" || fail 'incorrect manager watchdog drop-in'
sh "$TMP/watchdog-install.sh"
cmp -s "$TMP/expected.conf" "$TMP/etc/systemd/system.conf.d/10-startos-watchdog.conf" || fail 'drop-in installation not idempotent'

printf 'crash-diagnostics tests passed\n'
