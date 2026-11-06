#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../../.."
node - "${1:-}" <<'NODE'
const assert = require('node:assert/strict')
const fs = require('node:fs')
const os = require('node:os')
const path = require('node:path')
const { spawnSync } = require('node:child_process')
const read = file => fs.readFileSync(file, 'utf8')
const base = 'projects/start-os/'
assert.match(read(base + 'build/dpkg-deps/depends'), /^btrfsmaintenance$/m)
const postinst = read(base + 'debian/postinst')
const setConfig = postinst.match(/^set_config\(\) \{[\s\S]*?^\}/m)[0]
const configure = postinst.match(/^set_config \/etc\/default\/btrfsmaintenance[\s\S]*?^SYSTEMD_OFFLINE=.*$/m)[0]
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'startos-btrfsmaintenance-'))
const packageRoot = process.argv[2]
const configPath = tmp + '/etc/default/btrfsmaintenance'
const refreshPath = tmp + '/usr/share/btrfsmaintenance/btrfsmaintenance-refresh-cron.sh'
try {
  if (packageRoot) {
    fs.cpSync(packageRoot, tmp, { recursive: true })
    fs.rmSync(tmp + '/etc/systemd/system/timers.target.wants/btrfs-balance.timer', { force: true })
    const refresh = read(refreshPath)
      .replaceAll('/etc/', tmp + '/etc/')
      .replaceAll('systemctl ', `systemctl --root=${tmp} `)
    fs.writeFileSync(refreshPath, refresh)
  } else {
    fs.mkdirSync(path.dirname(configPath), { recursive: true })
    fs.mkdirSync(path.dirname(refreshPath), { recursive: true })
    fs.writeFileSync(configPath, 'BTRFS_BALANCE_MOUNTPOINTS="/"\nBTRFS_BALANCE_PERIOD="none"\nBTRFS_BALANCE_DUSAGE="5 10"\nBTRFS_BALANCE_MUSAGE="5"\nBTRFS_SCRUB_PERIOD="monthly"\nBTRFS_TRIM_PERIOD="none"\nBTRFS_DEFRAG_PERIOD="none"\nBTRFS_ALLOW_CONCURRENCY="false"\n')
    fs.writeFileSync(refreshPath, `#!/bin/sh\nset -eu\n[ "$SYSTEMD_OFFLINE" = 1 ]\n[ "$1" = systemd-timer ]\nprintf refreshed > ${tmp}/refreshed\n`, { mode: 0o755 })
  }
  const settings = () => {
    const result = spawnSync('sh', ['-ec', `. ${configPath}; set`], { encoding: 'utf8' })
    assert.equal(result.status, 0, result.stderr)
    return Object.fromEntries(result.stdout.split('\n').filter(line => line.startsWith('BTRFS_')).map(line => {
      const index = line.indexOf('=')
      return [line.slice(0, index), line.slice(index + 1).replace(/^'|'$/g, '')]
    }))
  }
  const before = settings()
  const expected = { ...before, BTRFS_BALANCE_MOUNTPOINTS: '/media/startos/data/main:/media/startos/data/package-data', BTRFS_BALANCE_PERIOD: 'weekly', BTRFS_SCRUB_PERIOD: 'none' }
  for (let run = 0; run < 2; run++) {
    const result = spawnSync('sh', ['-ec', setConfig + '\n' + configure.replaceAll('/etc/', tmp + '/etc/').replaceAll('/usr/share/', tmp + '/usr/share/')], { encoding: 'utf8' })
    assert.equal(result.status, 0, result.stderr)
    assert.deepEqual(settings(), expected)
    if (packageRoot) {
      assert.equal(read(tmp + '/etc/systemd/system/btrfs-balance.timer.d/schedule.conf'), '[Timer]\nOnCalendar=\nOnCalendar=weekly\n')
      assert.ok(fs.readlinkSync(tmp + '/etc/systemd/system/timers.target.wants/btrfs-balance.timer').endsWith('/btrfs-balance.timer'))
      for (const task of ['scrub', 'trim', 'defrag']) {
        assert.equal(fs.existsSync(tmp + `/etc/systemd/system/timers.target.wants/btrfs-${task}.timer`), false)
      }
    } else {
      assert.equal(read(tmp + '/refreshed'), 'refreshed')
    }
  }
  if (packageRoot) {
    const bin = tmp + '/bin'
    fs.mkdirSync(bin, { recursive: true })
    fs.mkdirSync(tmp + '/run', { recursive: true })
    const commands = {
      btrfs: `#!/bin/bash
if [[ "$1 $2" == 'filesystem show' ]]; then
  echo "uuid: fixture-\${3##*/}"
elif [[ "$1 $2" == 'balance start' ]]; then
  printf '%s\\n' "$*" >> "$BALANCE_LOG"
fi
`,
      stat: `#!/bin/bash
case "\${!#}" in
  /media/startos/data/main) if [ "$SKIP_MAIN" = 1 ]; then echo ext4; else echo btrfs; fi;;
  /media/startos/data/package-data) echo btrfs;;
  *) echo overlayfs;;
esac
`,
      df: '#!/bin/sh\nexit 0\n',
      'systemd-cat': '#!/bin/sh\ncat\n',
    }
    for (const [name, source] of Object.entries(commands)) fs.writeFileSync(bin + '/' + name, source, { mode: 0o755 })
    const balancePath = tmp + '/usr/share/btrfsmaintenance/btrfs-balance.sh'
    fs.writeFileSync(balancePath, read(balancePath).replaceAll('/etc/', tmp + '/etc/').replace('PATH=/sbin:', `PATH=${bin}:/sbin:`))
    const functionsPath = path.dirname(balancePath) + '/btrfsmaintenance-functions'
    fs.writeFileSync(functionsPath, read(functionsPath).replaceAll('/run/btrfs-', tmp + '/run/btrfs-'))
    const log = tmp + '/balance.log'
    for (const skipMain of ['0', '1']) {
      fs.writeFileSync(log, '')
      const result = spawnSync('bash', [balancePath], { encoding: 'utf8', env: { ...process.env, BALANCE_LOG: log, SKIP_MAIN: skipMain } })
      assert.equal(result.status, 0, result.stderr)
      const expected = []
      const paths = skipMain === '0' ? ['/media/startos/data/main', '/media/startos/data/package-data'] : ['/media/startos/data/package-data']
      for (const target of paths) {
        expected.push(`balance start -dusage=0 ${target}`, `balance start -v -dusage=5 ${target}`, `balance start -v -dusage=10 ${target}`, `balance start -musage=0 ${target}`, `balance start -v -musage=5 ${target}`)
      }
      assert.deepEqual(read(log).trim().split('\n'), expected)
    }
  }
} finally {
  fs.rmSync(tmp, { recursive: true, force: true })
}
console.log('Btrfs maintenance targets both data filesystems weekly and preserves upstream filtered-balance defaults' + (packageRoot ? ' (Debian package integration)' : ''))
NODE
