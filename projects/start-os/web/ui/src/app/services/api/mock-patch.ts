import { DataModel } from 'src/app/services/patch-db/data-model'
import { knownAuthorities } from 'src/app/utils/acme'
import { Mock } from './api.fixures'
const version = require('../../../../../../../../package.json').version

export const mockPatchData: DataModel = {
  ui: {
    registries: {
      'https://registry.start9.com/': 'Start9 Registry',
      'https://community-registry.start9.com/': 'Community Registry',
    },
    startosRegistry: 'https://registry.start9.com/',
    snakeHighScore: 0,
    hiddenUpdates: {},
  },
  serverInfo: {
    id: 'abcdefgh',
    version,
    lastBackup: new Date(new Date().valueOf() - 604800001).toISOString(),
    network: {
      wifi: {
        enabled: true,
        interface: 'wlan0',
        ssids: [],
        selected: null,
        lastRegion: null,
      },
      acme: {
        [knownAuthorities[0].url]: {
          contact: ['mailto:support@start9.com'],
        },
      },
      host: {
        bindings: {
          80: {
            enabled: true,
            net: {
              assignedPort: null,
              assignedSslPort: 443,
            },
            addresses: {
              enabled: [],
              disabled: [],
              guaWan: [],
              available: [
                {
                  ssl: true,
                  public: false,
                  hostname: 'adjective-noun.local',
                  port: 443,
                  metadata: {
                    kind: 'mdns',
                    gateways: ['eth0', 'wlan0'],
                  },
                },
                {
                  ssl: false,
                  public: false,
                  hostname: '10.0.0.1',
                  port: 80,
                  metadata: { kind: 'ipv4', gateway: 'eth0' },
                },
                {
                  ssl: false,
                  public: false,
                  hostname: '10.0.0.2',
                  port: 80,
                  metadata: { kind: 'ipv4', gateway: 'wlan0' },
                },
                {
                  ssl: false,
                  public: false,
                  hostname: 'fe80::cd00:0000:0cde:1257:0000:211e:72cd',
                  port: 80,
                  metadata: { kind: 'ipv6', gateway: 'eth0', scopeId: 2 },
                },
                {
                  ssl: false,
                  public: false,
                  hostname: 'fe80::cd00:0000:0cde:1257:0000:211e:1234',
                  port: 80,
                  metadata: { kind: 'ipv6', gateway: 'wlan0', scopeId: 3 },
                },
                {
                  ssl: true,
                  public: false,
                  hostname: 'my-server.home',
                  port: 443,
                  metadata: {
                    kind: 'private-domain',
                    gateways: ['eth0'],
                  },
                },
                {
                  ssl: false,
                  public: false,
                  hostname:
                    'abc123def456ghi789jkl012mno345pqr678stu901vwx234yz567abc.onion',
                  port: 80,
                  metadata: {
                    kind: 'plugin',
                    packageId: 'tor',
                    removeAction: 'delete-onion-service',
                    overflowActions: ['regenerate-key'],
                    info: null,
                  },
                },
                {
                  ssl: true,
                  public: false,
                  hostname:
                    'abc123def456ghi789jkl012mno345pqr678stu901vwx234yz567abc.onion',
                  port: 443,
                  metadata: {
                    kind: 'plugin',
                    packageId: 'tor',
                    removeAction: 'delete-onion-service',
                    overflowActions: ['regenerate-key'],
                    info: null,
                  },
                },
              ],
            },
            options: {
              preferredExternalPort: 80,
              addSsl: {
                preferredExternalPort: 443,
                alpn: { specified: ['http/1.1', 'h2'] },
                addXForwardedHeaders: false,
                auth: null,
              },
              secure: null,
            },
            interfaces: {},
          },
        },
        bindingRanges: {},
        publicDomains: {},
        privateDomains: {
          'my-server.home': ['eth0'],
        },
        portForwards: [
          {
            src: '203.0.113.45:443',
            dst: '10.0.0.1:443',
            gateway: 'eth0',
            count: 1,
          },
        ],
      },
      gateways: {
        eth0: {
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
            subnets: ['10.0.0.2/24'],
            wanIp: '203.0.113.45',
            ntpServers: [],
            lanIp: ['10.0.2.12'],
            dnsServers: [],
          },
        },
        wlan0: {
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
            name: 'Wireless Connection 1',
            scopeId: 2,
            deviceType: 'wireless',
            subnets: [
              '10.0.90.12/24',
              'fe80::cd00:0000:0cde:1257:0000:211e:72cd/64',
            ],
            wanIp: '203.0.113.45',
            ntpServers: [],
            lanIp: ['10.0.90.12'],
            dnsServers: ['8.8.8.8'],
          },
        },
        wireguard1: {
          name: 'StartTunnel',
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
            name: 'wireguard1',
            scopeId: 2,
            deviceType: 'wireguard',
            subnets: [
              '10.0.90.12/24',
              'fe80::cd00:0000:0cde:1257:0000:211e:72cd/64',
            ],
            wanIp: '203.0.113.45',
            ntpServers: [],
            lanIp: ['10.0.90.12'],
            dnsServers: ['1.1.1.1'],
          },
        },
        wireguard2: {
          name: 'Mullvad VPN',
          secure: null,
          type: 'outbound-only',
          portMap: {
            pcp: { supported: null, at: null },
            natPmp: { supported: null, at: null },
            upnp: { supported: null, at: null },
            pcpHostname: { supported: null, at: null },
          },
          dnsUpdate: { supported: null, at: null },
          ipInfo: {
            name: 'wireguard2',
            scopeId: 4,
            deviceType: 'wireguard',
            subnets: [],
            wanIp: '198.51.100.77',
            ntpServers: [],
            lanIp: [],
            dnsServers: ['10.64.0.1'],
          },
        },
      },
      passthroughs: [],
      defaultOutbound: 'eth0',
      dns: {
        dhcpServers: ['1.1.1.1', '8.8.8.8'],
        staticServers: null,
      },
    },
    unreadNotificationCount: 5,
    packageVersionCompat: '>=0.3.0 <=0.3.6',
    postInitMigrationTodos: {},
    statusInfo: {
      // currentBackup: null,
      updateProgress: null,
      restarting: false,
      shuttingDown: false,
      backupProgress: null,
      restart: null,
    },
    name: 'Random Words',
    hostname: 'random-words',
    pubkey: 'npub1sg6plzptd64u62a878hep2kev88swjh3tw00gjsfl8f237lmu63q0uf63m',
    caFingerprint: '63:2B:11:99:44:40:17:DF:37:FC:C3:DF:0F:3D:15',
    ntpSynced: false,
    smtp: null,
    echoipUrls: ['https://ipconfig.me', 'https://ifconfig.co'],
    zram: true,
    governor: 'performance',
    ram: 8 * 1024 * 1024 * 1024,
    devices: [],
    kiosk: true,
    language: 'en_US',
    keyboard: {
      layout: 'us',
      keymap: 'us',
      model: null,
      variant: null,
      options: [],
    },
    // keyboard: null,
  },
  packageData: {
    lnd: {
      stateInfo: {
        state: 'installed',
        manifest: {
          ...Mock.MockManifestLnd,
          version: '0.11.0:0.0.1',
        },
      },
      s9pk: '/media/startos/data/package-data/archive/installed/asdfasdf.s9pk',
      icon: '/assets/img/service-icons/lnd.png',
      lastBackup: null,
      statusInfo: {
        desired: { main: 'stopped' },
        error: null,
        health: {},
        started: null,
      },
      actions: {
        config: {
          name: 'Config',
          description: 'LND needs configuration before starting',
          warning: null,
          visibility: 'enabled',
          allowedStatuses: 'any',
          hasInput: true,
          group: null,
        },
        connect: {
          name: 'Connect',
          description: 'View LND connection details',
          warning: null,
          visibility: 'enabled',
          allowedStatuses: 'any',
          hasInput: true,
          group: 'Connecting',
        },
      },
      currentDependencies: {
        bitcoind: {
          title: Mock.BitcoinDep.title,
          icon: Mock.BitcoinDep.icon,
          kind: 'running',
          versionRange: '>=26.0.0',
          healthChecks: [],
        },
        'btc-rpc-proxy': {
          title: Mock.ProxyDep.title,
          icon: Mock.ProxyDep.icon,
          kind: 'running',
          versionRange: '<0.4.0',
          healthChecks: [],
        },
      },
      hosts: {},
      storeExposedDependents: [],
      outboundGateway: null,
      registry: 'https://registry.start9.com/',
      developerKey: 'developer-key',
      plugin: { url: null },
      tasks: {
        config: {
          active: true,
          task: {
            packageId: 'lnd',
            actionId: 'config',
            severity: 'critical',
            reason: 'LND needs configuration before starting',
          },
        },
        connect: {
          active: true,
          task: {
            packageId: 'lnd',
            actionId: 'connect',
            severity: 'important',
            reason: 'View LND connection details',
          },
        },
        'bitcoind/config': {
          active: true,
          task: {
            packageId: 'bitcoind',
            actionId: 'config',
            severity: 'critical',
            reason: 'LND likes BTC a certain way',
            input: {
              kind: 'partial',
              accept: [
                {
                  color: '#ffffff',
                  testnet: false,
                },
              ],
              set: {
                color: '#ffffff',
                testnet: false,
              },
            },
          },
        },
        'bitcoind/rpc': {
          active: true,
          task: {
            packageId: 'bitcoind',
            actionId: 'rpc',
            severity: 'important',
            reason: `LND want's its own RPC credentials`,
            input: {
              kind: 'partial',
              accept: [
                {
                  rpcsettings: {
                    rpcuser: 'lnd',
                  },
                },
              ],
              set: {
                rpcsettings: {
                  rpcuser: 'lnd',
                },
              },
            },
          },
        },
      },
    },
    bitcoind: {
      stateInfo: {
        state: 'installed',
        manifest: {
          ...Mock.MockManifestBitcoind,
          version: '0.20.0:0-alpha.1',
        },
      },
      s9pk: '/media/startos/data/package-data/archive/installed/asdfasdf.s9pk',
      icon: '/assets/img/service-icons/bitcoin-core.svg',
      lastBackup: new Date(new Date().valueOf() - 604800001).toISOString(),
      statusInfo: {
        desired: { main: 'stopped' },
        error: null,
        health: {},
        started: null,
      },
      // status: {
      //   main: 'error',
      //   message: 'Bitcoin is erroring out',
      //   debug: 'This is a complete stack trace for bitcoin',
      //   onRebuild: 'start',
      // },
      actions: {
        config: {
          name: 'Set Config',
          description: 'edit bitcoin.conf, <b>soo cool!</b>',
          warning: null,
          visibility: 'enabled',
          allowedStatuses: 'any',
          hasInput: true,
          group: null,
        },
        rpc: {
          name: 'Set RPC',
          description: 'Create RPC Credentials',
          warning: null,
          visibility: 'enabled',
          allowedStatuses: 'any',
          hasInput: true,
          group: null,
        },
        properties: {
          name: 'View Properties',
          description: 'view important information about Bitcoin',
          warning: null,
          visibility: 'hidden',
          allowedStatuses: 'any',
          hasInput: false,
          group: null,
        },
        test: {
          name: 'Do Another Thing',
          description:
            'An example of an action that shows a warning and takes no input',
          warning: 'careful running this action',
          visibility: { disabled: 'This is temporarily disabled' },
          allowedStatuses: 'only-running',
          hasInput: false,
          group: null,
        },
      },
      currentDependencies: {},
      hosts: {
        abcdefg: {
          bindings: {
            80: {
              enabled: true,
              net: {
                assignedPort: 42080,
                assignedSslPort: 42443,
              },
              addresses: {
                enabled: ['203.0.113.45:42443', '[2001:db8:abcd::a3b:2]:1234'],
                disabled: [],
                guaWan: ['[2001:db8:abcd::a3b:2]:1234'],
                available: [
                  {
                    ssl: true,
                    public: false,
                    hostname: 'adjective-noun.local',
                    port: 42443,
                    metadata: {
                      kind: 'mdns',
                      gateways: ['eth0'],
                    },
                  },
                  {
                    ssl: false,
                    public: false,
                    hostname: '10.0.0.1',
                    port: 42080,
                    metadata: { kind: 'ipv4', gateway: 'eth0' },
                  },
                  {
                    ssl: false,
                    public: false,
                    hostname: 'fe80::cd00:0cde:1257:211e:72cd',
                    port: 42080,
                    metadata: { kind: 'ipv6', gateway: 'eth0', scopeId: 2 },
                  },
                  {
                    ssl: true,
                    public: true,
                    hostname: '203.0.113.45',
                    port: 42443,
                    metadata: { kind: 'ipv4', gateway: 'eth0' },
                  },
                  {
                    ssl: true,
                    public: true,
                    hostname: 'bitcoin.example.com',
                    port: 42443,
                    metadata: { kind: 'public-domain', gateway: 'eth0' },
                  },
                  {
                    ssl: false,
                    public: false,
                    hostname: '192.168.10.11',
                    port: 42080,
                    metadata: { kind: 'ipv4', gateway: 'wlan0' },
                  },
                  {
                    ssl: false,
                    public: false,
                    hostname: 'fe80::cd00:0cde:1257:211e:1234',
                    port: 42080,
                    metadata: { kind: 'ipv6', gateway: 'wlan0', scopeId: 3 },
                  },
                  {
                    ssl: true,
                    public: false,
                    hostname: 'my-bitcoin.home',
                    port: 42443,
                    metadata: {
                      kind: 'private-domain',
                      gateways: ['wlan0'],
                    },
                  },
                  {
                    ssl: false,
                    public: false,
                    hostname:
                      'xyz789abc123def456ghi789jkl012mno345pqr678stu901vwx234.onion',
                    port: 42080,
                    metadata: {
                      kind: 'plugin',
                      packageId: 'tor',
                      removeAction: 'delete-onion-service',
                      overflowActions: ['regenerate-key'],
                      info: null,
                    },
                  },
                  {
                    ssl: true,
                    public: false,
                    hostname:
                      'xyz789abc123def456ghi789jkl012mno345pqr678stu901vwx234.onion',
                    port: 42443,
                    metadata: {
                      kind: 'plugin',
                      packageId: 'tor',
                      removeAction: 'delete-onion-service',
                      overflowActions: ['regenerate-key'],
                      info: null,
                    },
                  },
                  {
                    ssl: true,
                    public: true,
                    hostname: '2001:db8:abcd::a3b:2',
                    port: 1234,
                    metadata: { kind: 'ipv6', gateway: 'eth0', scopeId: 0 },
                  },
                ],
              },
              options: {
                preferredExternalPort: 42443,
                addSsl: {
                  preferredExternalPort: 42443,
                  alpn: { specified: ['http/1.1', 'h2'] },
                  addXForwardedHeaders: false,
                  auth: null,
                },
                secure: null,
              },
              interfaces: {
                ui: {
                  id: 'ui',
                  masked: false,
                  name: 'Web UI',
                  description:
                    'A launchable web app for you to interact with your Bitcoin node',
                  type: 'ui',
                  addressInfo: {
                    username: null,
                    hostId: 'abcdefg',
                    internalPort: 80,
                    scheme: 'http',
                    sslScheme: 'https',
                    suffix: '',
                  },
                },
                'admin-ui': {
                  id: 'admin-ui',
                  masked: false,
                  name: 'Admin UI',
                  description: 'An admin panel for managing your Bitcoin node',
                  type: 'ui',
                  addressInfo: {
                    username: null,
                    hostId: 'abcdefg',
                    internalPort: 80,
                    scheme: 'http',
                    sslScheme: 'https',
                    suffix: '/admin',
                  },
                },
              },
            },
          },
          bindingRanges: {
            49152: {
              enabled: true,
              externalStartPort: 49152,
              numberOfPorts: 100,
              // Same per-address model as a single-port binding, IPv4-only and
              // non-SSL, every entry on the external start port (49152). The
              // public WAN IP is seeded into `enabled` (WAN is opt-in); LAN /
              // mDNS / domains are enabled by default.
              addresses: {
                enabled: ['203.0.113.45:49152'],
                disabled: [],
                guaWan: [],
                available: [
                  {
                    ssl: false,
                    public: false,
                    hostname: 'adjective-noun.local',
                    port: 49152,
                    metadata: { kind: 'mdns', gateways: ['eth0', 'wlan0'] },
                  },
                  {
                    ssl: false,
                    public: false,
                    hostname: '10.0.0.1',
                    port: 49152,
                    metadata: { kind: 'ipv4', gateway: 'eth0' },
                  },
                  {
                    ssl: false,
                    public: true,
                    hostname: '203.0.113.45',
                    port: 49152,
                    metadata: { kind: 'ipv4', gateway: 'eth0' },
                  },
                  {
                    ssl: false,
                    public: true,
                    hostname: 'bitcoin.example.com',
                    port: 49152,
                    metadata: { kind: 'public-domain', gateway: 'eth0' },
                  },
                  {
                    ssl: false,
                    public: false,
                    hostname: '192.168.10.11',
                    port: 49152,
                    metadata: { kind: 'ipv4', gateway: 'wlan0' },
                  },
                  {
                    ssl: false,
                    public: false,
                    hostname: 'my-bitcoin.home',
                    port: 49152,
                    metadata: { kind: 'private-domain', gateways: ['wlan0'] },
                  },
                ],
              },
              interface: {
                id: 'zmq',
                name: 'ZMQ',
                description: 'Bitcoin ZMQ notification endpoints',
                scheme: 'tcp',
              },
            },
          },
          publicDomains: {
            'bitcoin.example.com': {
              gateway: 'eth0',
              acme: 'https://acme-v02.api.letsencrypt.org/directory',
            },
          },
          privateDomains: {
            'my-bitcoin.home': ['wlan0'],
          },
          portForwards: [
            {
              src: '203.0.113.45:443',
              dst: '10.0.0.1:443',
              gateway: 'eth0',
              count: 1,
            },
            {
              src: '203.0.113.45:42443',
              dst: '10.0.0.1:42443',
              gateway: 'eth0',
              count: 1,
            },
            {
              src: '203.0.113.45:49152',
              dst: '10.0.0.2:49152',
              gateway: 'eth0',
              count: 100,
            },
          ],
        },
        bcdefgh: {
          bindings: {
            8332: {
              enabled: true,
              net: {
                assignedPort: 48332,
                assignedSslPort: null,
              },
              addresses: {
                enabled: [],
                disabled: [],
                guaWan: [],
                available: [
                  {
                    ssl: false,
                    public: false,
                    hostname: 'adjective-noun.local',
                    port: 48332,
                    metadata: {
                      kind: 'mdns',
                      gateways: ['eth0'],
                    },
                  },
                  {
                    ssl: false,
                    public: false,
                    hostname: '10.0.0.1',
                    port: 48332,
                    metadata: { kind: 'ipv4', gateway: 'eth0' },
                  },
                  {
                    ssl: false,
                    public: false,
                    hostname:
                      'abc123def456ghi789jkl012mno345pqr678stu901vwx234yz567abc.onion',
                    port: 48332,
                    metadata: {
                      kind: 'plugin',
                      packageId: 'tor',
                      removeAction: 'delete-onion-service',
                      overflowActions: ['regenerate-key'],
                      info: null,
                    },
                  },
                ],
              },
              options: {
                addSsl: null,
                preferredExternalPort: 48332,
                secure: { ssl: false },
              },
              interfaces: {
                rpc: {
                  id: 'rpc',
                  masked: true,
                  name: 'RPC',
                  description:
                    'Used by dependent services and client wallets for connecting to your node',
                  type: 'api',
                  addressInfo: {
                    username: 'rpcuser',
                    hostId: 'bcdefgh',
                    internalPort: 8332,
                    scheme: 'http',
                    sslScheme: 'https',
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
        },
        cdefghi: {
          bindings: {
            8333: {
              enabled: true,
              net: {
                assignedPort: 48333,
                assignedSslPort: null,
              },
              addresses: {
                enabled: [],
                disabled: [],
                guaWan: [],
                available: [],
              },
              options: {
                addSsl: null,
                preferredExternalPort: 48333,
                secure: { ssl: false },
              },
              interfaces: {
                p2p: {
                  id: 'p2p',
                  masked: false,
                  name: 'P2P',
                  description:
                    'Used for connecting to other nodes on the Bitcoin network',
                  type: 'p2p',
                  addressInfo: {
                    username: null,
                    hostId: 'cdefghi',
                    internalPort: 8333,
                    scheme: 'bitcoin',
                    sslScheme: null,
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
        },
      },
      storeExposedDependents: [],
      outboundGateway: null,
      registry: 'https://registry.start9.com/',
      developerKey: 'developer-key',
      plugin: { url: null },
      tasks: {
        // 'bitcoind-config': {
        //   task: {
        //     packageId: 'bitcoind',
        //     actionId: 'config',
        //     severity: 'critical',
        //     reason:
        //       'You must run Config before starting Bitcoin for the first time',
        //   },
        //   active: true,
        // },
        'bitcoind-properties': {
          task: {
            packageId: 'bitcoind',
            actionId: 'properties',
            severity: 'important',
            reason: 'Check out all the info about your Bitcoin node',
          },
          active: true,
        },
      },
    },
    tor: {
      stateInfo: {
        state: 'installed',
        manifest: {
          ...Mock.MockManifestTor,
          version: '0.4.8:0',
        },
      },
      s9pk: '/media/startos/data/package-data/archive/installed/tor.s9pk',
      icon: '/assets/img/service-icons/fallback.png',
      lastBackup: null,
      statusInfo: {
        desired: { main: 'running' },
        error: null,
        health: {},
        started: new Date().toISOString(),
      },
      actions: {
        'create-onion-service': {
          name: 'Create Onion Service',
          description: 'Register a new .onion address for a service interface',
          warning: null,
          visibility: 'enabled',
          allowedStatuses: 'only-running',
          hasInput: true,
          group: null,
        },
        'delete-onion-service': {
          name: 'Delete Onion Service',
          description: 'Remove an existing .onion address',
          warning: 'This will permanently remove the .onion address.',
          visibility: 'enabled',
          allowedStatuses: 'only-running',
          hasInput: false,
          group: null,
        },
        'regenerate-key': {
          name: 'Regenerate Key',
          description: 'Generate a new key pair and .onion address',
          warning:
            'This will change the .onion address. Any bookmarks or links to the old address will stop working.',
          visibility: 'enabled',
          allowedStatuses: 'only-running',
          hasInput: false,
          group: null,
        },
      },
      currentDependencies: {},
      hosts: {},
      storeExposedDependents: [],
      outboundGateway: null,
      registry: 'https://registry.start9.com/',
      developerKey: 'developer-key',
      plugin: { url: { tableAction: 'create-onion-service' } },
      tasks: {},
    },
  },
}
