#!/bin/bash
set -euo pipefail

stage=${1:?staging directory required}
keep=${2:?artifact retention flag required}
[[ $stage =~ ^/tmp/startos-forwarding-vm\.[a-zA-Z0-9]+$ && -d $stage ]]
[[ $keep == 0 || $keep == 1 ]]
[[ $EUID == 0 && -f /run/startos-forwarding-vm-test ]] || exit 1
systemd-detect-virt --vm >/dev/null || { echo 'A disposable virtual machine is required' >&2; exit 1; }
exec 9>/run/startos-forwarding-vm-test.lock
flock -n 9 || { echo 'Another forwarding VM test is running' >&2; exit 1; }
for service in startd start-tunneld; do
    state=$(systemctl is-active "$service" || true)
    case "$state" in
        inactive|failed|unknown) ;;
        *) echo "$service must be stopped before running this test ($state)" >&2; exit 1 ;;
    esac
    if pgrep -x "$service" >/dev/null; then
        echo "$service is running outside systemd" >&2
        exit 1
    fi
done
for tool in /usr/sbin/nft wg ip python3 conntrack; do
    command -v "$tool" >/dev/null || { echo "Missing VM dependency: $tool" >&2; exit 1; }
done

test_name=tunnel::api::forwarding_vm_tests::forwarding_handlers_vm
chmod 755 "$stage/tests"
"$stage/tests" --list --ignored > "$stage/test-list.txt"
grep -Fx "$test_name: test" "$stage/test-list.txt" >/dev/null || {
    echo 'The supplied binary does not contain the ignored forwarding VM test' >&2
    exit 1
}

scripts=/usr/lib/startos/scripts
mkdir -p "$stage/backup" "$stage/bin" "$scripts"
for script in forward-port forward-port6 forward-port-nft; do
    if [[ -e $scripts/$script || -L $scripts/$script ]]; then
        cp -a "$scripts/$script" "$stage/backup/$script"
    fi
done
cleanup() {
    status=$?
    trap - EXIT
    set +e
    rm -f "$stage/fail-delete" "$stage/barrier" "$stage/entered"
    touch "$stage/release"
    for script in forward-port forward-port6 forward-port-nft; do
        rm -f "$scripts/$script"
        if [[ -e $stage/backup/$script || -L $stage/backup/$script ]]; then
            cp -a "$stage/backup/$script" "$scripts/$script" || status=1
        fi
    done
    if [[ $status == 0 && $keep == 0 ]]; then
        rm -rf -- "$stage"
    else
        echo "Retained VM artifacts: $stage"
    fi
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
install -m 755 "$stage/nft" "$stage/bin/nft"
for script in forward-port forward-port6 forward-port-nft; do
    install -m 755 "$stage/$script" "$scripts/$script"
done
env PATH="$stage/bin:/usr/sbin:/usr/bin:/sbin:/bin" \
    STARTOS_FORWARDING_VM_TEST="$stage" \
    "$stage/tests" --exact "$test_name" \
    --ignored --nocapture --test-threads=1 2>&1 | tee "$stage/test.log"
