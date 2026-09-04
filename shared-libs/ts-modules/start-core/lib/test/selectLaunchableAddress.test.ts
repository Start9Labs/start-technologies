import { AddressInfo, BindInfo, Host, HostnameInfo } from '../osBindings'
import {
  LauncherAccess,
  selectLaunchableAddress,
} from '../util/selectLaunchableAddress'

const addressInfo: AddressInfo = {
  username: null,
  hostId: 'ui',
  internalPort: 80,
  scheme: 'http',
  sslScheme: 'https',
  suffix: '/app?from=startos',
}

function hostname(
  hostname: string,
  options: {
    ssl?: boolean
    public?: boolean
    port?: number | null
    metadata?: HostnameInfo['metadata']
  } = {},
): HostnameInfo {
  return {
    ssl: options.ssl ?? true,
    public: options.public ?? false,
    hostname,
    port: options.port === undefined ? 443 : options.port,
    metadata: options.metadata ?? {
      kind: 'private-domain',
      gateways: ['eth0'],
    },
  }
}

function hostWith(
  available: HostnameInfo[],
  options: { disabled?: Array<[string, number]> } = {},
) {
  const binding = {
    enabled: [],
    disabled: options.disabled ?? [],
    guaWan: [],
    available,
  }
  const bindInfo = {
    enabled: true,
    options: {
      preferredExternalPort: 80,
      addSsl: null,
      secure: null,
    },
    net: { assignedPort: 80, assignedSslPort: 443 },
    addresses: binding,
    interfaces: {},
  } as BindInfo
  const host = {
    bindings: { 80: bindInfo },
    bindingRanges: {},
    publicDomains: {},
    privateDomains: {},
    portForwards: [],
  } as Host
  return host
}

function select(
  preferred: string | null | undefined,
  accessType: LauncherAccess['accessType'],
  available: HostnameInfo[],
  options: {
    disabled?: Array<[string, number]>
    hostname?: string
  } = {},
): string {
  const ui = {
    id: 'ui',
    name: 'Web UI',
    description: '',
    masked: false,
    addressInfo,
    type: 'ui' as const,
    preferredLauncherAddress: preferred,
  }
  return selectLaunchableAddress(ui, hostWith(available, options), {
    accessType,
    hostname: options.hostname ?? '',
  })
}

const privateDomain = hostname('pad.home')
const publicDomain = hostname('pad.example.com', {
  public: true,
  metadata: { kind: 'public-domain', gateway: 'eth0' },
})
const onion = hostname('pad123.onion', {
  metadata: {
    kind: 'plugin',
    packageId: 'tor',
    removeAction: null,
    overflowActions: [],
    info: null,
  },
})

describe('selectLaunchableAddress', () => {
  const localIp = hostname('192.168.1.10', {
    ssl: false,
    port: 80,
    metadata: { kind: 'ipv4', gateway: 'eth0' },
  })
  const publicIp = hostname('203.0.113.10', {
    ssl: false,
    public: true,
    port: null,
    metadata: { kind: 'ipv4', gateway: 'eth0' },
  })
  const ipv6 = hostname('2001:db8::10', {
    ssl: false,
    port: 80,
    metadata: { kind: 'ipv6', gateway: 'eth0', scopeId: 0 },
  })
  const localhost = hostname('localhost', {
    ssl: false,
    port: 80,
    metadata: { kind: 'ipv4', gateway: 'lo' },
  })
  const mdns = hostname('pad.local', {
    ssl: false,
    port: 80,
    metadata: { kind: 'mdns', gateways: ['eth0'] },
  })
  const candidates = [
    localIp,
    publicIp,
    ipv6,
    localhost,
    mdns,
    publicDomain,
    onion,
  ]

  test.each([
    ['ipv4', '192.168.1.10', 'http://192.168.1.10/app?from=startos'],
    ['ipv6', '2001:db8::10', 'http://[2001:db8::10]/app?from=startos'],
    ['localhost', '', 'http://localhost/app?from=startos'],
    ['mdns', '', 'http://pad.local/app?from=startos'],
    ['domain', '', 'https://pad.example.com/app?from=startos'],
    ['tor', '', 'https://pad123.onion/app?from=startos'],
    ['wan-ipv4', '', 'http://203.0.113.10/app?from=startos'],
  ] as const)(
    'preserves %s session selection without a nomination',
    (accessType, sessionHostname, expected) => {
      expect(
        select(undefined, accessType, candidates, {
          hostname: sessionHostname,
        }),
      ).toBe(expected)
    },
  )
  test.each<LauncherAccess['accessType']>([
    'ipv4',
    'ipv6',
    'localhost',
    'mdns',
    'domain',
    'tor',
    'wan-ipv4',
  ])('selects a public nomination from a %s session', accessType => {
    expect(select('https://PAD.EXAMPLE.COM/', accessType, [publicDomain])).toBe(
      'https://pad.example.com/app?from=startos',
    )
  })

  test.each<LauncherAccess['accessType']>([
    'ipv4',
    'ipv6',
    'localhost',
    'mdns',
    'domain',
    'wan-ipv4',
  ])('selects a private nomination from a %s session', accessType => {
    expect(select('https://pad.home', accessType, [privateDomain])).toBe(
      'https://pad.home/app?from=startos',
    )
  })

  test.each([
    ['tor', 'https://pad123.onion', onion],
    ['tor', 'https://pad.home', privateDomain],
    ['domain', 'https://pad123.onion', onion],
    [
      'tor',
      'https://plugin.example',
      hostname('plugin.example', {
        public: true,
        metadata: {
          kind: 'plugin',
          packageId: 'other-plugin',
          removeAction: null,
          overflowActions: [],
          info: null,
        },
      }),
    ],
  ] as const)(
    'returns the transport-compatible nomination for %s to %s',
    (accessType, preferred, candidate) => {
      expect(select(preferred, accessType, [candidate])).toBe(
        candidate === onion && accessType === 'tor'
          ? 'https://pad123.onion/app?from=startos'
          : candidate === privateDomain
            ? 'https://pad.home/app?from=startos'
            : '',
      )
    },
  )

  test('matches the nominated SSL leg by origin', () => {
    const plain = hostname('pad.example.com', {
      ssl: false,
      public: true,
      port: 80,
      metadata: { kind: 'public-domain', gateway: 'eth0' },
    })
    expect(
      select('https://pad.example.com', 'domain', [plain, publicDomain]),
    ).toBe('https://pad.example.com/app?from=startos')
  })

  test.each([
    [
      'missing',
      undefined,
      [publicDomain],
      {},
      'https://pad.example.com/app?from=startos',
    ],
    [
      'null',
      null,
      [publicDomain],
      {},
      'https://pad.example.com/app?from=startos',
    ],
    [
      'blank',
      '',
      [publicDomain],
      {},
      'https://pad.example.com/app?from=startos',
    ],
    [
      'whitespace',
      ' \n\t ',
      [publicDomain],
      {},
      'https://pad.example.com/app?from=startos',
    ],
    [
      'invalid',
      'not a url',
      [publicDomain],
      {},
      'https://pad.example.com/app?from=startos',
    ],
    [
      'vanished',
      'https://gone.example',
      [publicDomain],
      {},
      'https://pad.example.com/app?from=startos',
    ],
    [
      'wrong port',
      'https://pad.example.com:8443',
      [publicDomain],
      {},
      'https://pad.example.com/app?from=startos',
    ],
    [
      'disabled',
      'https://pad.home',
      [privateDomain],
      { disabled: [['pad.home', 443]] },
      '',
    ],
    [
      'loopback',
      'https://localhost',
      [hostname('localhost', { metadata: { kind: 'ipv4', gateway: 'lo' } })],
      {},
      '',
    ],
    [
      'bridge',
      'https://10.0.3.1',
      [
        hostname('10.0.3.1', {
          metadata: { kind: 'ipv4', gateway: 'lxcbr0' },
        }),
      ],
      {},
      '',
    ],
  ] as const)(
    'falls back correctly when the nomination is %s',
    (_name, preferred, values, options, expected) => {
      expect(
        select(preferred, 'domain', [...values], {
          disabled:
            'disabled' in options
              ? options.disabled.map(([host, port]) => [host, port])
              : undefined,
        }),
      ).toBe(expected)
    },
  )
})
