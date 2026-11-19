#!/bin/bash
set -euo pipefail

if [ "${STARTOS_TEST_MANAGED_GUEST:-}" != 1 ] || [ "$(< /proc/1/comm)" != systemd ] || [ "$EUID" -ne 0 ]; then
    echo 'Requires an exclusive managed StartOS guest; never run on the build host.' >&2
    exit 1
fi
if [ "$#" -lt 3 ]; then
    echo 'Usage: fde-capture-test.sh <test-executable> <built-startbox> <genuine-source-image> [test-name...]' >&2
    exit 1
fi
binary=$(realpath -e -- "$1")
startbox=$(realpath -e -- "$2")
image=$(realpath -e -- "$3")
shift 3
root=$(mktemp -d /var/lib/startos-fde-capture.XXXXXXXX)
unit=
fixture=
cleanup() {
    local result=$?
    trap - EXIT
    if [ -n "$unit" ]; then
        systemctl kill --kill-whom=main --signal=KILL "$unit" 2>/dev/null || true
    fi
    if [ -n "$fixture" ] && mountpoint -q "$fixture/volume"; then
        fsfreeze --unfreeze "$fixture/volume" 2>/dev/null || true
    fi
    if [ -n "$fixture" ] && [ -p "$fixture/release-gate" ]; then
        exec {release}<> "$fixture/release-gate"
        printf 'restore\n' >&"$release"
    fi
    if [ -n "$unit" ]; then
        systemctl stop "$unit" 2>/dev/null || true
    fi
    if [ -n "$fixture" ]; then
        for path in volume/alias working ram base volume; do
            if mountpoint -q "$fixture/$path"; then
                umount "$fixture/$path" || result=1
            fi
        done
        for path in volume/alias working ram base volume; do
            if mountpoint -q "$fixture/$path"; then
                echo "Fixture mount remains: $fixture/$path" >&2
                exit 1
            fi
        done
    fi
    rm -rf -- "$root"
    exit "$result"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
cp -- "$startbox" "$root/startbox"
chmod 755 "$root/startbox"
if [ "$#" = 0 ]; then
    set -- \
        fde::capture::tests::lifecycle::native_remount_preserves_security_flags_and_raw_alias_exclusion \
        fde::capture::tests::lifecycle::native_cleanup_retry_holds_mutation_admission_and_keeps_cached_reads_live \
        fde::capture::tests::native_capture_freezes_sources_keeps_db_reads_live_and_projects_after_thaw \
        fde::capture::tests::native_profile_capture_failure_thaws_before_returning_without_projection \
        fde::capture::tests::native_capture_helper_death_restores_owned_sources_and_preserves_readonly_intent \
        fde::capture::tests::native_capture_stop_thaws_before_waiting_for_a_blocked_owned_writer \
        fde::capture::tests::native_capture_requester_death_leaves_cleanup_owned_by_pid1
fi
inventory=$("$binary" --list)
for name in "$@"; do
    if ! grep -Fxq -- "$name: test" <<< "$inventory"; then
        echo "Unknown exact test: $name" >&2
        exit 1
    fi
done
for name in "$@"; do
    fixture="$root/$(< /proc/sys/kernel/random/uuid)"
    mkdir -m 700 "$fixture"
    unit="startos-fde-capture-$(< /proc/sys/kernel/random/uuid)"
    STARTOS_TEST_MANAGED_GUEST=1 FDE_CAPTURE_TEST_ROOT="$fixture" FDE_CAPTURE_EXECUTABLE="$root/startbox" \
        FDE_CAPTURE_UNIT="$unit" FDE_SOURCE_IMAGE="$image" \
        timeout --kill-after=20s 240s "$binary" "$name" --exact --ignored --nocapture
    systemctl stop "$unit" 2>/dev/null || true
    unit=
    fixture=
done
