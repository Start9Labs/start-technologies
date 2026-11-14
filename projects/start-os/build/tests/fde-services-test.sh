#!/bin/bash
set -euo pipefail
if [[ $# != 1 || $EUID != 0 ]]; then
    printf 'Usage (root, exclusive managed StartOS guest only): %s <start-core-test-executable>\n' "$0" >&2
    exit 1
fi
executable=$(realpath -e "$1")
export PATH=/usr/sbin:/usr/bin:/sbin:/bin
cleanup() {
    for unit in startos-shutdown.service startos-restart.service; do
        rm -f "/run/systemd/system/$unit.d/99-fde-native-test.conf"
    done
    systemctl daemon-reload
}
trap cleanup EXIT
# Keeps the guest's reboot hooks from firing during test isolation.
for unit in startos-shutdown.service startos-restart.service; do
    mkdir -p "/run/systemd/system/$unit.d"
    printf '[Unit]\nIgnoreOnIsolate=yes\n' > "/run/systemd/system/$unit.d/99-fde-native-test.conf"
done
systemctl daemon-reload
systemctl get-default
systemctl is-active ssh.service
status=0
systemd-run --unit=fde-services-native-test --wait --property=IgnoreOnIsolate=yes --property=RuntimeMaxSec=240 /usr/bin/env PATH="$PATH" "$executable" --exact fde::boot_services::tests::real_systemd_profile_isolates_ssh_then_restores_normal_target --ignored --nocapture || status=$?
journalctl -u fde-services-native-test.service --no-pager -o cat
systemctl get-default
systemctl is-active ssh.service
exit "$status"
