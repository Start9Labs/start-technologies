#!/bin/bash
set -euo pipefail

if [[ $# != 2 || $1 == -* || ! $1 =~ ^[a-zA-Z0-9_.@:-]+$ ]]; then
    echo "Usage: $0 DISPOSABLE_VM_SSH_DESTINATION COMPILED_START_CORE_TEST_BINARY" >&2
    exit 2
fi
vm=$1
binary=$2
[[ -f $binary && -x $binary ]] || { echo "Not an executable file: $binary" >&2; exit 2; }
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../../.." && pwd)
keep=${KEEP_VM_ARTIFACTS:-0}
[[ $keep == 0 || $keep == 1 ]] || { echo 'KEEP_VM_ARTIFACTS must be 0 or 1' >&2; exit 2; }

ssh "$vm" 'sudo -n test -f /run/startos-forwarding-vm-test'
stage=$(ssh "$vm" 'mktemp -d /tmp/startos-forwarding-vm.XXXXXXXX')
[[ $stage =~ ^/tmp/startos-forwarding-vm\.[a-zA-Z0-9]+$ ]] || exit 1
printf 'VM artifacts: %s:%s\n' "$vm" "$stage"
scp "$binary" "$vm:$stage/tests"
scp "$here/nft" "$here/remote.sh" "$vm:$stage/"
scp "$repo"/build/lib/scripts/forward-port{,6,-nft} "$vm:$stage/"
printf -v command "sudo -n bash '%s/remote.sh' '%s' '%s'" "$stage" "$stage" "$keep"
# shellcheck disable=SC2029
ssh "$vm" "$command"
