import { Host, HostnameInfo } from '../osBindings'
import { deepEqual } from '../util'
import {
  fillHost,
  isAddressEnabled,
  isAddressInsecure,
} from '../util/filledAddress'

const host = (fingerprint: string): Host =>
  ({
    bindings: {
      5223: {
        enabled: true,
        proxyChains: [],
        options: {
          transport: ['tcp', 'udp'],
          preferredExternalPort: 5223,
          addSsl: null,
          secure: null,
          proxy: null,
        },
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
            preferredLauncherAddress: null,
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
  }) as unknown as Host

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

describe('legacy server LAN address overrides', () => {
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

  test('an enabled mDNS address is listed beside disabled LAN IPs', () => {
    const { h, addresses } = lan(eth)
    addresses.disabled = [[eth.hostname, 5223]]

    expect(hostnames(h)).toEqual(['relay.onion', 'relay.local'])
  })

  test('plaintext mDNS stays on beside disabled visible IPs and hidden link-local', () => {
    const linkLocal: HostnameInfo = {
      ssl: false,
      public: false,
      hostname: 'fe80::10',
      port: 5223,
      metadata: { kind: 'ipv6', gateway: 'eth0', scopeId: 2 },
    }
    const { h, addresses } = lan(eth, gua, linkLocal)
    addresses.available = addresses.available.map(a => ({ ...a, ssl: false }))
    addresses.disabled = [[eth.hostname, 5223]]
    const mdns = addresses.available.find(a => a.metadata.kind === 'mdns')!

    expect(isAddressEnabled(addresses, mdns)).toBe(true)
    expect(isAddressEnabled(addresses, eth)).toBe(false)
    expect(isAddressEnabled(addresses, gua)).toBe(false)
    expect(isAddressEnabled(addresses, linkLocal)).toBe(true)
    expect(addressOf(h).nonLocal.hostnames.map(a => a.hostname)).toEqual([
      'relay.onion',
      'relay.local',
    ])

    addresses.disabled.push([linkLocal.hostname, 5223])
    expect(isAddressEnabled(addresses, mdns)).toBe(true)
    expect(addressOf(h).hostnames.map(a => a.hostname)).toEqual([
      'relay.onion',
      'relay.local',
    ])
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

  test('an enabled public GUA leaves a disabled mDNS address off', () => {
    const { addresses } = lan(gua)
    addresses.available = addresses.available.map(a => ({ ...a, ssl: false }))
    addresses.enabled = ['[2001:db8::10]:5223']
    addresses.disabled = [['relay.local', 5223]]
    const mdns = addresses.available.find(a => a.metadata.kind === 'mdns')!

    expect(isAddressEnabled(addresses, gua)).toBe(true)
    expect(isAddressEnabled(addresses, mdns)).toBe(false)
  })
})

describe('server-owned effective addresses', () => {
  test('an empty effective selection never falls back to legacy defaults', () => {
    const h = host('AAAA=')
    const addresses = h.bindings[5223].addresses
    addresses.effective = []
    expect(addressOf(h).hostnames).toEqual([])
    expect(isAddressEnabled(addresses, addresses.available[0])).toBe(false)
  })

  test('consumed selections distinguish the same name on different gateways', () => {
    const h = host('AAAA=')
    const addresses = h.bindings[5223].addresses
    const name = (gateway: string): HostnameInfo => ({
      ssl: false,
      public: false,
      hostname: 'relay.local',
      port: 5223,
      metadata: { kind: 'mdns', gateways: [gateway] },
    })
    const trusted = name('eth0')
    const insecure = name('wlan0')
    addresses.available = [trusted, insecure]
    addresses.insecure = [['relay.local', 5223, 'wlan0']]
    addresses.effective = [name('eth0')]
    expect(isAddressEnabled(addresses, trusted)).toBe(true)
    expect(isAddressEnabled(addresses, insecure)).toBe(false)
    expect(isAddressInsecure(addresses, trusted)).toBe(false)
    expect(isAddressInsecure(addresses, insecure)).toBe(true)
    addresses.effective.push(name('wlan0'))
    expect(isAddressEnabled(addresses, insecure)).toBe(true)
    addresses.effective = [trusted]
    expect(isAddressEnabled(addresses, insecure)).toBe(false)
  })
})
