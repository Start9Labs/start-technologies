import assert from 'node:assert/strict'
import test from 'node:test'
import {
  portEightyForwarded,
  resolveSniProtocol,
  sniBindings,
  sniRoute,
} from '../src/app/utils/port-forwards.ts'

function forward() {
  return {
    kind: 'sni',
    tcp: { routes: {}, fallback: null },
    udp: { routes: {}, fallback: null },
    both: { routes: {}, fallback: null },
  }
}

function route(target = '10.59.0.2:443', enabled = false) {
  return { target, enabled, label: null, auto: false }
}

test('each protocol selects its explicit bucket', () => {
  const entry = forward()
  assert.equal(sniBindings(entry, 'tcp'), entry.tcp)
  assert.equal(sniBindings(entry, 'udp'), entry.udp)
  assert.equal(sniBindings(entry, 'tcp+udp'), entry.both)
})

test('omitted management follows disabled TCP ownership, not UDP-only intent', () => {
  for (const [owner, expected] of [
    ['tcp', 'tcp'],
    ['udp', 'tcp'],
    ['tcp+udp', 'tcp+udp'],
  ]) {
    for (const hostname of [null, 'SERVICE.EXAMPLE.COM']) {
      const entry = forward()
      const mapping = route()
      if (hostname)
        sniBindings(entry, owner).routes['service.example.com'] = mapping
      else sniBindings(entry, owner).fallback = mapping
      for (const omitted of [undefined, null]) {
        assert.equal(resolveSniProtocol(entry, omitted, hostname), expected)
        const selected = sniRoute(entry, expected, hostname)
        if (owner === 'udp') assert.ok(selected == null)
        else assert.equal(selected, mapping)
      }
      for (const explicit of ['tcp', 'udp', 'tcp+udp'])
        assert.equal(resolveSniProtocol(entry, explicit, hostname), explicit)
    }
  }
})

test('independent singles and combined fallback retain separate metadata', () => {
  const entry = forward()
  entry.tcp.routes['service.example.com'] = route()
  entry.udp.routes['service.example.com'] = route('10.59.0.3:8443')
  entry.both.fallback = route()
  const protocol = resolveSniProtocol(entry, undefined, 'SERVICE.EXAMPLE.COM')
  sniRoute(entry, protocol, 'SERVICE.EXAMPLE.COM').label = 'renamed'
  assert.equal(entry.tcp.routes['service.example.com'].label, 'renamed')
  assert.equal(entry.udp.routes['service.example.com'].label, null)
  assert.equal(resolveSniProtocol(entry, undefined, null), 'tcp+udp')
})

test('TCP and combined intent reserve port 80, including disabled routes and fallbacks', () => {
  for (const protocol of ['tcp', 'udp', 'tcp+udp']) {
    for (const enabled of [false, true]) {
      for (const fallback of [false, true]) {
        const entry = forward()
        if (fallback)
          sniBindings(entry, protocol).fallback = route(undefined, enabled)
        else
          sniBindings(entry, protocol).routes['service.example.com'] = route(
            undefined,
            enabled,
          )
        const forwards = { '203.0.113.1:80': entry }
        assert.equal(
          portEightyForwarded(forwards, '203.0.113.1'),
          protocol !== 'udp',
        )
        assert.equal(portEightyForwarded(forwards, '203.0.113.2'), false)
      }
    }
  }
  assert.equal(
    portEightyForwarded({ '203.0.113.1:80': forward() }, '203.0.113.1'),
    false,
  )
})

test('plain ranges reserve only the addresses and ports they cover', () => {
  const entry = {
    kind: 'dnat',
    target: '10.59.0.2:79',
    enabled: false,
    label: null,
    auto: false,
    count: 2,
  }
  assert.equal(
    portEightyForwarded({ '203.0.113.1:79': entry }, '203.0.113.1'),
    true,
  )
  entry.count = 1
  assert.equal(
    portEightyForwarded({ '203.0.113.1:79': entry }, '203.0.113.1'),
    false,
  )
  assert.equal(
    portEightyForwarded({ '203.0.113.1:81': entry }, '203.0.113.1'),
    false,
  )
})
