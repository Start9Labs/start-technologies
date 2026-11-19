import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { stripTypeScriptTypes } from 'node:module'
import test from 'node:test'

const source = await readFile(
  new URL(
    '../src/app/routes/home/routes/published-ports/utils.ts',
    import.meta.url,
  ),
  'utf8',
)
const code = stripTypeScriptTypes(source, { mode: 'transform' })
const { mapForwards, mapPinholes, portEightyForwarded } = await import(
  `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`
)

const dnat = {
  kind: 'dnat',
  target: '10.59.0.2:6000',
  count: 2,
  enabled: true,
  auto: true,
  label: null,
}
const pinhole = {
  count: 2,
  internalPort: 7000,
  enabled: false,
  auto: true,
  label: null,
}

test('rows retain protocol-specific operation keys and separate transport from IP', () => {
  const rows = mapForwards(
    {
      'tcp@192.0.2.1:5000': dnat,
      'udp@192.0.2.1:5000': dnat,
      '192.0.2.1:7000': dnat,
    },
    [],
  )
  assert.deepEqual(
    rows.map(({ source, externalip, externalport, transport }) => ({
      source,
      externalip,
      externalport,
      transport,
    })),
    [
      {
        source: 'tcp@192.0.2.1:5000',
        externalip: '192.0.2.1',
        externalport: '5000',
        transport: ['tcp'],
      },
      {
        source: 'udp@192.0.2.1:5000',
        externalip: '192.0.2.1',
        externalport: '5000',
        transport: ['udp'],
      },
      {
        source: '192.0.2.1:7000',
        externalip: '192.0.2.1',
        externalport: '7000',
        transport: ['tcp', 'udp'],
      },
    ],
  )
  const [row] = mapPinholes({ 'udp@[2001:db8::2]:5000': pinhole }, [])
  assert.equal(row.externalip, '2001:db8::2')
  assert.equal(row.externalport, '5000')
  assert.equal(row.internalport, '7000')
  assert.deepEqual(row.transport, ['udp'])
  assert.equal(row.source, 'udp@[2001:db8::2]:5000')
})

test('SNI rows report TCP and UDP mappings do not reserve the HTTP redirect', () => {
  const sni = {
    kind: 'sni',
    routes: {
      'service.example.com': {
        target: '10.59.0.2:443',
        enabled: true,
        auto: true,
        label: null,
      },
    },
    fallback: null,
  }
  assert.deepEqual(mapForwards({ '192.0.2.1:443': sni }, [])[0].transport, [
    'tcp',
  ])
  assert.equal(
    portEightyForwarded({ 'udp@192.0.2.1:79': dnat }, '192.0.2.1'),
    false,
  )
  assert.equal(
    portEightyForwarded({ 'tcp@192.0.2.1:79': dnat }, '192.0.2.1'),
    true,
  )
  assert.equal(portEightyForwarded({ '192.0.2.1:79': dnat }, '192.0.2.1'), true)
  assert.equal(
    portEightyForwarded({ 'tcp@192.0.2.2:79': dnat }, '192.0.2.1'),
    false,
  )
})
