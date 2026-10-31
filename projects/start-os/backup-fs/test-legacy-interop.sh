#!/bin/bash
set -Eeuo pipefail
trap 'printf "legacy interoperability assertion failed at line %s\n" "$LINENO" >&2' ERR
current=$(realpath "${1:?current helper required}")
legacy=$(realpath "${2:?pre-PR format-1 helper required}")
work=$(mktemp -d)
store="$work/store"
mounted="$work/mounted"
expected="$work/expected"
pid=
cleanup() {
  if mountpoint -q "$mounted"; then fusermount3 -u "$mounted" || true; fi
  if [ -n "$pid" ]; then kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; fi
  rm -rf "$work"
}
trap cleanup EXIT
mkdir -p "$store" "$mounted" "$expected"
printf password > "$work/password"
mount_fs() {
  "$@" > "$work/mount.log" 2>&1 &
  pid=$!
  for _ in $(seq 1 200); do
    if mountpoint -q "$mounted"; then return; fi
    if ! kill -0 "$pid" 2>/dev/null; then
      printf 'helper exited before mounting\n' >&2
      exit 1
    fi
    sleep 0.05
  done
  printf 'mount readiness timed out\n' >&2
  exit 1
}
close_fs() {
  fusermount3 -u "$mounted"
  wait "$pid"
  pid=
}
verify() {
  diff -r --no-dereference "$expected" "$mounted"
  test "$(stat -c '%a %u %g %h' "$expected/packed")" = "$(stat -c '%a %u %g %h' "$mounted/packed")"
  test "$(stat -c '%i' "$mounted/packed")" = "$(stat -c '%i' "$mounted/hardlink")"
  test "$(getfattr --only-values -n user.interop "$mounted/packed" 2>/dev/null)" = 'original'
}
legacy_format() {
  test "$("$current" inspect "$store")" = '"legacy"'
}
checked_error() {
  local result
  if "$@" > "$work/error.log" 2>&1; then result=0; else result=$?; fi
  test "$result" = 3
  ! grep -q 'panicked' "$work/error.log"
}
node - "$expected" <<'JS'
const fs = require('node:fs'), crypto = require('node:crypto'), path = require('node:path')
const base = process.argv[2]
fs.writeFileSync(path.join(base, 'inline'), 'original inline data')
fs.writeFileSync(path.join(base, 'packed'), Buffer.alloc(12000, 0x42))
fs.writeFileSync(path.join(base, 'block'), crypto.randomBytes(4 * 1024 * 1024 + 97))
fs.mkdirSync(path.join(base, 'directory'))
fs.writeFileSync(path.join(base, 'directory/nested'), 'nested data')
fs.symlinkSync('directory/nested', path.join(base, 'symlink'))
fs.linkSync(path.join(base, 'packed'), path.join(base, 'hardlink'))
fs.chownSync(path.join(base, 'packed'), 123, 456)
fs.chmodSync(path.join(base, 'packed'), 0o640)
JS
setfattr -n user.interop -v original "$expected/packed"
mount_fs "$legacy" mount "$store" "$mounted" --password password
cp -a "$expected/." "$mounted/"
verify
close_fs
legacy_format

mount_fs "$current" mount "$store" "$mounted" --password-file "$work/password" --readonly -o ro
verify
close_fs
legacy_format

mount_fs "$current" mount "$store" "$mounted" --password-file "$work/password"
node - "$expected" "$mounted" <<'JS'
const fs = require('node:fs'), path = require('node:path')
for (const base of process.argv.slice(2)) {
  const block = fs.openSync(path.join(base, 'block'), 'r+')
  fs.writeSync(block, Buffer.alloc(211, 0x71), 0, 211, 65517)
  fs.ftruncateSync(block, 1048613)
  fs.ftruncateSync(block, 3 * 1024 * 1024 + 47)
  fs.closeSync(block)
  fs.appendFileSync(path.join(base, 'packed'), Buffer.alloc(2 * 1024 * 1024 + 17, 0x65))
  fs.writeFileSync(path.join(base, 'new-inline'), 'written by the current helper')
  fs.renameSync(path.join(base, 'directory/nested'), path.join(base, 'directory/renamed'))
  fs.unlinkSync(path.join(base, 'symlink'))
  fs.symlinkSync('directory/renamed', path.join(base, 'symlink'))
}
JS
verify
close_fs
legacy_format
for operation in create delete; do
  checked_error "$current" snapshot "$operation" "$store" unavailable --password-file "$work/password"
  legacy_format
done
test -z "$("$current" snapshot list "$store" --password-file "$work/password")"
checked_error "$current" change-password "$store" --password-file "$work/password" --new-password-file "$work/missing"

mount_fs "$legacy" mount "$store" "$mounted" --password password --readonly -o ro
verify
close_fs
mount_fs "$legacy" mount "$store" "$mounted" --password password
for base in "$expected" "$mounted"; do
  printf 'written by the old helper after new-helper writes' > "$base/from-old"
  printf 'old append' >> "$base/inline"
  mv "$base/new-inline" "$base/renamed-by-old"
done
verify
close_fs
legacy_format
mount_fs "$current" mount "$store" "$mounted" --password-file "$work/password" --readonly -o ro
verify
close_fs
legacy_format

"$current" convert "$store" --password-file "$work/password" > "$work/progress.jsonl"
test "$("$current" inspect "$store")" = '"current"'
mount_fs "$current" mount "$store" "$mounted" --password-file "$work/password" --readonly -o ro
verify
close_fs
if "$legacy" fsck "$store" --password password > "$work/old-reader.log" 2>&1; then
  printf 'old helper unexpectedly accepted the converted store\n' >&2
  exit 1
fi
printf 'PASS: old/new FUSE read/write interoperability, unchanged legacy format, explicit conversion and old-reader rejection\n'
