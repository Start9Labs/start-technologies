import { T } from '@start9labs/start-core'

export const BITTORRENT_INTERFACE: T.ServiceInterface = {
  id: 'bittorrent',
  name: 'BitTorrent peers',
  description: 'Incoming BitTorrent connections for sharing stored software',
  masked: false,
  addressInfo: {
    username: null,
    hostId: 'admin',
    internalPort: 6881,
    scheme: 'tcp',
    sslScheme: null,
    suffix: '',
  },
  type: 'p2p',
  preferredLauncherAddress: null,
}

export function mockBitTorrentPreview(): T.BindingPreview {
  const { options, addresses } = mockBitTorrentBinding()
  return {
    options,
    addresses: {
      ...addresses,
      available: addresses.available.map(address => ({
        ...address,
        port: null,
      })),
    },
  }
}

export function mockBitTorrentBinding(): T.BindInfo {
  const port = 50000
  return {
    enabled: true,
    options: {
      transport: ['tcp'],
      preferredExternalPort: 0,
      addSsl: null,
      secure: { ssl: false },
      proxy: { type: 'tcp' },
      defaultOff: true,
    },
    net: { assignedPort: port, assignedSslPort: null },
    interfaces: { [BITTORRENT_INTERFACE.id]: BITTORRENT_INTERFACE },
    addresses: {
      enabled: [],
      disabled: [],
      enabledNames: [],
      defaultOff: true,
      selected: [],
      guaWan: [],
      lanEnabled: [],
      available: [
        ...['eth0', 'wireguard1'].map<T.HostnameInfo>(gateway => ({
          hostname: '203.0.113.45',
          port,
          ssl: false,
          public: true,
          metadata: { kind: 'ipv4', gateway },
        })),
        {
          hostname: '192.168.1.2',
          port,
          ssl: false,
          public: false,
          metadata: { kind: 'ipv4', gateway: 'eth0' },
        },
        {
          hostname: 'fd00::1234',
          port,
          ssl: false,
          public: false,
          metadata: { kind: 'ipv6', gateway: 'eth0', scopeId: 0 },
        },
        {
          hostname: '2001:db8::1234',
          port,
          ssl: false,
          public: false,
          metadata: { kind: 'ipv6', gateway: 'eth0', scopeId: 0 },
        },
      ],
    },
  }
}
