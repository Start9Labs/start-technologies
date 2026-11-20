#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../../.."
node <<'NODE'
const assert = require('node:assert/strict')
const fs = require('node:fs')
const os = require('node:os')
const path = require('node:path')
const { spawnSync } = require('node:child_process')
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'forward-transport-'))
const stateFile = path.join(tmp, 'state.json')
const logFile = path.join(tmp, 'commands')
function executable(name, source) {
  fs.writeFileSync(path.join(tmp, name), source, { mode: 0o755 })
}
executable('nft', `#!/usr/bin/env node
const fs = require('node:fs')
const stateFile = process.env.NFT_STATE
const state = JSON.parse(fs.readFileSync(stateFile, 'utf8'))
const args = process.argv.slice(2)
if (args[0] === '-a') {
  for (const rule of state.rules.filter(r => r.family === args[3] && r.chain === args[5])) {
    console.log(rule.body + ' # handle ' + rule.handle)
  }
} else {
  const input = fs.readFileSync(0, 'utf8')
  fs.appendFileSync(process.env.COMMAND_LOG, input)
  for (const line of input.trim().split('\\n')) {
    const add = line.match(/^add rule (ip6?) startos (\\S+) (.*)$/)
    const del = line.match(/^delete rule (ip6?) startos (\\S+) handle (\\d+)$/)
    if (add) state.rules.push({family: add[1], chain: add[2], body: add[3], handle: state.next++})
    if (del) {
      const index = state.rules.findIndex(r => r.family === del[1] && r.chain === del[2] && r.handle === Number(del[3]))
      if (index < 0) process.exit(1)
      state.rules.splice(index, 1)
    }
  }
  fs.writeFileSync(stateFile, JSON.stringify(state))
}
`)
executable('ip', '#!/bin/sh\necho "2: eth0 inet 192.0.2.1/24 scope global eth0"\n')
executable('conntrack', '#!/bin/sh\nprintf "conntrack %s\\n" "$*" >> "$COMMAND_LOG"\n')
const state = () => JSON.parse(fs.readFileSync(stateFile, 'utf8')).rules
const reset = () => {
  fs.writeFileSync(stateFile, JSON.stringify({ rules: [], next: 1 }))
  fs.writeFileSync(logFile, '')
}
function run(v6, transport, count = 1, undo = false, extra = {}) {
  const env = {
    ...process.env,
    PATH: tmp + ':' + process.env.PATH,
    NFT_STATE: stateFile,
    COMMAND_LOG: logFile,
    sip: v6 ? '2001:db8::1' : '192.0.2.1',
    dip: v6 ? 'fd00:3::2' : '10.0.3.2',
    dprefix: v6 ? '64' : '24',
    sport: '5000',
    dport: '6000',
    count: String(count),
    UNDO: undo ? '1' : '0',
    ...extra,
  }
  delete env.transport
  if (transport !== undefined) env.transport = transport
  const result = spawnSync('bash', ['build/lib/scripts/forward-port' + (v6 ? '6' : '')], { env, encoding: 'utf8' })
  assert.equal(result.status, 0, result.stderr)
}
function protocols() {
  return new Set(state().map(rule => rule.body.match(/meta l4proto (tcp|udp)/)?.[1]))
}
try {
  for (const v6 of [false, true]) {
    for (const count of (v6 ? [1] : [1, 3])) {
      for (const transport of ['tcp', 'udp', 'tcp,udp', undefined]) {
        reset()
        run(v6, transport, count)
        const expected = transport === 'tcp' || transport === 'udp' ? [transport] : ['tcp', 'udp']
        assert.deepEqual(protocols(), new Set(expected))
        assert(state().every(rule => !rule.body.includes('{ tcp, udp }')))
        const log = fs.readFileSync(logFile, 'utf8')
        for (const protocol of ['tcp', 'udp']) {
          assert.equal(log.includes('conntrack -D ' + (v6 ? '-f ipv6 ' : '') + '-p ' + protocol), expected.includes(protocol))
        }
        if (!v6) {
          assert.equal(state().some(rule => rule.chain === 'forward_snat'), expected.includes('udp'))
          if (count > 1) {
            assert(state().some(rule => rule.body.includes('5000 : 10.0.3.2 . 6000, 5001 : 10.0.3.2 . 6001, 5002 : 10.0.3.2 . 6002')))
          }
        }
        const installed = state().length
        run(v6, transport, count)
        assert.equal(state().length, installed, 'reinstall replaces rules without duplication')
        run(v6, transport, count, true)
        assert.equal(state().length, 0)
      }
      for (const [before, after] of [['tcp,udp', 'tcp'], ['tcp,udp', 'udp'], ['tcp', 'udp'], ['udp', 'tcp,udp']]) {
        reset()
        run(v6, before, count)
        run(v6, before, count, true)
        run(v6, after, count)
        assert.deepEqual(protocols(), new Set(after === 'tcp,udp' ? ['tcp', 'udp'] : [after]))
      }
      reset()
      run(v6, 'tcp', count)
      const tcpRules = state()
      run(v6, 'udp', count, false, { dip: v6 ? 'fd00:3::3' : '10.0.3.3' })
      run(v6, 'udp', count, true, { dip: v6 ? 'fd00:3::3' : '10.0.3.3' })
      assert.deepEqual(state(), tcpRules, 'UDP removal leaves an independent TCP target untouched')
      run(v6, 'tcp,udp', count)
      run(v6, 'udp', count, true)
      assert.deepEqual(protocols(), new Set(['tcp']), 'removing UDP from Both leaves TCP')
    }
  }
  reset()
  run(false, 'tcp', 3, false, { dport: '5000' })
  assert(state().some(rule => rule.body.includes('th dport 5000-5002 dnat to 10.0.3.2')))
  console.log('Forwarding transport: IPv4 singles/ranges and IPv6 singles, defaults, ownership, replacement, and teardown passed')
} finally {
  fs.rmSync(tmp, { recursive: true, force: true })
}
NODE
