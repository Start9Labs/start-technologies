#!/usr/bin/env bash
set -euo pipefail

bin=$(realpath "$1")
fixture=$(realpath "$(dirname "$0")/testdata/legacy-format1.tar.gz")
docker run --rm --cap-add SYS_PTRACE --security-opt seccomp=unconfined \
    --tmpfs /store:rw,size=32m \
    -v "$fixture:/tmp/legacy.tar.gz:ro" -v "$bin:/tmp/startos-backup-fs:ro" \
    -e DEBIAN_FRONTEND=noninteractive start9/start-core-testenv:latest bash -euc '
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends strace
    tar -xzf /tmp/legacy.tar.gz -C /store
    chown -R "$(id -u):$(id -g)" /store
    chmod 755 /store
    node <<"NODE"
const fs = require("node:fs")
function files(dir) {
  return fs.readdirSync(dir, {withFileTypes: true}).flatMap(e => e.isDirectory() ? files(`${dir}/${e.name}`) : [`${dir}/${e.name}`])
}
const replaced = files("/store").filter(p => p.endsWith(".seg"))
if (!replaced.length) throw Error("fixture lacks legacy segments")
fs.writeFileSync("/tmp/replaced.json", JSON.stringify(replaced))
NODE
    printf password | strace -f -yy -o /tmp/trace -e trace=fsync,rename,syncfs /tmp/startos-backup-fs convert /store
    node <<"NODE"
const fs = require("node:fs")
const path = require("node:path")
const replaced = JSON.parse(fs.readFileSync("/tmp/replaced.json", "utf8"))
const lines = fs.readFileSync("/tmp/trace", "utf8").split("\n")
for (const target of replaced) {
  const tmp = target.slice(0, target.lastIndexOf(".")) + ".tmp"
  const rename = lines.findIndex(line => line.includes(`rename("${tmp}", "${target}")`) && line.endsWith("= 0"))
  if (rename < 0) throw Error(`missing replacement: ${target}`)
  const synced = lines.slice(0, rename).some(line => line.includes(`fsync(`) && line.includes(`<${tmp}>`) && line.endsWith("= 0"))
  if (!synced) throw Error(`replacement lacks preceding data fsync: ${target}`)
  const following = lines.slice(rename + 1).find(line => line.includes("fsync(") || line.includes("rename("))
  if (!following?.includes(`<${path.dirname(target)}>`)) throw Error(`replacement lacks immediate parent fsync: ${target}`)
}
console.log(`PASS: ${replaced.length} in-place replacements sync data before rename and their parent afterward`)
NODE
'
