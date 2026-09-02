import { Host } from '../osBindings'
import { deepEqual } from '../util'
import { fillHost } from '../util/filledAddress'

const host = (fingerprint: string): Host => ({
  bindings: {
    5223: {
      enabled: true,
      options: { preferredExternalPort: 5223, addSsl: null, secure: null },
      net: { assignedPort: null, assignedSslPort: 5223 },
      addresses: {
        enabled: [],
        disabled: [],
        guaWan: [],
        available: [
          {
            ssl: true,
            public: false,
            hostname: 'relay.onion',
            port: 5223,
            metadata: {
              kind: 'plugin',
              packageId: 'tor',
              removeAction: null,
              overflowActions: [],
              info: null,
            },
          },
          {
            ssl: true,
            public: false,
            hostname: 'relay.local',
            port: 5223,
            metadata: { kind: 'mdns', gateways: [] },
          },
        ],
      },
      interfaces: {
        smp: {
          id: 'smp',
          name: 'SMP',
          description: '',
          masked: true,
          type: 'api',
          addressInfo: {
            username: fingerprint,
            hostId: 'main',
            internalPort: 5223,
            scheme: 'smp',
            sslScheme: 'smp',
            suffix: '',
          },
        },
      },
    },
  },
  bindingRanges: {},
  publicDomains: {},
  privateDomains: {},
  portForwards: [],
})

const addressOf = (h: Host) =>
  fillHost(h).bindings[5223].interfaces['smp'].addressInfo

describe('fillHost', () => {
  test('a filled host is deep-equal to itself', () => {
    const filled = fillHost(host('AAAA='))
    expect(deepEqual(filled, filled)).toBe(true)
  })

  test('two fills of the same host are deep-equal', () => {
    expect(deepEqual(fillHost(host('AAAA=')), fillHost(host('AAAA=')))).toBe(
      true,
    )
  })

  test('a changed address is not deep-equal', () => {
    expect(deepEqual(fillHost(host('AAAA=')), fillHost(host('BBBB=')))).toBe(
      false,
    )
  })

  test('helpers are hidden from enumeration', () => {
    expect(Object.keys(addressOf(host('AAAA=')))).toEqual([
      'username',
      'hostId',
      'internalPort',
      'scheme',
      'sslScheme',
      'suffix',
      'hostnames',
    ])
  })

  test('helpers still resolve', () => {
    const address = addressOf(host('AAAA='))
    expect(address.filter({ kind: 'plugin' }).format('urlstring')).toEqual([
      'smp://AAAA=@relay.onion:5223',
    ])
    expect(address.nonLocal.hostnames.map(h => h.hostname)).toEqual([
      'relay.onion',
    ])
  })
})
