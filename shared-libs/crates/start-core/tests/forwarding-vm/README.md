# Forwarding VM regression fixture

This runs the ignored `tunnel::api::forwarding_vm_tests::forwarding_handlers_vm`
unit test against real nftables and WireGuard inside an intentionally disposable
Linux VM. The test lives in `src/tunnel/api_forwarding_vm_tests.rs`.

**Use a disposable VM snapshot only.** The test initializes a tunnel context,
changes networking, and writes StartOS nftables tables. Restore the snapshot
afterwards, including after a failed or interrupted run. Never use a production
server, the build host, or a container sharing the host network namespace.

## Build and run

From the repository root, use the normal make runner and its existing Cargo and
sccache mounts:

```sh
make start-core-test PROFILE=release
```

The ignored VM test is compiled but skipped by that command. Use the exact
`target/release/deps/start_core-<hash>` executable printed by Cargo's `Running
unittests` line. Build for the VM's CPU architecture and compatible Linux runtime.
The runner requires an explicit executable path; it checks that the binary
contains the ignored test before installing scripts.

The VM needs Bash, Python 3, real `/usr/sbin/nft`, WireGuard kernel support and
`wg`, `ip`, `conntrack`, systemd, `flock`, `pgrep`, and passwordless `sudo` for the
SSH user. Stop `startd` and `start-tunneld` yourself on that disposable VM before
running. The runner refuses active daemons and never stops them for you.

Explicitly opt in **on the disposable VM**:

```sh
sudo touch /run/startos-forwarding-vm-test
```

Then run from the repository root, substituting your SSH destination and the
executable path from the build output:

```sh
KEEP_VM_ARTIFACTS=1 bash shared-libs/crates/start-core/tests/forwarding-vm/run-vm.sh \
  user@disposable-vm 'target/release/deps/start_core-<hash>'
```

Use an SSH config alias for custom ports or keys. The runner stages the supplied
binary, the wrapper, and the checkout's real `build/lib/scripts/forward-port`,
`forward-port6`, and `forward-port-nft`. It runs all networking commands remotely.
The wrapper's `PATH` applies only to the test process and its children. Original
VM scripts are backed up and restored on normal exit, test failure, or a handled
signal. A VM-wide lock serializes fixture runs.

Every run gets a unique `/tmp/startos-forwarding-vm.*` directory with its own
failure/barrier controls, database, `test.log`, `transactions.jsonl`, and
`race-rules.txt`. Failures retain artifacts; `KEEP_VM_ARTIFACTS=1` also retains
successful runs. With the default `0`, successful runs remove their staging
directory. Retrieve retained artifacts with `sudo` on the VM as needed. The exit
status reports the test or script-restoration failure. A hard kill or lost SSH
connection can interrupt restoration: restore the disposable snapshot before
another run. The runner's file cleanup is not a rollback of VM networking.

## Assertions and fault injection

The test exercises the real API handlers and checks:

- A three-port IPv4 range retains every translated offset through repeated
  disable/enable calls.
- An injected IPv6 deletion failure retains the database entry, lease expiry,
  and exact nft rules; retrying the same removal clears all three.
- A concurrent IPv4 removal waits for an admitted addition's nft transaction,
  then leaves the database, active-owner map, and nft rules clear.
- Final forwarding shutdown completes and fixture rules are absent.

`nft` logs each invocation and delegates to `/usr/sbin/nft`. While `fail-delete`
exists, transactions containing `delete rule` fail before reaching nft. A
`barrier` containing a matching fragment pauses an `add rule` transaction until
`release` exists, with a 15-second timeout and an `entered` marker. These are
injected failures and scheduling barriers, not simulated successful nft changes.
The test inspects kernel rules through the absolute real nft path.

For build-free fixture checks:

```sh
bash -n shared-libs/crates/start-core/tests/forwarding-vm/{run-vm,remote}.sh
shellcheck shared-libs/crates/start-core/tests/forwarding-vm/{run-vm,remote}.sh
```
