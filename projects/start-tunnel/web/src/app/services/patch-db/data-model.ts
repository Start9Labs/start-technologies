import { T } from '@start9labs/start-core'

export type TunnelData = Pick<
  T.Tunnel.TunnelDatabase,
  'wg' | 'portForwards' | 'pinholes6' | 'gateways' | 'dnsRecords'
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
            name: 'Start9 Server',
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
        },
        dns: { type: 'default' },
        wanIp: '69.1.1.42',
        ipv6: '2001:db8:59::/64',
      },
    },
  },
  portForwards: {
    '69.1.1.42:443': {
      kind: 'dnat',
      target: '10.59.0.2:443',
      label: 'HTTPS',
      enabled: true,
      count: 1,
      auto: false,
    },
    '69.1.1.42:3000': {
      kind: 'dnat',
      target: '10.59.0.2:3000',
      label: 'Grafana',
      enabled: true,
      count: 1,
      auto: true,
    },
    '69.1.1.42:8443': {
      kind: 'sni',
      routes: {
        'app.example.com': {
          target: '10.59.0.2:443',
          label: 'App',
          enabled: true,
          auto: true,
        },
        'blog.example.com': {
          target: '10.59.0.3:443',
          label: 'Blog',
          enabled: true,
          auto: false,
        },
      },
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
      name: 'home.example.com',
      type: 'A',
      value: '10.59.0.2',
      ttl: 300,
      source: '10.59.0.2',
    },
  ],
  gateways: {
    eth0: {
      name: null,
      secure: null,
      type: 'inbound-outbound',
      ipInfo: {
        name: 'Wired Connection 1',
        scopeId: 1,
        deviceType: 'ethernet',
        subnets: ['69.1.1.42/24'],
        wanIp: null,
        ntpServers: [],
        lanIp: ['10.59.0.1'],
        dnsServers: [],
      },
    },
  },
}
