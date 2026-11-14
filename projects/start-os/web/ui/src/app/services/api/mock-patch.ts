import { defaultRegistries, registryUrl, sameUrl } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  DataModel,
  InstalledState,
  PackageDataEntry,
} from 'src/app/services/patch-db/data-model'
import { knownAuthorities } from 'src/app/utils/acme'
import { Mock } from './api.fixures'
const version = require('../../../../../../../../package.json').version

function mockService(
  id: string,
  title: string,
  pkgVersion: string,
  statusInfo: T.StatusInfo,
  sharedVolumes = false,
): PackageDataEntry<InstalledState> {
  return {
    stateInfo: {
      state: 'installed',
      manifest: {
        ...Mock.MockManifestBitcoind,
        id,
        title,
        version: pkgVersion,
        sharedVolumes,
      },
    },
    s9pk: `/media/startos/data/package-data/archive/installed/${id}.s9pk`,
    lastBackup: null,
    statusInfo,
    actions: {},
    currentDependencies: {},
    hosts: {},
    storeExposedDependents: [],
    outboundGateway: null,
    outboundDns: null,
    registry: 'https://registry.start9.com/',
    developerKey: 'developer-key',
    plugin: { url: null },
    tasks: {},
  }
}

function mockStart9Key(
  name: string,
  publicKey: string,
  userIds: string[],
  enabled: boolean,
  fingerprint?: string,
): T.TrustedKey {
  return {
    name,
    origin: 'https://registry.start9.com',
    originRegistry: { name: 'Start9 Registry', icon: null },
    publicKey,
    gpgExports: userIds.length
      ? [
          {
            armored: '',
            fingerprints: [],
            userIds,
            identities: { [publicKey]: userIds },
            keyFingerprints: fingerprint ? { [publicKey]: [fingerprint] } : {},
          },
        ]
      : [],
    contact: [],
    addedAt: '2026-06-02T17:00:00.000Z',
    removable: false,
    marketplace: enabled,
    os: enabled,
  }
}

/** The community registry mocks an unreachable registry. */
export function mockRegistryCache(
  url: string,
  info: T.RegistryInfo,
): DataModel['registryCache'][string] {
  return sameUrl(url, defaultRegistries.community)
    ? {
        info,
        packages: {},
        status: 'error',
        error: `error sending request for url (${registryUrl(url)}): connection timed out`,
      }
    : { info, packages: Mock.RegistryPackages, status: 'connected' }
}

export function mockOsUpdateCache(
  registry: string,
): NonNullable<DataModel['osUpdateCache']> {
  return {
    registry: registryUrl(registry),
    versions: sameUrl(registry, defaultRegistries.community)
      ? {}
      : Mock.RegistryOSUpdate,
    status: 'connected',
  }
}

export const mockPatchData: DataModel = {
  registryCache: {
    [defaultRegistries.start9]: mockRegistryCache(
      defaultRegistries.start9,
      Mock.RegistryInfo,
    ),
    [defaultRegistries.community]: mockRegistryCache(
      defaultRegistries.community,
      { ...Mock.RegistryInfo, name: 'Community Registry' },
    ),
  },
  osUpdateCache: mockOsUpdateCache(defaultRegistries.start9),
  disks: structuredClone(Mock.Disks),
  sharedVolumes: {
    MOCKLOCALVOLUME234567ABCDEFGHIJK: {
      name: 'Shared files',
      source: { kind: 'local' },
      grants: {
        'btc-rpc-proxy': { readonly: false },
        nextcloud: { readonly: false },
      },
      availability: 'available',
      available: 812400000000,
      error: null,
      checkedAt: new Date().toISOString(),
    },
    MOCKDISKVOLUME234567ABCDEFGHIJKL: {
      name: 'Photos library',
      source: Mock.diskSource('/dev/sdc1', 'media/photos'),
      grants: {
        nextcloud: { readonly: true },
        jellyfin: { readonly: true },
      },
      availability: 'available',
      available: 900000000000,
      error: null,
      checkedAt: new Date().toISOString(),
    },
    MOCKMISSINGDRIVE234567ABCDEFGHIJ: {
      name: 'Movies',
      source: {
        ...Mock.diskSource('/dev/sdz1', 'movies'),
        drive: {
          vendor: 'WD',
          model: 'Elements 25A3',
          label: 'Elements',
          capacity: 4000000000000,
        },
      },
      grants: { jellyfin: { readonly: true } },
      availability: 'unavailable',
      available: null,
      error:
        'The volume source is unavailable. Check the source, network and credentials.: mount: special device ' +
        Mock.driveLink('/dev/sdz1') +
        ' does not exist',
      checkedAt: new Date(Date.now() - 3600000).toISOString(),
    },
    MOCKCIFSDOCUMENTS234567ABCDEFGHI: {
      name: 'nas.local',
      source: {
        kind: 'cifs',
        hostname: 'nas.local',
        path: '/documents',
        username: 'family',
      },
      grants: { nextcloud: { readonly: false } },
      availability: 'available',
      available: 3215000000000,
      error: null,
      checkedAt: new Date().toISOString(),
    },
    MOCKCIFSVOLUME234567ABCDEFGHIJKL: {
      name: 'nas.invalid',
      source: {
        kind: 'cifs',
        hostname: 'nas.invalid',
        path: '/music',
        username: 'media',
      },
      grants: { jellyfin: { readonly: true } },
      availability: 'unavailable',
      available: null,
      error:
        'The volume source is unavailable. Check the source, network and credentials.: mount error(113): could not connect to nas.invalid',
      checkedAt: new Date().toISOString(),
    },
  },
  backupTargets: {
    MOCKBACKUPLOCAL234567ABCDEFGHIJK: {
      name: 'Quick restore',
      source: { kind: 'local' },
      availability: 'available',
      available: 812400000000,
      error: null,
      checkedAt: new Date().toISOString(),
      startOs: {
        [Mock.ServerId]: {
          hostname: 'server-name',
          timestamp: new Date(Date.now() - 86400000 * 2).toISOString(),
          version: '0.4.0',
          formatStatus: 'current',
        },
      },
      legacyBackup: false,
    },
    MOCKBACKUPDRIVE234567ABCDEFGHIJK: {
      name: 'Samsung T7',
      source: Mock.diskSource('/dev/sdc1'),
      availability: 'available',
      available: 900000000000,
      error: null,
      checkedAt: new Date().toISOString(),
      startOs: {
        [Mock.ServerId]: {
          hostname: 'server-name',
          timestamp: new Date().toISOString(),
          version: '0.4.0',
          formatStatus: 'legacy',
        },
      },
      legacyBackup: false,
    },
    MOCKBACKUPLEGACY234567ABCDEFGHIJ: {
      name: 'SSK drive',
      source: Mock.diskSource('/dev/sdd1', 'backups'),
      availability: 'available',
      available: 1000000000,
      error: null,
      checkedAt: new Date().toISOString(),
      startOs: {
        [Mock.ServerId]: {
          hostname: 'server-name',
          timestamp: new Date().toISOString(),
          version: '0.4.0',
          formatStatus: 'interrupted',
        },
      },
      legacyBackup: true,
    },
    MOCKBACKUPPASSPORT234567ABCDEFGH: {
      name: 'WD Passport',
      source: Mock.diskSource('/dev/sde1', 'StartOS'),
      availability: 'available',
      available: 61200000000,
      error: null,
      checkedAt: new Date().toISOString(),
      startOs: {
        [Mock.ServerId]: {
          hostname: 'server-name',
          timestamp: new Date(Date.now() - 86400000).toISOString(),
          version: '0.4.0',
          formatStatus: 'current',
        },
      },
      legacyBackup: true,
    },
    MOCKBACKUPNAS234567ABCDEFGHIJKLM: {
      name: 'office-nas.local',
      source: {
        kind: 'cifs',
        hostname: 'office-nas.local',
        path: '/startos-backups',
        username: 'TestUser',
      },
      availability: 'available',
      available: 50000000000,
      error: null,
      checkedAt: new Date().toISOString(),
      startOs: {
        [Mock.ServerId]: {
          hostname: 'server-name',
          timestamp: new Date(Date.now() - 86400000 * 3).toISOString(),
          version: '0.4.0',
          formatStatus: 'current',
        },
        'archive-server': {
          hostname: 'archive-server',
          timestamp: new Date(Date.now() - 86400000 * 90).toISOString(),
          version: '0.4.0',
          formatStatus: 'legacy',
        },
      },
      legacyBackup: false,
    },
    MOCKBACKUPDESKTOP234567ABCDEFGHI: {
      name: 'desktop.invalid',
      source: {
        kind: 'cifs',
        hostname: 'desktop.invalid',
        path: '/Desktop/startos-backups',
        username: 'TestUser',
      },
      availability: 'unavailable',
      available: null,
      error:
        'The volume source is unavailable. Check the source, network and credentials.: mount error(113): could not connect to desktop.invalid',
      checkedAt: new Date(Date.now() - 3600000).toISOString(),
      startOs: {},
      legacyBackup: false,
    },
  },
  ui: {
    snakeHighScore: 0,
    hiddenUpdates: {},
    servicesView: { desktopLayout: 'list', asc: true },
  },
  serverInfo: {
    id: Mock.ServerId,
    registries: {
      'https://registry.start9.com/': 'Start9 Registry',
      'https://community-registry.start9.com/': 'Community Registry',
    },
    osRegistry: 'https://registry.start9.com/',
    registryKeys: {
      'https://registry.start9.com': [
        'MCowBQYDK2VwAyEAqPjOeMD2CB3UEASoICgfJlfrO5ygjOSBCCNRyDBs75A=',
        'MCowBQYDK2VwAyEAgpcI3hOExv1njCt+zJoYBartdyk4SKiqSIy3+9tcpOI=',
        'MCowBQYDK2VwAyEAfeubYt/ozIfZspUZdmu2Fa6sK2psMnHVGxuVolo8u40=',
        'MCowBQYDK2VwAyEAJdDg62o3oHIh4lXECn9gqORe+cCBbH/67MWjMxIMzDg=',
      ],
      'https://community-registry.start9.com': [
        'MCowBQYDK2VwAyEAgpcI3hOExv1njCt+zJoYBartdyk4SKiqSIy3+9tcpOI=',
      ],
    },
    trustedKeys: {
      'MCowBQYDK2VwAyEAqPjOeMD2CB3UEASoICgfJlfrO5ygjOSBCCNRyDBs75A=': {
        name: 'Package publisher',
        origin: 'https://registry.start9.com',
        originRegistry: { name: 'Start9 Registry', icon: null },
        publicKey:
          'MCowBQYDK2VwAyEAqPjOeMD2CB3UEASoICgfJlfrO5ygjOSBCCNRyDBs75A=',
        gpgExports: [],
        contact: [{ email: 'packages@example.com' }],
        addedAt: '2026-10-30T12:00:00.000Z',
        removable: false,
        marketplace: false,
        os: false,
      },
      'MCowBQYDK2VwAyEAgpcI3hOExv1njCt+zJoYBartdyk4SKiqSIy3+9tcpOI=':
        mockStart9Key(
          'drbonez',
          'MCowBQYDK2VwAyEAgpcI3hOExv1njCt+zJoYBartdyk4SKiqSIy3+9tcpOI=',
          [
            'Aiden McClelland <aiden@start9.com>',
            'Aiden McClelland <me@drbonez.dev>',
          ],
          true,
          'b416c0e67b9df3f1718480ae7cffda41ca66056a',
        ),
      'MCowBQYDK2VwAyEAfeubYt/ozIfZspUZdmu2Fa6sK2psMnHVGxuVolo8u40=':
        mockStart9Key(
          'matthill',
          'MCowBQYDK2VwAyEAfeubYt/ozIfZspUZdmu2Fa6sK2psMnHVGxuVolo8u40=',
          ['Matt Hill <matthill@start9.com>'],
          true,
          'a969c8eba8b13613d2568a85ee395832e45a6664',
        ),
      'MCowBQYDK2VwAyEAJdDg62o3oHIh4lXECn9gqORe+cCBbH/67MWjMxIMzDg=':
        mockStart9Key(
          'Start9',
          'MCowBQYDK2VwAyEAJdDg62o3oHIh4lXECn9gqORe+cCBbH/67MWjMxIMzDg=',
          ['Start9 <security@start9.com>'],
          true,
          '5456dbff1b9df905041fa7765259adfc2d63c217',
        ),
      'MCowBQYDK2VwAyEAGaAYxsY7iO5GyQrCA5vcLWYIQyoKImp0du4WWvIzr2Y=':
        mockStart9Key(
          'Start9 Alpha',
          'MCowBQYDK2VwAyEAGaAYxsY7iO5GyQrCA5vcLWYIQyoKImp0du4WWvIzr2Y=',
          [],
          false,
        ),
      'MCowBQYDK2VwAyEA4P+LwrLkZfhgcM3MNz6g8ihBvNnIfGSu+ojr+lFoDxU=':
        mockStart9Key(
          'Community builds',
          'MCowBQYDK2VwAyEA4P+LwrLkZfhgcM3MNz6g8ihBvNnIfGSu+ojr+lFoDxU=',
          [],
          false,
        ),
    },
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
              lanEnabled: [],
              available: [
                {
                  ssl: true,
                  public: false,
                  hostname: 'server-name.local',
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
                upstreamCertValidation: null,
                preferredExternalPort: 443,
                alpn: ['http/1.1', 'h2'],
              },
              secure: null,
              proxy: null,
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
            local: true,
          },
        ],
      },
      gateways: {
        eth0: {
          wireguardEndpointDomains: [],
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
          dns: {
            staticServers: null,
            servers: ['1.1.1.1:53', '[2606:4700:4700::1111]:53'],
            source: 'public-fallback',
          },
          ipInfo: {
            name: 'Wired Connection 1',
            scopeId: 1,
            deviceType: 'ethernet',
            subnets: ['10.0.0.2/24'],
            wanIp: '203.0.113.45',
            ntpServers: [],
            lanIp: ['10.0.2.12'],
            defaultRouteV4: 100,
            defaultRouteV6: null,
            dnsServers: [],
          },
        },
        wlan0: {
          wireguardEndpointDomains: [],
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
          dns: {
            staticServers: null,
            servers: ['8.8.8.8:53'],
            source: 'gateway',
          },
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
            defaultRouteV4: 600,
            defaultRouteV6: 600,
            dnsServers: ['8.8.8.8'],
          },
        },
        wireguard1: {
          wireguardEndpointDomains: ['tunnel.example.com'],
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
          dns: {
            staticServers: ['9.9.9.9'],
            servers: ['9.9.9.9:53'],
            source: 'static',
          },
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
            defaultRouteV4: null,
            defaultRouteV6: null,
            dnsServers: ['1.1.1.1'],
          },
        },
        wireguard2: {
          wireguardEndpointDomains: [],
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
          dns: {
            staticServers: null,
            servers: ['10.64.0.1:53'],
            source: 'gateway',
          },
          ipInfo: {
            name: 'wireguard2',
            scopeId: 4,
            deviceType: 'wireguard',
            subnets: [],
            wanIp: '198.51.100.77',
            ntpServers: [],
            lanIp: [],
            defaultRouteV4: null,
            defaultRouteV6: null,
            dnsServers: ['10.64.0.1'],
          },
        },
        wireguard3: {
          wireguardEndpointDomains: ['offline.example.com'],
          name: 'Offline VPN',
          secure: null,
          type: 'outbound-only',
          ipInfo: null,
          dns: {
            staticServers: ['9.9.9.9'],
            servers: ['9.9.9.9:53'],
            source: 'static',
          },
          portMap: {
            pcp: { supported: null, at: null },
            natPmp: { supported: null, at: null },
            upnp: { supported: null, at: null },
            pcpHostname: { supported: null, at: null },
          },
          dnsUpdate: { supported: null, at: null },
        },
      },
      passthroughs: [],
      defaultOutbound: 'eth0',
      outboundDns: null,
    },
    unreadNotificationCount: 5,
    packageVersionCompat: '>=0.3.0 <=0.3.6',
    postInitMigrationTodos: {},
    latestMigrationRevision: 0,
    statusInfo: {
      updateProgress: null,
      backupConversions: {},
      restarting: false,
      shuttingDown: false,
      backupProgress: null,
      restart: null,
    },
    hostname: 'random-words',
    pubkey: 'npub1sg6plzptd64u62a878hep2kev88swjh3tw00gjsfl8f237lmu63q0uf63m',
    caFingerprint: '63:2B:11:99:44:40:17:DF:37:FC:C3:DF:0F:3D:15',
    ntpSynced: false,
    smtp: null,
    nut: { enabled: false, settings: null },
    echoipUrls: ['https://ipconfig.me', 'https://ifconfig.co'],
    zram: false,
    governor: 'performance',
    epp: 'balance_power',
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
    'btc-rpc-proxy': structuredClone(Mock.LocalPkgs['btc-rpc-proxy']!),
    lnd: {
      stateInfo: {
        state: 'installed',
        manifest: {
          ...Mock.MockManifestLnd,
          version: '0.11.0:0.0.1',
        },
      },
      s9pk: '/media/startos/data/package-data/archive/installed/lnd.s9pk',
      lastBackup: null,
      statusInfo: {
        desired: { main: 'stopped' },
        error: null,
        health: {},
        started: null,
      },
      actions: {
        config: {
          access: null,
          name: 'Config',
          description: 'LND needs configuration before starting',
          warning: null,
          visibility: 'enabled',
          allowedStatuses: 'any',
          hasInput: true,
          group: null,
        },
        connect: {
          access: null,
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
          kind: 'running',
          versionRange: '>=26.0.0',
          healthChecks: [],
        },
        'btc-rpc-proxy': {
          title: Mock.ProxyDep.title,
          kind: 'running',
          versionRange: '<0.4.0',
          healthChecks: [],
        },
      },
      hosts: {},
      storeExposedDependents: [],
      outboundGateway: null,
      outboundDns: null,
      registry: 'https://registry.start9.com/',
      developerKey: 'developer-key',
      plugin: { url: null },
      tasks: {
        config: {
          active: true,
          task: {
            when: null,
            input: null,
            packageId: 'lnd',
            actionId: 'config',
            severity: 'critical',
            reason: 'LND needs configuration before starting',
          },
        },
        connect: {
          active: true,
          task: {
            when: null,
            input: null,
            packageId: 'lnd',
            actionId: 'connect',
            severity: 'important',
            reason: 'View LND connection details',
          },
        },
        'bitcoind/config': {
          active: true,
          task: {
            when: null,
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
            when: null,
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
      s9pk: '/media/startos/data/package-data/archive/installed/bitcoind.s9pk',
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
          access: null,
          name: 'Set Config',
          description: 'edit bitcoin.conf, <b>soo cool!</b>',
          warning: null,
          visibility: 'enabled',
          allowedStatuses: 'any',
          hasInput: true,
          group: null,
        },
        rpc: {
          access: null,
          name: 'Set RPC',
          description: 'Create RPC Credentials',
          warning: null,
          visibility: 'enabled',
          allowedStatuses: 'any',
          hasInput: true,
          group: null,
        },
        properties: {
          access: null,
          name: 'View Properties',
          description: 'view important information about Bitcoin',
          warning: null,
          visibility: 'hidden',
          allowedStatuses: 'any',
          hasInput: false,
          group: null,
        },
        'big-qr': {
          access: null,
          name: 'Show Large QR',
          description:
            'Returns a QR payload too long for correction level M, the size a hardware-wallet code reaches',
          warning: null,
          visibility: 'enabled',
          allowedStatuses: 'any',
          hasInput: false,
          group: null,
        },
        'unencodable-qr': {
          access: null,
          name: 'Show Unencodable QR',
          description:
            'Returns a QR payload past correction level L, which no QR code can hold',
          warning: null,
          visibility: 'enabled',
          allowedStatuses: 'any',
          hasInput: false,
          group: null,
        },
        multiline: {
          access: null,
          name: 'Show Report',
          description:
            'Returns a multi-line value with a copy button and a download',
          warning: null,
          visibility: 'enabled',
          allowedStatuses: 'any',
          hasInput: false,
          group: null,
        },
        'multiline-secret': {
          access: null,
          name: 'Show Device Config',
          description:
            'Returns a masked multi-line value that can also be shown as a QR code',
          warning: null,
          visibility: 'enabled',
          allowedStatuses: 'any',
          hasInput: false,
          group: null,
        },
        'file-upload': {
          access: null,
          name: 'Import files',
          description: 'Upload files before running the action',
          warning: null,
          visibility: 'enabled',
          allowedStatuses: 'any',
          hasInput: true,
          group: null,
        },
        'file-result': {
          access: null,
          name: 'Download report',
          description: 'Return a prepared volume file',
          warning: null,
          visibility: 'enabled',
          allowedStatuses: 'any',
          hasInput: false,
          group: null,
        },
        'multiline-group': {
          access: null,
          name: 'Show Mixed Group',
          description:
            'Returns a group whose members mix single-line and multi-line values',
          warning: null,
          visibility: 'enabled',
          allowedStatuses: 'any',
          hasInput: false,
          group: null,
        },
        test: {
          access: null,
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
                lanEnabled: [],
                available: [
                  {
                    ssl: true,
                    public: false,
                    hostname: 'server-name.local',
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
                  upstreamCertValidation: null,
                  preferredExternalPort: 42443,
                  alpn: ['http/1.1', 'h2'],
                },
                secure: null,
                proxy: null,
              },
              interfaces: {
                ui: {
                  id: 'ui',
                  masked: false,
                  name: 'Web UI',
                  description:
                    'A launchable web app for you to interact with your Bitcoin node',
                  type: 'ui',
                  preferredLauncherAddress: 'https://my-bitcoin.home:42443',
                  addressInfo: {
                    username: null,
                    hostId: 'abcdefg',
                    internalPort: 80,
                    scheme: 'http',
                    sslScheme: 'https',
                    suffix: '',
                  },
                },
                connect: {
                  preferredLauncherAddress: null,
                  id: 'connect',
                  masked: true,
                  name: 'gRPC Connect',
                  description:
                    'A connection URI carrying a certificate — longer than any QR code holds',
                  type: 'api',
                  addressInfo: {
                    username: null,
                    hostId: 'abcdefg',
                    internalPort: 80,
                    scheme: 'lndconnect',
                    sslScheme: 'lndconnect',
                    suffix: Mock.UNENCODABLE_QUERY,
                  },
                },
                'admin-ui': {
                  preferredLauncherAddress: null,
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
                lanEnabled: [],
                available: [
                  {
                    ssl: false,
                    public: false,
                    hostname: 'server-name.local',
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
              local: true,
            },
            {
              src: '203.0.113.45:42443',
              dst: '10.0.0.1:42443',
              gateway: 'eth0',
              count: 1,
              local: true,
            },
            {
              src: '203.0.113.45:49152',
              dst: '10.0.0.2:49152',
              gateway: 'eth0',
              count: 100,
              local: true,
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
                lanEnabled: [],
                available: [
                  {
                    ssl: false,
                    public: false,
                    hostname: 'server-name.local',
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
                proxy: null,
              },
              interfaces: {
                rpc: {
                  preferredLauncherAddress: null,
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
                lanEnabled: [],
                available: [],
              },
              options: {
                addSsl: null,
                preferredExternalPort: 48333,
                secure: { ssl: false },
                proxy: null,
              },
              interfaces: {
                p2p: {
                  preferredLauncherAddress: null,
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
      outboundDns: null,
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
            when: null,
            input: null,
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
      lastBackup: null,
      statusInfo: {
        desired: { main: 'running' },
        error: null,
        health: {},
        started: new Date().toISOString(),
      },
      actions: {
        'create-onion-service': {
          access: null,
          name: 'Create Onion Service',
          description: 'Register a new .onion address for a service interface',
          warning: null,
          visibility: 'enabled',
          allowedStatuses: 'only-running',
          hasInput: true,
          group: null,
        },
        'delete-onion-service': {
          access: null,
          name: 'Delete Onion Service',
          description: 'Remove an existing .onion address',
          warning: 'This will permanently remove the .onion address.',
          visibility: 'enabled',
          allowedStatuses: 'only-running',
          hasInput: false,
          group: null,
        },
        'regenerate-key': {
          access: null,
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
      outboundDns: null,
      registry: 'https://registry.start9.com/',
      developerKey: 'developer-key',
      plugin: { url: { tableAction: 'create-onion-service' } },
      tasks: {},
    },
    nextcloud: mockService(
      'nextcloud',
      'Nextcloud',
      '30.0.2:0',
      {
        desired: { main: 'running' },
        error: null,
        health: {
          web: { name: 'Web Interface', result: 'success', message: null },
        },
        started: new Date(Date.now() - 86400000).toISOString(),
      },
      true,
    ),
    vaultwarden: mockService('vaultwarden', 'Vaultwarden', '1.32.7:0', {
      desired: { main: 'running' },
      error: null,
      health: {
        web: {
          name: 'Web Vault',
          result: 'loading',
          waiting: false,
          message: 'Starting up',
        },
      },
      started: new Date(Date.now() - 30000).toISOString(),
    }),
    jellyfin: mockService(
      'jellyfin',
      'Jellyfin',
      '10.10.3:0',
      {
        desired: { main: 'stopped' },
        error: null,
        health: {},
        started: null,
      },
      true,
    ),
    electrs: mockService('electrs', 'Electrs', '0.10.5:0', {
      desired: { main: 'running' },
      error: null,
      health: {
        rpc: {
          name: 'RPC',
          result: 'failure',
          message: 'Cannot reach bitcoind',
        },
      },
      started: new Date(Date.now() - 3600000).toISOString(),
    }),
    'home-assistant': mockService(
      'home-assistant',
      'Home Assistant',
      '2024.11.3:0',
      {
        desired: { main: 'running' },
        error: {
          details: 'Container exited unexpectedly',
          debug: 'exit code 137',
          info: null,
        },
        health: {},
        started: null,
      },
      true,
    ),
  },
}
