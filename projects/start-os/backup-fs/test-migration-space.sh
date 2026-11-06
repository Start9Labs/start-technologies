#!/usr/bin/env bash
set -euo pipefail

bin=$(realpath "$1")
fixture=$(realpath "$(dirname "$0")/testdata/legacy-format1.tar.gz")
docker run --rm --tmpfs /store:rw,size=3m \
    -v "$fixture:/tmp/legacy.tar.gz:ro" -v "$bin:/tmp/startos-backup-fs:ro" \
    start9/start-core-testenv:latest bash -euc '
    tar -xzf /tmp/legacy.tar.gz -C /store
    chown -R "$(id -u):$(id -g)" /store
    chmod 755 /store
    node <<"NODE"
const fs = require("node:fs")
fs.mkdirSync("/store/contents/ffff", {recursive: true})
for (let i = 0; i < 16; i++) {
  fs.writeFileSync(`/store/contents/ffff/${i.toString(16).padStart(28, "0")}`, Buffer.alloc(128 * 1024, 0x50))
}
const stats = fs.statfsSync("/store")
if (stats.bavail * stats.bsize >= 2 * 1024 * 1024) throw Error("fixture must not fit a full copy")
console.log(`Legacy objects exceed available staging space: ${stats.bavail * stats.bsize} bytes free`)
NODE
    printf password | /tmp/startos-backup-fs convert /store
    printf password | /tmp/startos-backup-fs snapshot create /store space-check
    printf password | /tmp/startos-backup-fs snapshot list /store | grep -Fx space-check
    test -f /store/segments/state
    echo "PASS: upgrade completed without staging space for a second copy of the legacy objects"
'
