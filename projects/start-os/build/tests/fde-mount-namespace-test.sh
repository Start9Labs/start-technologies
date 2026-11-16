#!/bin/bash
set -euo pipefail
binary=$(realpath "${1:?start-core test executable}")
root=$(mktemp -d /var/lib/fde-namespace-test-XXXXXX)
unit="fde-namespace-probe-${root##*-}"
cleanup() {
    systemctl stop "$unit" 2>/dev/null || true
    systemctl reset-failed "$unit" 2>/dev/null || true
    if [ -f "$root/staging-path" ]; then
        IFS= read -r staged < "$root/staging-path" || true
        if [ -n "${staged:-}" ]; then umount -l -R "$staged" 2>/dev/null || true; fi
    fi
    for path in rootfs lower source; do umount -l -R "$root/$path" 2>/dev/null || true; done
    rm -rf "$root"
}
trap cleanup EXIT
systemd-run --unit="$unit" --property=ProtectSystem=strict --property=PrivateTmp=yes /usr/bin/sleep infinity
pid=$(systemctl show --property=MainPID --value "$unit")
test "$pid" -gt 0
export FDE_NAMESPACE_PEER_PID="$pid" FDE_NAMESPACE_TEST_ROOT="$root"
"$binary" --ignored --exact disk::mount::util::namespace_tests::native_slave_namespace_releases_rootfs_and_nested_staging_copies --nocapture
