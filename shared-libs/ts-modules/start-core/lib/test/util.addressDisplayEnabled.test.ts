import { T } from '..'
import { addressDisplayEnabled } from '../util/filledAddress'

const gateway = 'eth0'
const port = 443

function address(
  hostname: string,
  metadata: T.HostnameMetadata,
  options: Partial<Pick<T.HostnameInfo, 'ssl' | 'port'>> = {},
): T.HostnameInfo {
  return {
    ssl: options.ssl ?? true,
    public: false,
    hostname,
    port: options.port ?? port,
    metadata,
  }
}

describe('addressDisplayEnabled', () => {
  test('mDNS remains enabled when its LAN address is filtered from the page', () => {
    const mdns = address('server.local', { kind: 'mdns', gateways: [gateway] })
    const linkLocal = address('fe80::1', {
      kind: 'ipv6',
      gateway,
      scopeId: 2,
    })
    const addr: T.DerivedAddressInfo = {
      available: [mdns, linkLocal],
      enabled: [],
      disabled: [],
      guaWan: [],
    }

    expect(addressDisplayEnabled(addr, mdns, gateway)).toBe(true)
  })

  test('mDNS is disabled when every LAN address on its gateway is disabled', () => {
    const mdns = address('server.local', { kind: 'mdns', gateways: [gateway] })
    const lan = address('192.168.1.2', { kind: 'ipv4', gateway })
    const addr: T.DerivedAddressInfo = {
      available: [mdns, lan],
      enabled: [],
      disabled: [[lan.hostname, port]],
      guaWan: [],
    }

    expect(addressDisplayEnabled(addr, mdns, gateway)).toBe(false)
  })

  test('an enabled plaintext leg does not enable the disabled HTTPS mDNS row', () => {
    const mdns = address('server.local', { kind: 'mdns', gateways: [gateway] })
    const https = address('192.168.1.2', { kind: 'ipv4', gateway })
    const http = address(
      '192.168.1.2',
      { kind: 'ipv4', gateway },
      { ssl: false, port: 80 },
    )
    const addr: T.DerivedAddressInfo = {
      available: [mdns, https, http],
      enabled: [],
      disabled: [[https.hostname, port]],
      guaWan: [],
    }

    expect(addressDisplayEnabled(addr, mdns, gateway)).toBe(false)
  })

  test('mDNS reachability is evaluated for the displayed gateway', () => {
    const otherGateway = 'eth1'
    const mdns = address('server.local', {
      kind: 'mdns',
      gateways: [gateway, otherGateway],
    })
    const disabledLan = address('192.168.1.2', { kind: 'ipv4', gateway })
    const enabledLan = address('fe80::2', {
      kind: 'ipv6',
      gateway: otherGateway,
      scopeId: 3,
    })
    const addr: T.DerivedAddressInfo = {
      available: [mdns, disabledLan, enabledLan],
      enabled: [],
      disabled: [[disabledLan.hostname, port]],
      guaWan: [],
    }

    expect(addressDisplayEnabled(addr, mdns, gateway)).toBe(false)
    expect(addressDisplayEnabled(addr, mdns, otherGateway)).toBe(true)
  })
})
