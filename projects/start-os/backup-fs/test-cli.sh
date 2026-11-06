#!/usr/bin/env bash
set -euo pipefail

bin=$(realpath "$1")
root=$(mktemp -d /tmp/backupfs-cli.XXXXXX)
store="$root/store"
live="$root/live"
recovery="$root/recovery"
password='backupfs-cli-synthetic-password'
mounted=
daemon_pid=

cleanup() {
    if [[ -n "$mounted" ]]; then
        fusermount3 -u "$mounted" || true
    fi
    rm -rf "$root"
}
trap cleanup EXIT
mkdir -p "$store" "$live" "$recovery"

mount_view() {
    local destination=$1
    shift
    printf '%s' "$password" | "$bin" mount "$store" "$destination" "$@"
    mounted=$destination
    daemon_pid=$(node - "$bin" "$store" <<'NODE'
const fs = require('node:fs')
const [bin, store] = process.argv.slice(2)
for (const pid of fs.readdirSync('/proc').filter(p => /^\d+$/.test(p))) {
  let args
  try { args = fs.readFileSync(`/proc/${pid}/cmdline`, 'utf8').split('\0') } catch { continue }
  if (args[0] !== bin || !args.includes(store)) continue
  if (args.some(arg => arg.includes('backupfs-cli-synthetic-password'))) throw Error('credential exposed in daemon argv')
  console.log(pid)
  process.exit(0)
}
throw Error('mounted daemon not found')
NODE
)
    if [[ -f /run/mount/utab ]] && grep -qF "$password" /run/mount/utab; then
        echo 'credential exposed in mount table' >&2
        exit 1
    fi
}

unmount_view() {
    fusermount3 -u "$mounted"
    mounted=
    timeout 30 tail --pid="$daemon_pid" -f /dev/null
}

snapshot() {
    printf '%s' "$password" | "$bin" snapshot "$@"
}

inventory() {
    find "$store/segments" -name '*.seg' -type f -exec sha256sum {} + | sort
}

segment_bytes() {
    find "$store/segments" -name '*.seg' -type f -printf '%s\n' | awk '{ total += $1 } END { print total + 0 }'
}

mount_view "$live" --file-size-padding 0.05
node - "$live/data.mp4" "$root/original" "$live/holes.mp4" <<'NODE'
const fs = require('node:fs')
const bytes = Buffer.alloc(5 * 1024 * 1024)
for (let i = 0; i < bytes.length; i++) bytes[i] = (i * 31 + Math.floor(i / 251)) % 256
for (const path of process.argv.slice(2, 4)) fs.writeFileSync(path, bytes)
fs.writeFileSync(process.argv[4], Buffer.alloc(4 * 1024 * 1024, 0x6d))
NODE
chmod 640 "$live/data.mp4"
ln "$live/data.mp4" "$live/hardlink"
ln -s data.mp4 "$live/link"
setfattr -n user.backupfs-test -v preserved "$live/data.mp4"
unmount_view
inventory > "$root/before"
snapshot create "$store" before-update
inventory > "$root/after"
cmp "$root/before" "$root/after"
[[ "$(snapshot list "$store")" = before-update ]]

mount_view "$live" --file-size-padding 0.05
node - "$live/data.mp4" "$live/holes.mp4" <<'NODE'
const fs = require('node:fs')
const fd = fs.openSync(process.argv[2], 'r+')
fs.writeSync(fd, Buffer.from('changed'), 0, 7, 70000)
fs.fsyncSync(fd)
fs.closeSync(fd)
fs.truncateSync(process.argv[3], 1024 * 1024)
NODE
mv "$live/data.mp4" "$live/renamed.mp4"
rm "$live/link"
unmount_view

mount_view "$live" --file-size-padding 0.05
node - "$live/holes.mp4" <<'NODE'
const fs = require('node:fs')
const path = process.argv[2]
fs.truncateSync(path, 4 * 1024 * 1024)
const bytes = fs.readFileSync(path)
if (bytes.subarray(1024 * 1024).some(byte => byte !== 0)) throw Error('truncated bytes reappeared after remount')
const fd = fs.openSync(path, 'r+')
fs.writeSync(fd, Buffer.from('edit'), 0, 4, 2 * 1024 * 1024 + 9)
fs.fsyncSync(fd)
fs.closeSync(fd)
NODE
unmount_view

mount_view "$recovery" --snapshot before-update --file-size-padding 0.05
node - "$recovery/holes.mp4" <<'NODE'
const fs = require('node:fs')
if (fs.readFileSync(process.argv[2]).some(byte => byte !== 0x6d)) throw Error('truncate modified snapshot data')
NODE
cmp "$root/original" "$recovery/data.mp4"
[[ "$(stat -c %a "$recovery/data.mp4")" = 640 ]]
[[ "$(stat -c %h "$recovery/data.mp4")" = 2 ]]
[[ "$(readlink "$recovery/link")" = data.mp4 ]]
[[ "$(getfattr --only-values -n user.backupfs-test "$recovery/data.mp4" 2>/dev/null)" = preserved ]]
if printf x >> "$recovery/data.mp4"; then
    echo 'recovery mount accepted a write' >&2
    exit 1
fi
if touch "$recovery/new-file"; then
    echo 'recovery mount accepted a create' >&2
    exit 1
fi
unmount_view

before=$(segment_bytes)
BACKUPFS_COMPACT_RATIO=0.05 snapshot delete "$store" before-update
after=$(segment_bytes)
[[ "$after" -lt "$before" ]]
[[ -z "$(snapshot list "$store")" ]]
mount_view "$live" --file-size-padding 0.05
node - "$live/renamed.mp4" "$live/hardlink" "$live/holes.mp4" <<'NODE'
const fs = require('node:fs')
for (const path of process.argv.slice(2, 4)) {
  if (fs.readFileSync(path).subarray(70000, 70007).toString() !== 'changed') throw Error('live data changed during snapshot deletion')
}
const expected = Buffer.alloc(4 * 1024 * 1024)
expected.fill(0x6d, 0, 1024 * 1024)
expected.write('edit', 2 * 1024 * 1024 + 9)
if (!fs.readFileSync(process.argv[4]).equals(expected)) throw Error('holes changed during snapshot deletion')
NODE
unmount_view
echo 'PASS: helper credentials, zero-copy snapshots, recovery metadata, readonly mounts, truncate/remount/regrow, and selective reclamation'
