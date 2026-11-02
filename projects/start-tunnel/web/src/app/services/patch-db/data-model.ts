import { T } from '@start9labs/start-core'

export type TunnelData = Pick<
  T.Tunnel.TunnelDatabase,
  | 'wg'
  | 'portForwards'
  | 'pinholes6'
  | 'gateways'
  | 'dnsRecords'
  | 'httpRedirects'
>

export const mockTunnelData: TunnelData = {
  wg: {
    port: 51820,
    key: '',
    subnets: {
      '10.59.0.0/24': {
        name: 'Family',
        clients: {
          '10.59.0.2': {
            name: 'Server One',
            key: '',
            psk: '',
            kind: 'server',
            allowDnsInjection: true,
            allowAutoPortForward: true,
            wanIp: null,
          },
          '10.59.0.3': {
            name: 'Phone',
            key: '',
            psk: '',
            kind: 'client',
            allowDnsInjection: false,
            allowAutoPortForward: false,
            wanIp: null,
          },
          '10.59.0.4': {
            name: 'Laptop',
            key: '',
            psk: '',
            kind: 'client',
            allowDnsInjection: false,
            allowAutoPortForward: false,
            wanIp: null,
          },
          '10.59.0.5': {
            name: 'Tablet',
            key: '',
            psk: '',
            kind: 'client',
            allowDnsInjection: false,
            allowAutoPortForward: false,
            wanIp: null,
          },
        },
        dns: { type: 'default' },
        wanIp: '203.0.113.42',
        ipv6: '2001:db8:59::/64',
      },
      '10.59.1.0/24': {
        name: 'Friends',
        clients: {
          '10.59.1.2': {
            name: "Alex's phone",
            key: '',
            psk: '',
            kind: 'client',
            allowDnsInjection: false,
            allowAutoPortForward: false,
            wanIp: null,
          },
          '10.59.1.3': {
            name: "Sam's laptop",
            key: '',
            psk: '',
            kind: 'client',
            allowDnsInjection: false,
            allowAutoPortForward: false,
            wanIp: null,
          },
        },
        dns: { type: 'custom', servers: ['9.9.9.9', '149.112.112.112'] },
        wanIp: null,
        ipv6: null,
      },
    },
  },
  portForwards: {
    '203.0.113.42:443': {
      kind: 'sni',
      routes: {
        'cloud.example.com': {
          target: '10.59.0.2:443',
          label: 'Nextcloud',
          enabled: true,
          auto: true,
        },
        'photos.example.com': {
          target: '10.59.0.2:443',
          label: 'Immich',
          enabled: true,
          auto: true,
        },
        'blog.example.com': {
          target: '10.59.0.4:443',
          label: 'Blog',
          enabled: true,
          auto: false,
        },
      },
      // Hostname-less fallback: catches bare-IP / non-matching-SNI traffic on
      // this shared port.
      fallback: {
        target: '10.59.0.2:443',
        label: 'Server One (fallback)',
        enabled: true,
        auto: false,
      },
    },
    '203.0.113.42:8333': {
      kind: 'dnat',
      target: '10.59.0.2:8333',
      label: 'Bitcoin',
      enabled: true,
      count: 1,
      auto: true,
    },
    '203.0.113.42:25565': {
      kind: 'dnat',
      target: '10.59.0.4:25565',
      label: 'Minecraft',
      enabled: true,
      count: 1,
      auto: false,
    },
  },
  pinholes6: {
    // Client 10.59.0.2's GUA on the subnet's 2001:db8:59::/64 prefix.
    '[2001:db8:59::a3b:2]:8443': {
      label: 'Nextcloud',
      enabled: true,
      count: 1,
      internalPort: null,
      auto: false,
    },
    '[2001:db8:59::a3b:2]:443': {
      label: 'PCP',
      enabled: true,
      count: 1,
      internalPort: null,
      auto: true,
    },
  },
  dnsRecords: [
    {
      name: 'cloud.example.com',
      type: 'A',
      value: '10.59.0.2',
      ttl: 300,
      source: '10.59.0.2',
    },
    {
      name: 'photos.example.com',
      type: 'A',
      value: '10.59.0.2',
      ttl: 300,
      source: '10.59.0.2',
    },
    {
      name: 'server-one.local',
      type: 'A',
      value: '10.59.0.2',
      ttl: 300,
      source: '10.59.0.2',
    },
    {
      name: 'blog.home.arpa',
      type: 'A',
      value: '10.59.0.4',
      ttl: 3600,
      source: null,
    },
  ],
  gateways: {
    eth0: {
      wireguardEndpointDomains: [],
      dns: {
        staticServers: null,
        servers: ['1.1.1.1:53', '[2606:4700:4700::1111]:53'],
        source: 'public-fallback',
      },
      name: null,
      secure: null,
      type: 'inbound-outbound',
      portMap: {
        pcp: { supported: null, at: null },
        natPmp: { supported: null, at: null },
        upnp: { supported: null, at: null },
        pcpHostname: { supported: null, at: null },
      },
      dnsUpdate: { supported: null, at: null },
      ipInfo: {
        name: 'Wired Connection 1',
        scopeId: 1,
        deviceType: 'ethernet',
        subnets: ['203.0.113.42/24'],
        wanIp: null,
        ntpServers: [],
        lanIp: ['10.59.0.1'],
        dnsServers: [],
        defaultRouteV4: 100,
        defaultRouteV6: null,
      },
    },
  },
  httpRedirects: {
    disabled: [],
  },
}
