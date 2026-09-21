import { Host, HostnameInfo } from '../osBindings'
import { deepEqual } from '../util'
import { fillHost, isAddressEnabled } from '../util/filledAddress'

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
        lanEnabled: [],
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
            metadata: { kind: 'mdns', gateways: ['eth0', 'wlan0'] },
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
      'relay.local',
    ])
  })
})

describe('LAN address overrides', () => {
  const lanIp = (hostname: string, gateway: string): HostnameInfo => ({
    ssl: true,
    public: false,
    hostname,
    port: 5223,
    metadata: { kind: 'ipv4', gateway },
  })
  const eth = lanIp('192.0.2.10', 'eth0')
  const wifi = lanIp('198.51.100.10', 'wlan0')
  const gua: HostnameInfo = {
    ssl: false,
    public: true,
    hostname: '2001:db8::10',
    port: 5223,
    metadata: { kind: 'ipv6', gateway: 'eth0', scopeId: 0 },
  }

  const lan = (...ips: HostnameInfo[]) => {
    const h = host('AAAA=')
    const addresses = h.bindings[5223].addresses
    addresses.available.push(...ips)
    return { h, addresses }
  }
  const hostnames = (h: Host) => addressOf(h).hostnames.map(a => a.hostname)

  test('an mDNS address stays while its gateways hold no LAN IP', () => {
    expect(hostnames(lan().h)).toContain('relay.local')
  })

  test('a LAN IP is disabled on its own', () => {
    const { h, addresses } = lan(eth, wifi)
    addresses.disabled = [[eth.hostname, 5223]]

    expect(hostnames(h)).toEqual(['relay.onion', 'relay.local', wifi.hostname])
  })

  test('an mDNS address with every LAN IP disabled is left out', () => {
    const { h, addresses } = lan(eth)
    addresses.disabled = [[eth.hostname, 5223]]
    const mdns = addresses.available.find(a => a.metadata.kind === 'mdns')!

    expect(isAddressEnabled(addresses, mdns)).toBe(true)
    expect(hostnames(h)).toEqual(['relay.onion'])
  })

  test('a LAN IP without an override follows its mDNS address', () => {
    const { h, addresses } = lan(eth, wifi)
    addresses.disabled = [['relay.local', 5223]]
    addresses.lanEnabled = [[eth.hostname, 5223]]

    expect(hostnames(h)).toEqual(['relay.onion', eth.hostname])
  })

  test('an enabled LAN IP serves its mDNS address on a non-SSL port', () => {
    const { addresses } = lan(eth)
    addresses.available = addresses.available.map(a => ({ ...a, ssl: false }))
    addresses.disabled = [['relay.local', 5223]]
    const mdns = addresses.available.find(a => a.metadata.kind === 'mdns')!

    expect(isAddressEnabled(addresses, mdns)).toBe(false)
    addresses.lanEnabled = [[eth.hostname, 5223]]
    expect(isAddressEnabled(addresses, mdns)).toBe(true)
  })

  test('an enabled public GUA keeps mDNS reachable on a non-SSL port', () => {
    const { h, addresses } = lan(eth, gua)
    addresses.available = addresses.available.map(a => ({ ...a, ssl: false }))
    addresses.disabled = [
      ['relay.local', 5223],
      [eth.hostname, 5223],
    ]
    addresses.enabled = [`[${gua.hostname}]:5223`]

    expect(hostnames(h)).toEqual(['relay.onion', 'relay.local', gua.hostname])
  })

  test('a disabled public GUA does not look like a disconnected gateway', () => {
    const { h } = lan(gua)

    expect(hostnames(h)).toEqual(['relay.onion'])
  })
})
