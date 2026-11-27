import { Effects } from '../Effects'
import { Host } from '../osBindings'
import { getBridgeAddress } from '../util/GetHostInfo'

const ipRow = (ssl: boolean, port: number) => ({
  ssl,
  public: false,
  hostname: '10.0.3.1',
  port,
  metadata: { kind: 'ipv4', gateway: 'lxcbr0' },
})

const nameRow = (ssl: boolean, port: number, gateways = ['lxcbr0']) => ({
  ssl,
  public: false,
  hostname: 'startos.internal',
  port,
  metadata: { kind: 'private-domain', gateways },
})

const host = {
  bindings: {
    8332: {
      enabled: true,
      options: {},
      net: { assignedPort: 8332, assignedSslPort: 54404 },
      addresses: {
        effective: [
          ipRow(false, 8332),
          ipRow(true, 54404),
          nameRow(true, 443, ['lxcbr0', 'eth0']),
          nameRow(false, 8332),
          nameRow(true, 54404),
        ],
        available: [],
      },
      interfaces: {},
    },
  },
  bindingRanges: {},
  publicDomains: {},
  privateDomains: {},
  portForwards: [],
} as unknown as Host

const effects = (h: Host | null) =>
  ({
    getHostInfo: jest.fn(async () => h),
    getOsIp: jest.fn(async () => '10.0.3.1'),
  }) as unknown as Effects

describe('getBridgeAddress', () => {
  test('names the server rather than its bridge IP', async () => {
    const at = (ssl: boolean) =>
      getBridgeAddress(effects(host), {
        hostId: 'rpc',
        packageId: 'bitcoind',
        internalPort: 8332,
        ssl,
      }).once()
    expect(await at(false)).toBe('startos.internal:8332')
    expect(await at(true)).toBe('startos.internal:54404')
  })

  test('an absent dependency falls back to the server IP', async () => {
    const opts = { hostId: 'socks', packageId: 'tor', internalPort: 9050 }
    expect(await getBridgeAddress(effects(null), opts).once()).toBeNull()
    expect(
      await getBridgeAddress(effects(null), {
        ...opts,
        fallbackPort: 9050,
      }).once(),
    ).toBe('10.0.3.1:9050')
  })
})
