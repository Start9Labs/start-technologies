import { Effects } from '../Effects'
import { Host } from '../osBindings'
import { getBridgeAddress } from '../util/GetHostInfo'

const bridgeRow = (ssl: boolean, port: number) => ({
  ssl,
  public: false,
  hostname: '10.0.3.1',
  port,
  metadata: { kind: 'ipv4', gateway: 'lxcbr0' },
})

const host = {
  bindings: {
    8332: {
      enabled: true,
      options: {},
      net: { assignedPort: 8332, assignedSslPort: 54404 },
      addresses: {
        effective: [bridgeRow(false, 8332), bridgeRow(true, 54404)],
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

  test('an absent dependency falls back to the server name', async () => {
    const opts = { hostId: 'socks', packageId: 'tor', internalPort: 9050 }
    expect(await getBridgeAddress(effects(null), opts).once()).toBeNull()
    expect(
      await getBridgeAddress(effects(null), {
        ...opts,
        fallbackPort: 9050,
      }).once(),
    ).toBe('startos.internal:9050')
  })
})
