import { Effects } from '@start9labs/start-core/Effects'
import * as T from '@start9labs/start-core/types'
import { compareGateways } from '../util/compareGateways'

const pub = (
  hostname: string,
  metadata: T.HostnameMetadata,
  port = 3478,
): T.HostnameInfo => ({ ssl: false, public: true, hostname, port, metadata })

const tunnelV4 = pub('134.199.220.102', { kind: 'ipv4', gateway: 'wg1' })
const routerV4 = pub('73.34.87.9', { kind: 'ipv4', gateway: 'eno0' })
const tunnelV6 = pub('2604:a880:4:1d0::5', {
  kind: 'ipv6',
  gateway: 'wg1',
  scopeId: 4,
})
const domain = pub('turn.example.com', {
  kind: 'public-domain',
  gateway: 'wg1',
})

const addresses = (
  available: T.HostnameInfo[],
  enabled: string[],
): T.DerivedAddressInfo => ({
  enabled,
  disabled: [],
  guaWan: [],
  lanEnabled: [],
  available,
})

const host = (
  binding: T.DerivedAddressInfo,
  range?: T.DerivedAddressInfo,
): T.Host => ({
  bindings: {
    3478: {
      enabled: true,
      options: {
        preferredExternalPort: 3478,
        addSsl: null,
        secure: null,
        proxy: null,
      },
      net: { assignedPort: 3478, assignedSslPort: null },
      addresses: binding,
      interfaces: {},
    },
  },
  bindingRanges: range
    ? {
        42000: {
          enabled: true,
          externalStartPort: 42000,
          numberOfPorts: 500,
          addresses: range,
          interface: null,
        },
      }
    : {},
  publicDomains: {},
  privateDomains: {},
  portForwards: [],
})

const effects = (outbound: string, h: T.Host | null) =>
  ({
    getOutboundGateway: async () => outbound,
    getHostInfo: async () => h,
  }) as unknown as Effects

describe('compareGateways', () => {
  it('resolves each enabled public address to its gateway', async () => {
    const h = host(
      addresses(
        [tunnelV4, routerV4, domain],
        ['134.199.220.102:3478', '73.34.87.9:3478'],
      ),
    )
    expect(
      await compareGateways(effects('eno0', h), 'turn', [
        '134.199.220.102',
        '73.34.87.9',
        'TURN.example.com',
      ]),
    ).toEqual({
      outbound: 'eno0',
      addresses: [
        { address: '134.199.220.102', gateway: 'wg1' },
        { address: '73.34.87.9', gateway: 'eno0' },
        { address: 'TURN.example.com', gateway: 'wg1' },
      ],
    })
  })

  it('gives no gateway to an address that is disabled or absent', async () => {
    const h = host(addresses([tunnelV4, routerV4], ['134.199.220.102:3478']))
    expect(
      (
        await compareGateways(effects('wg1', h), 'turn', [
          '73.34.87.9',
          '203.0.113.7',
        ])
      ).addresses,
    ).toEqual([
      { address: '73.34.87.9', gateway: null },
      { address: '203.0.113.7', gateway: null },
    ])
  })

  it('matches IPv6 in any spelling', async () => {
    const h = host(addresses([tunnelV6], ['[2604:a880:4:1d0::5]:3478']))
    expect(
      (
        await compareGateways(effects('wg1', h), 'turn', [
          '2604:a880:4:1d0:0:0:0:5',
        ])
      ).addresses,
    ).toEqual([{ address: '2604:a880:4:1d0:0:0:0:5', gateway: 'wg1' }])
  })

  it('reads port-range bindings', async () => {
    const h = host(
      addresses([], []),
      addresses([{ ...routerV4, port: 42000 }], ['73.34.87.9:42000']),
    )
    expect(
      (await compareGateways(effects('wg1', h), 'turn', ['73.34.87.9']))
        .addresses,
    ).toEqual([{ address: '73.34.87.9', gateway: 'eno0' }])
  })

  it('treats a missing host as publishing nothing', async () => {
    expect(
      await compareGateways(effects('eno0', null), 'turn', ['134.199.220.102']),
    ).toEqual({
      outbound: 'eno0',
      addresses: [{ address: '134.199.220.102', gateway: null }],
    })
  })
})
