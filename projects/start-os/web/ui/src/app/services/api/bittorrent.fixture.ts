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

export function mockBitTorrentBinding(): T.BindInfo {
  const port = BITTORRENT_INTERFACE.addressInfo.internalPort
  return {
    enabled: false,
    options: {
      preferredExternalPort: port,
      addSsl: null,
      secure: { ssl: false },
      proxy: { type: 'tcp' },
      publicOnly: true,
    },
    net: { assignedPort: port, assignedSslPort: null },
    interfaces: { [BITTORRENT_INTERFACE.id]: BITTORRENT_INTERFACE },
    addresses: {
      enabled: [],
      disabled: [],
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
          hostname: '2001:db8::1234',
          port,
          ssl: false,
          public: true,
          metadata: { kind: 'ipv6', gateway: 'eth0', scopeId: 0 },
        },
      ],
    },
  }
}
