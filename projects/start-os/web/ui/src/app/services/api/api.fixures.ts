import { GetPackagesRes } from '@start9labs/marketplace'
import { ISB, IST, T } from '@start9labs/start-core'
import {
  InstalledState,
  PackageDataEntry,
} from 'src/app/services/patch-db/data-model'
import { configBuilderToSpec } from 'src/app/utils/configBuilderToSpec'
import { BTC_ICON, LND_ICON, PROXY_ICON, REGISTRY_ICON } from './api-icons'
import { ActionRes } from './api.types'

import markdown from './md-sample.md'

const mockMerkleArchiveCommitment: T.MerkleArchiveCommitment = {
  rootSighash: 'fakehash',
  rootMaxsize: 0,
}

const mockDescription = {
  short: 'Lorem ipsum dolor sit amet',
  long: 'Lorem ipsum dolor sit amet, <p>consectetur adipiscing elit</p>, sed do eiusmod <i>tempor</i> incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.',
}

export namespace Mock {
  export const ServerUpdated: T.ServerStatus = {
    backupProgress: null,
    updateProgress: null,
    restarting: false,
    shuttingDown: false,
    restart: null,
  }

  export const RegistryOSUpdate: T.OsVersionInfoMap = {
    '0.4.1': {
      headline: 'v0.4.1',
      releaseNotes: 'Testing some release notes',
      sourceVersion: '>=0.3.5:0 <=0.4.0-alpha.17:0',
      authorized: ['G24CSA5HNYEPIXJNMK7ZM4KD5SX5N6X4'],
      iso: {},
      squashfs: {
        aarch64: {
          publishedAt: '2025-03-21T23:55:29.583006392Z',
          urls: [
            'https://alpha-registry-x.start9.com/startos/v0.4.0-alpha.17/startos-0.4.0-alpha.17-b8ff331~dev_aarch64.squashfs',
          ],
          commitment: {
            hash: 'OUnANnZePtf7rSbj38JESl+iJAV0z0aiZ4opCiwpGbo=',
            size: 1331900416,
          },
          signatures: {
            '-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAqPjOeMD2CB3UEASoICgfJlfrO5ygjOSBCCNRyDBs75A=\n-----END PUBLIC KEY-----\n':
              '-----BEGIN SIGNATURE-----\nMEkwBQYDK2VwBEAMbmYB7TQknW7AdeasgNsqhWBVuRkYzrgZs7I1yHPHmLtMcNRo\nErEz7QKhKTgdzGn1FUQlHJh4GWGd8tkzCi8N\n-----END SIGNATURE-----\n',
          },
        },
        'aarch64-nonfree': {
          publishedAt: '2025-03-21T23:56:38.299572946Z',
          urls: [
            'https://alpha-registry-x.start9.com/startos/v0.4.0-alpha.17/startos-0.4.0-alpha.17-b8ff331~dev_aarch64-nonfree.squashfs',
          ],
          commitment: {
            hash: '6k+0RcyRQV+5A+h06OqpHxd4IT6IlFkfdy9dfHIP90c=',
            size: 1641500672,
          },
          signatures: {
            '-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAqPjOeMD2CB3UEASoICgfJlfrO5ygjOSBCCNRyDBs75A=\n-----END PUBLIC KEY-----\n':
              '-----BEGIN SIGNATURE-----\nMEkwBQYDK2VwBEBEc2Vwxgf7Hxnm9GPlmc59DGRP6z0QyIQho7BlTSglXe3yz4FS\n8vtBGpOT9w0tRhZrWn5pInKr2R1OdGqoSosI\n-----END SIGNATURE-----\n',
          },
        },
        raspberrypi: {
          publishedAt: '2025-03-22T00:08:17.083064390Z',
          urls: [
            'https://alpha-registry-x.start9.com/startos/v0.4.0-alpha.17/startos-0.4.0-alpha.17-b8ff331~dev_raspberrypi.squashfs',
          ],
          commitment: {
            hash: 'K+XuTZxo1KVsKjNSV8PPOMruCvAEZwerF9mbpFl53Gk=',
            size: 1544417280,
          },
          signatures: {
            '-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAqPjOeMD2CB3UEASoICgfJlfrO5ygjOSBCCNRyDBs75A=\n-----END PUBLIC KEY-----\n':
              '-----BEGIN SIGNATURE-----\nMEkwBQYDK2VwBEAA07qx3oBE23A4XSRs5nRm4pCT534+wMwfozy0dEVdIwJ4tIWn\nnWhREVWXPJjmh5haoF+U4fISBbFKgJZhWbkD\n-----END SIGNATURE-----\n',
          },
        },
        x86_64: {
          publishedAt: '2025-03-22T00:05:57.684319247Z',
          urls: [
            'https://alpha-registry-x.start9.com/startos/v0.4.0-alpha.17/startos-0.4.0-alpha.17-b8ff331~dev_x86_64.squashfs',
          ],
          commitment: {
            hash: '3UVkx3TQMBPlSU1OnV48Om9vjjA3s+Nk6dX3auYGpBo=',
            size: 1424007168,
          },
          signatures: {
            '-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAqPjOeMD2CB3UEASoICgfJlfrO5ygjOSBCCNRyDBs75A=\n-----END PUBLIC KEY-----\n':
              '-----BEGIN SIGNATURE-----\nMEkwBQYDK2VwBEDuGbncT5wcXd26p8ieqZyDQKmAcWKx/gP90i2GMNXgCf9L4jA4\nTz+B5PqC6kljRstyT1600wL3R02+fRpqkAUH\n-----END SIGNATURE-----\n',
          },
        },
        'x86_64-nonfree': {
          publishedAt: '2025-03-22T00:07:11.893777122Z',
          urls: [
            'https://alpha-registry-x.start9.com/startos/v0.4.0-alpha.17/startos-0.4.0-alpha.17-b8ff331~dev_x86_64-nonfree.squashfs',
          ],
          commitment: {
            hash: 'IS1gJ56n/HlQqFbl1upMOAtLxyxB0cY0H89Ha+9h1lE=',
            size: 1743425536,
          },
          signatures: {
            '-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAqPjOeMD2CB3UEASoICgfJlfrO5ygjOSBCCNRyDBs75A=\n-----END PUBLIC KEY-----\n':
              '-----BEGIN SIGNATURE-----\nMEkwBQYDK2VwBEBOt5QUnB6XA2kOph45ARpAN9vlftX29Ic/8/Wcp4TmBQz3A+be\n5zGqW1Gvm4eoyocL+PjibxPYXS6kcC8O9YUA\n-----END SIGNATURE-----\n',
          },
        },
      },
      img: {},
    },
    '0.4.1-alpha.5': {
      headline: 'v0.4.1-alpha.5',
      releaseNotes: 'Some more release notes',
      sourceVersion: '>=0.3.5:0 <=0.4.0-alpha.5:0',
      authorized: ['G24CSA5HNYEPIXJNMK7ZM4KD5SX5N6X4'],
      iso: {},
      squashfs: {
        aarch64: {
          publishedAt: '2025-04-21T20:58:48.140749883Z',
          urls: [
            'https://alpha-registry-x.start9.com/startos/v0.4.0-alpha.14/startos-0.4.0-alpha.14-33ae46f~dev_aarch64.squashfs',
          ],
          commitment: {
            hash: '4elBFVkd/r8hNadKmKtLIs42CoPltMvKe2z3LRqkphk=',
            size: 1343500288,
          },
          signatures: {
            '-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAqPjOeMD2CB3UEASoICgfJlfrO5ygjOSBCCNRyDBs75A=\n-----END PUBLIC KEY-----\n':
              '-----BEGIN SIGNATURE-----\nMEkwBQYDK2VwBEAQlzQCZ07nY4REcknK3aZWtHlcAwSzV+Pae/5wb6ijV/utaNWu\n3BPWtKZFrS8K8fCfDmCHgFScLJCLUk4GwKoI\n-----END SIGNATURE-----\n',
          },
        },
        'aarch64-nonfree': {
          publishedAt: '2025-04-21T21:07:00.249285116Z',
          urls: [
            'https://alpha-registry-x.start9.com/startos/v0.4.0-alpha.14/startos-0.4.0-alpha.14-33ae46f~dev_aarch64-nonfree.squashfs',
          ],
          commitment: {
            hash: 'MrCEi4jxbmPS7zAiGk/JSKlMsiuKqQy6RbYOxlGHOIQ=',
            size: 1653075968,
          },
          signatures: {
            '-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAqPjOeMD2CB3UEASoICgfJlfrO5ygjOSBCCNRyDBs75A=\n-----END PUBLIC KEY-----\n':
              '-----BEGIN SIGNATURE-----\nMEkwBQYDK2VwBECg1u7PoQAuTvSjtVNGZz0tpZOV8TC0P8xpNSQacGfcklSGN5OT\nsmtu/E+z/o4c9mWa3h9QB4jRTWyYpz49H+gJ\n-----END SIGNATURE-----\n',
          },
        },
        raspberrypi: {
          publishedAt: '2025-04-21T21:16:12.933319237Z',
          urls: [
            'https://alpha-registry-x.start9.com/startos/v0.4.0-alpha.14/startos-0.4.0-alpha.14-33ae46f~dev_raspberrypi.squashfs',
          ],
          commitment: {
            hash: '/XTVQRCqY3RK544PgitlKu7UplXjkmzWoXUh2E4HCw0=',
            size: 1490731008,
          },
          signatures: {
            '-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAqPjOeMD2CB3UEASoICgfJlfrO5ygjOSBCCNRyDBs75A=\n-----END PUBLIC KEY-----\n':
              '-----BEGIN SIGNATURE-----\nMEkwBQYDK2VwBEDQAJaxCtyZSzUQ+oBB2TP9ctf7AyBD659vkhHVct7DTM+ZUqjm\ncCbUMV77PNlGFmDJEJ9kaGq8LmLMD467zqMA\n-----END SIGNATURE-----\n',
          },
        },
        x86_64: {
          publishedAt: '2025-04-21T21:14:20.246908903Z',
          urls: [
            'https://alpha-registry-x.start9.com/startos/v0.4.0-alpha.14/startos-0.4.0-alpha.14-33ae46f~dev_x86_64.squashfs',
          ],
          commitment: {
            hash: '/6romKTVQGSaOU7FqSZdw0kFyd7P+NBSYNwM3q7Fe44=',
            size: 1411657728,
          },
          signatures: {
            '-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAqPjOeMD2CB3UEASoICgfJlfrO5ygjOSBCCNRyDBs75A=\n-----END PUBLIC KEY-----\n':
              '-----BEGIN SIGNATURE-----\nMEkwBQYDK2VwBECWOeNTSyjYn2D5FLtyEtjfUbmNPB/vnGFfc3WK+HfuUvZrZEGj\n1bCdShp/4kuLrqEjasonSJTXUQfAJ1NT4gkA\n-----END SIGNATURE-----\n',
          },
        },
        'x86_64-nonfree': {
          publishedAt: '2025-04-21T21:15:17.955265284Z',
          urls: [
            'https://alpha-registry-x.start9.com/startos/v0.4.0-alpha.14/startos-0.4.0-alpha.14-33ae46f~dev_x86_64-nonfree.squashfs',
          ],
          commitment: {
            hash: 'HCRq9sr/0t85pMdrEgNBeM4x11zVKHszGnD1GDyZbSE=',
            size: 1731035136,
          },
          signatures: {
            '-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAqPjOeMD2CB3UEASoICgfJlfrO5ygjOSBCCNRyDBs75A=\n-----END PUBLIC KEY-----\n':
              '-----BEGIN SIGNATURE-----\nMEkwBQYDK2VwBECMvpyxKmTCzv+1Dlk28TSzyjCCb6+QNaXNA01rl4OHTN3YcqAQ\n4ubS89dDDoiOkxXv0J+aImG94SUqrSWXglYI\n-----END SIGNATURE-----\n',
          },
        },
      },
      img: {},
    },
  }

  export const RegistryInfo: T.RegistryInfo = {
    name: 'Start9 Registry',
    icon: REGISTRY_ICON,
    categories: {
      ai: {
        name: 'AI',
      },
      bitcoin: {
        name: 'Bitcoin',
      },
      crypto: {
        name: 'Crypto',
      },
      finance: {
        name: 'Finance',
      },
      media: {
        name: 'Media',
      },
      networking: {
        name: 'Networking',
      },
      'files-and-productivity': {
        name: 'Files and Productivity',
      },
      social: {
        name: 'Social',
      },
    },
  }

  export const MockManifestBitcoind: T.Manifest = {
    id: 'bitcoind',
    title: 'Bitcoin Core',
    version: '0.21.0:0',
    satisfies: [],
    canMigrateTo: '!',
    canMigrateFrom: '*',
    gitHash: 'abcdefgh',
    description: {
      short: 'A Bitcoin full node by Bitcoin Core.',
      long: 'Bitcoin is a decentralized consensus protocol and settlement network.',
    },
    releaseNotes: 'Taproot, Schnorr, and more.',
    license: 'MIT',
    packageRepo: 'https://github.com/start9labs/bitcoind-wrapper',
    upstreamRepo: 'https://github.com/bitcoin/bitcoin',
    marketingUrl: 'https://bitcoin.org',
    donationUrl: 'https://start9.com',
    osVersion: '0.2.12',
    sdkVersion: '0.4.0',
    dependencies: {},
    images: {
      main: {
        source: 'packed',
        arch: ['x86_64', 'aarch64'],
        emulateMissingAs: 'aarch64',
        nvidiaContainer: false,
      },
    },
    volumes: ['main'],
    hardwareRequirements: {
      device: [],
      arch: null,
      ram: null,
    },
    hardwareAcceleration: false,
    userspaceFilesystems: false,
    virtualNetworking: false,
    plugins: [],
  }

  export const MockManifestLnd: T.Manifest = {
    id: 'lnd',
    title: 'Lightning Network Daemon',
    version: '0.11.1:0',
    satisfies: [],
    canMigrateTo: '!',
    canMigrateFrom: '*',
    gitHash: 'abcdefgh',
    description: {
      short: 'A bolt spec compliant client.',
      long: 'More info about LND. More info about LND. More info about LND.',
    },
    releaseNotes: 'Dual funded channels!',
    license: 'MIT',
    packageRepo: 'https://github.com/start9labs/lnd-wrapper',
    upstreamRepo: 'https://github.com/lightningnetwork/lnd',
    marketingUrl: 'https://lightning.engineering/',
    donationUrl: null,
    osVersion: '0.2.12',
    sdkVersion: '0.4.0',
    dependencies: {
      bitcoind: {
        description: 'LND needs bitcoin to live.',
        optional: true,
        s9pk: '',
      },
      'btc-rpc-proxy': {
        description:
          'As long as Bitcoin is pruned, LND needs Bitcoin Proxy to fetch block over the P2P network.',
        optional: true,
        s9pk: '',
      },
    },
    images: {
      main: {
        source: 'packed',
        arch: ['x86_64', 'aarch64'],
        emulateMissingAs: 'aarch64',
        nvidiaContainer: false,
      },
    },
    volumes: ['main'],
    hardwareRequirements: {
      device: [],
      arch: null,
      ram: null,
    },
    hardwareAcceleration: false,
    userspaceFilesystems: false,
    virtualNetworking: false,
    plugins: [],
  }

  export const MockManifestTor: T.Manifest = {
    id: 'tor',
    title: 'Tor',
    version: '0.4.8:0',
    satisfies: [],
    canMigrateTo: '!',
    canMigrateFrom: '*',
    gitHash: 'torhash1',
    description: {
      short: 'An anonymous overlay network.',
      long: 'Tor provides anonymous communication by directing traffic through a free, worldwide overlay network.',
    },
    releaseNotes: 'Bug fixes and stability improvements.',
    license: 'BSD-3-Clause',
    packageRepo: 'https://github.com/start9labs/tor-wrapper',
    upstreamRepo: 'https://gitlab.torproject.org/tpo/core/tor',
    marketingUrl: 'https://www.torproject.org',
    donationUrl: null,
    osVersion: '0.2.12',
    sdkVersion: '0.4.0',
    dependencies: {},
    images: {
      main: {
        source: 'packed',
        arch: ['x86_64', 'aarch64'],
        emulateMissingAs: 'aarch64',
        nvidiaContainer: false,
      },
    },
    volumes: ['main'],
    hardwareRequirements: {
      device: [],
      arch: null,
      ram: null,
    },
    hardwareAcceleration: false,
    userspaceFilesystems: false,
    virtualNetworking: false,
    plugins: ['url-v0'],
  }

  export const MockManifestBitcoinProxy: T.Manifest = {
    id: 'btc-rpc-proxy',
    title: 'Bitcoin Proxy',
    version: '0.2.2:0',
    satisfies: [],
    canMigrateTo: '!',
    canMigrateFrom: '*',
    gitHash: 'lmnopqrx',
    description: {
      short: 'A super charger for your Bitcoin node.',
      long: 'More info about Bitcoin Proxy. More info about Bitcoin Proxy. More info about Bitcoin Proxy.',
    },
    releaseNotes: 'Even better support for Bitcoin and wallets!',
    license: 'MIT',
    packageRepo: 'https://github.com/start9labs/btc-rpc-proxy-wrapper',
    upstreamRepo: 'https://github.com/Kixunil/btc-rpc-proxy',
    marketingUrl: '',
    donationUrl: 'https://start9.com',
    osVersion: '0.2.12',
    sdkVersion: '0.4.0',
    dependencies: {
      bitcoind: {
        description: 'Bitcoin Proxy requires a Bitcoin node.',
        optional: false,
        s9pk: '',
      },
    },
    images: {
      main: {
        source: 'packed',
        arch: ['x86_64', 'aarch64'],
        emulateMissingAs: 'aarch64',
        nvidiaContainer: false,
      },
    },
    volumes: ['main'],
    hardwareRequirements: {
      device: [],
      arch: null,
      ram: null,
    },
    hardwareAcceleration: false,
    userspaceFilesystems: false,
    virtualNetworking: false,
    plugins: [],
  }

  export const BitcoinDep: T.DependencyMetadata = {
    title: 'Bitcoin',
    icon: BTC_ICON,
    optional: false,
    description: 'Needed to run',
  }

  export const ProxyDep: T.DependencyMetadata = {
    title: 'Bitcoin Proxy',
    icon: PROXY_ICON,
    optional: true,
    description: 'Needed to run',
  }

  export const OtherPackageVersions: {
    [id: T.PackageId]: GetPackagesRes
  } = {
    bitcoind: {
      '=26.1.0:0.1.0': {
        best: {
          '26.1.0:0.1.0': {
            title: 'Bitcoin Core',
            description: mockDescription,
            license: 'mit',
            packageRepo: 'https://github.com/start9labs/bitcoind-startos',
            upstreamRepo: 'https://github.com/bitcoin/bitcoin',
            marketingUrl: 'https://bitcoin.org',
            releaseNotes: 'Even better support for Bitcoin and wallets!',
            osVersion: '0.4.0',
            sdkVersion: '0.4.0-beta.49',
            gitHash: 'fakehash',
            icon: BTC_ICON,
            sourceVersion: null,
            satisfies: [],
            dependencyMetadata: {},
            donationUrl: null,
            s9pks: [
              [
                { arch: null, device: [], ram: null },
                {
                  urls: [
                    'https://github.com/Start9Labs/bitcoind-startos/releases/download/v26.1.0/bitcoind.s9pk',
                  ],
                  commitment: mockMerkleArchiveCommitment,
                  signatures: {},
                  publishedAt: Date.now().toString(),
                },
              ],
            ],
            hardwareAcceleration: false,
            userspaceFilesystems: false,
            virtualNetworking: false,
            plugins: [],
          },
          '#knots:26.1.20240325:0': {
            title: 'Bitcoin Knots',
            description: {
              short: 'An alternate fully verifying implementation of Bitcoin',
              long: 'Bitcoin Knots is a combined Bitcoin node and wallet. Not only is it easy to use, but it also ensures bitcoins you receive are both real bitcoins and really yours.',
            },
            license: 'mit',
            packageRepo: 'https://github.com/start9labs/bitcoinknots-startos',
            upstreamRepo: 'https://github.com/bitcoinknots/bitcoin',
            marketingUrl: 'https://bitcoinknots.org',
            releaseNotes: 'Even better support for Bitcoin and wallets!',
            osVersion: '0.4.0',
            sdkVersion: '0.4.0-beta.49',
            gitHash: 'fakehash',
            icon: BTC_ICON,
            sourceVersion: null,
            satisfies: [],
            dependencyMetadata: {},
            donationUrl: null,
            s9pks: [
              [
                { arch: null, device: [], ram: null },
                {
                  urls: [
                    'https://github.com/Start9Labs/bitcoinknots-startos/releases/download/v26.1.20240513/bitcoind.s9pk',
                  ],
                  commitment: mockMerkleArchiveCommitment,
                  signatures: {},
                  publishedAt: Date.now().toString(),
                },
              ],
            ],
            hardwareAcceleration: false,
            userspaceFilesystems: false,
            virtualNetworking: false,
            plugins: [],
          },
        },
        categories: ['bitcoin'],
        otherVersions: {
          '27.0.0:1.0.0': {
            releaseNotes: 'Even better support for Bitcoin and wallets!',
          },
          '#knots:27.1.0:0': {
            releaseNotes: 'Even better support for Bitcoin and wallets!',
          },
        },
      },
      '=#knots:26.1.20240325:0': {
        best: {
          '26.1.0:0.1.0': {
            title: 'Bitcoin Core',
            description: mockDescription,
            license: 'mit',
            packageRepo: 'https://github.com/start9labs/bitcoind-startos',
            upstreamRepo: 'https://github.com/bitcoin/bitcoin',
            marketingUrl: 'https://bitcoin.org',
            releaseNotes: 'Even better support for Bitcoin and wallets!',
            osVersion: '0.4.0',
            sdkVersion: '0.4.0-beta.49',
            gitHash: 'fakehash',
            icon: BTC_ICON,
            sourceVersion: null,
            satisfies: [],
            dependencyMetadata: {},
            donationUrl: null,
            s9pks: [
              [
                { arch: null, device: [], ram: null },
                {
                  urls: [
                    'https://github.com/Start9Labs/bitcoind-startos/releases/download/v26.1.0/bitcoind.s9pk',
                  ],
                  commitment: mockMerkleArchiveCommitment,
                  signatures: {},
                  publishedAt: Date.now().toString(),
                },
              ],
            ],
            hardwareAcceleration: false,
            userspaceFilesystems: false,
            virtualNetworking: false,
            plugins: [],
          },
          '#knots:26.1.20240325:0': {
            title: 'Bitcoin Knots',
            description: {
              short: 'An alternate fully verifying implementation of Bitcoin',
              long: 'Bitcoin Knots is a combined Bitcoin node and wallet. Not only is it easy to use, but it also ensures bitcoins you receive are both real bitcoins and really yours.',
            },
            license: 'mit',
            packageRepo: 'https://github.com/start9labs/bitcoinknots-startos',
            upstreamRepo: 'https://github.com/bitcoinknots/bitcoin',
            marketingUrl: 'https://bitcoinknots.org',
            releaseNotes: 'Even better support for Bitcoin and wallets!',
            osVersion: '0.4.0',
            sdkVersion: '0.4.0-beta.49',
            gitHash: 'fakehash',
            icon: BTC_ICON,
            sourceVersion: null,
            satisfies: [],
            dependencyMetadata: {},
            donationUrl: null,
            s9pks: [
              [
                { arch: null, device: [], ram: null },
                {
                  urls: [
                    'https://github.com/Start9Labs/bitcoinknots-startos/releases/download/v26.1.20240513/bitcoind.s9pk',
                  ],
                  commitment: mockMerkleArchiveCommitment,
                  signatures: {},
                  publishedAt: Date.now().toString(),
                },
              ],
            ],
            hardwareAcceleration: false,
            userspaceFilesystems: false,
            virtualNetworking: false,
            plugins: [],
          },
        },
        categories: ['bitcoin'],
        otherVersions: {
          '27.0.0:1.0.0': {
            releaseNotes: 'Even better support for Bitcoin and wallets!',
          },
          '#knots:27.1.0:0': {
            releaseNotes: 'Even better support for Bitcoin and wallets!',
          },
        },
      },
    },
    lnd: {
      '=0.17.5:0': {
        best: {
          '0.17.5:0': {
            title: 'LND',
            description: mockDescription,
            license: 'mit',
            packageRepo: 'https://github.com/start9labs/lnd-startos',
            upstreamRepo: 'https://github.com/lightningnetwork/lnd',
            marketingUrl: 'https://lightning.engineering/',
            releaseNotes: 'Upstream release to 0.17.5',
            osVersion: '0.4.0',
            sdkVersion: '0.4.0-beta.49',
            gitHash: 'fakehash',
            icon: LND_ICON,
            sourceVersion: null,
            satisfies: [],
            dependencyMetadata: {
              bitcoind: BitcoinDep,
              'btc-rpc-proxy': ProxyDep,
            },
            donationUrl: null,
            s9pks: [
              [
                { arch: null, device: [], ram: null },
                {
                  urls: [
                    'https://github.com/Start9Labs/lnd-startos/releases/download/v0.17.5/lnd.s9pk',
                  ],
                  commitment: mockMerkleArchiveCommitment,
                  signatures: {},
                  publishedAt: Date.now().toString(),
                },
              ],
            ],
            hardwareAcceleration: false,
            userspaceFilesystems: false,
            virtualNetworking: false,
            plugins: [],
          },
        },
        categories: ['bitcoin', 'finance'],
        otherVersions: {
          '0.18.0:0.0.1': {
            releaseNotes: 'Upstream release and minor fixes.',
          },
          '0.17.4-beta:1.0-alpha': {
            releaseNotes: 'Upstream release to 0.17.4',
          },
        },
      },
      '=0.17.4-beta:1.0-alpha': {
        best: {
          '0.17.4-beta:1.0-alpha': {
            title: 'LND',
            description: mockDescription,
            license: 'mit',
            packageRepo: 'https://github.com/start9labs/lnd-startos',
            upstreamRepo: 'https://github.com/lightningnetwork/lnd',
            marketingUrl: 'https://lightning.engineering/',
            releaseNotes: 'Upstream release to 0.17.4',
            osVersion: '0.4.0',
            sdkVersion: '0.4.0-beta.49',
            gitHash: 'fakehash',
            icon: LND_ICON,
            sourceVersion: null,
            satisfies: [],
            dependencyMetadata: {
              bitcoind: BitcoinDep,
              'btc-rpc-proxy': ProxyDep,
            },
            donationUrl: null,
            s9pks: [
              [
                { arch: null, device: [], ram: null },
                {
                  urls: [
                    'https://github.com/Start9Labs/lnd-startos/releases/download/v0.17.4/lnd.s9pk',
                  ],
                  commitment: mockMerkleArchiveCommitment,
                  signatures: {},
                  publishedAt: Date.now().toString(),
                },
              ],
            ],
            hardwareAcceleration: false,
            userspaceFilesystems: false,
            virtualNetworking: false,
            plugins: [],
          },
        },
        categories: ['bitcoin', 'finance'],
        otherVersions: {
          '0.18.0:0.0.1': {
            releaseNotes: 'Upstream release and minor fixes.',
          },
          '0.17.5:0': {
            releaseNotes: 'Upstream release to 0.17.5',
          },
        },
      },
    },
    'btc-rpc-proxy': {
      '=0.3.2.7:0': {
        best: {
          '0.3.2.7:0': {
            title: 'Bitcoin Proxy',
            description: mockDescription,
            license: 'mit',
            packageRepo: 'https://github.com/Start9Labs/btc-rpc-proxy-wrappers',
            upstreamRepo: 'https://github.com/Kixunil/btc-rpc-proxy',
            marketingUrl: '',
            releaseNotes: 'Upstream release and minor fixes.',
            osVersion: '0.4.0',
            sdkVersion: '0.4.0-beta.49',
            gitHash: 'fakehash',
            icon: PROXY_ICON,
            sourceVersion: null,
            satisfies: [],
            dependencyMetadata: {
              bitcoind: BitcoinDep,
            },
            donationUrl: null,
            s9pks: [
              [
                { arch: null, device: [], ram: null },
                {
                  urls: [
                    'https://github.com/Start9Labs/btc-rpc-proxy-startos/releases/download/v0.3.2.7/btc-rpc-proxy.s9pk',
                  ],
                  commitment: mockMerkleArchiveCommitment,
                  signatures: {},
                  publishedAt: Date.now().toString(),
                },
              ],
            ],
            hardwareAcceleration: false,
            userspaceFilesystems: false,
            virtualNetworking: false,
            plugins: [],
          },
        },
        categories: ['bitcoin', 'networking'],
        otherVersions: {
          '0.4.0:0': {
            releaseNotes: 'Major release with breaking changes.',
          },
        },
      },
    },
  }

  export const RegistryPackages: GetPackagesRes = {
    bitcoind: {
      best: {
        '27.0.0:1.0.0': {
          title: 'Bitcoin Core',
          description: mockDescription,
          license: 'mit',
          packageRepo: 'https://github.com/start9labs/bitcoind-startos',
          upstreamRepo: 'https://github.com/bitcoin/bitcoin',
          marketingUrl: 'https://bitcoin.org',
          releaseNotes: `# Bitcoin Core v27.0.0 Release Notes

## Overview

This is a major release of Bitcoin Core with significant performance improvements, new RPC methods, and critical security patches. We strongly recommend all users upgrade as soon as possible.

## Breaking Changes

- The deprecated \`getinfo\` RPC has been fully removed. Use \`getblockchaininfo\`, \`getnetworkinfo\`, and \`getwalletinfo\` instead.
- Configuration option \`rpcallowip\` no longer accepts hostnames — only CIDR notation is supported (e.g. \`192.168.1.0/24\`).
- The wallet database format has been migrated from BerkeleyDB to SQLite. Existing wallets will be automatically converted on first load. **This migration is irreversible.**

## New Features

- **Compact Block Filters (BIP 158):** Full support for serving compact block filters to light clients over the P2P network. Enable with \`-blockfilterindex=basic -peerblockfilters=1\`.
- **Miniscript support in descriptors:** You can now use miniscript policies inside \`wsh()\` descriptors for more expressive spending conditions.
- **New RPC: \`getdescriptoractivity\`:** Returns all wallet-relevant transactions for a given set of output descriptors within a block range.

## Performance Improvements

- Block validation is now 18% faster due to improved UTXO cache management and parallel script verification.
- Initial block download (IBD) time reduced by approximately 25% on NVMe storage thanks to batched database writes.
- Memory usage during reindex reduced from ~4.2 GB to ~2.8 GB peak.

## Configuration Changes

\`\`\`ini
# New options added in this release
blockfilterindex=basic       # Enable BIP 158 compact block filter index
peerblockfilters=1           # Serve compact block filters to peers
shutdownnotify=<cmd>         # Execute command on clean shutdown
v2transport=1                # Prefer BIP 324 encrypted P2P connections
\`\`\`

## Bug Fixes

1. Fixed a race condition in the mempool acceptance logic that could cause \`submitblock\` to return stale rejection reasons under high transaction throughput.
2. Corrected fee estimation for transactions with many inputs where the estimator previously overestimated by up to 15%.
3. Resolved an edge case where \`pruneblockchain\` could delete blocks still needed by an in-progress \`rescanblockchain\` operation.
4. Fixed incorrect handling of \`OP_CHECKSIGADD\` in legacy script verification mode that could lead to consensus divergence on certain non-standard transactions.
5. Patched a denial-of-service vector where a malicious peer could send specially crafted \`inv\` messages causing excessive memory allocation in the transaction request tracker.

## Dependency Updates

| Dependency | Old Version | New Version |
|------------|-------------|-------------|
| OpenSSL    | 1.1.1w      | 3.0.13      |
| libevent   | 2.1.12      | 2.2.1       |
| Boost      | 1.81.0      | 1.84.0      |
| SQLite     | 3.38.5      | 3.45.1      |
| miniupnpc  | 2.2.4       | 2.2.7       |

## Migration Guide

For users running Bitcoin Core as a service behind a reverse proxy, note that the default RPC authentication mechanism now uses cookie-based auth by default. If you previously relied on \`rpcuser\`/\`rpcpassword\`, you must explicitly set \`rpcauth\` in your configuration file. See https://github.com/bitcoin/bitcoin/blob/master/share/rpcauth/rpcauth.py for the auth string generator.

## Known Issues

- Wallet encryption with very long passphrases (>1024 characters) may cause the wallet to become temporarily unresponsive during unlock. A fix is planned for v27.0.1.
- The \`listtransactions\` RPC may return duplicate entries when called with \`include_watchonly=true\` on descriptor wallets that share derivation paths across multiple descriptors.

For the full changelog, see https://github.com/bitcoin/bitcoin/blob/v27.0.0/doc/release-notes/release-notes-27.0.0.md#full-changelog-with-detailed-descriptions-of-every-commit-and-pull-request-merged`,
          osVersion: '0.4.0',
          sdkVersion: '0.4.0-beta.49',
          gitHash: 'fakehash',
          icon: BTC_ICON,
          sourceVersion: '>=26.0.0:0',
          satisfies: [],
          dependencyMetadata: {},
          donationUrl: null,
          s9pks: [
            [
              { arch: null, device: [], ram: null },
              {
                urls: [
                  'https://github.com/Start9Labs/bitcoind-startos/releases/download/v27.0.0/bitcoind.s9pk',
                ],
                commitment: mockMerkleArchiveCommitment,
                signatures: {},
                publishedAt: Date.now().toString(),
              },
            ],
          ],
          hardwareAcceleration: false,
          userspaceFilesystems: false,
          virtualNetworking: false,
          plugins: [],
        },
        '#knots:27.1.0:0': {
          title: 'Bitcoin Knots',
          description: {
            short: 'An alternate fully verifying implementation of Bitcoin',
            long: 'Bitcoin Knots is a combined Bitcoin node and wallet. Not only is it easy to use, but it also ensures bitcoins you receive are both real bitcoins and really yours.',
          },
          license: 'mit',
          packageRepo: 'https://github.com/start9labs/bitcoinknots-startos',
          upstreamRepo: 'https://github.com/bitcoinknots/bitcoin',
          marketingUrl: 'https://bitcoinknots.org',
          releaseNotes: 'Even better support for Bitcoin and wallets!',
          osVersion: '0.4.0',
          sdkVersion: '0.4.0-beta.49',
          gitHash: 'fakehash',
          icon: BTC_ICON,
          sourceVersion: null,
          satisfies: [],
          dependencyMetadata: {},
          donationUrl: null,
          s9pks: [
            [
              { arch: null, device: [], ram: null },
              {
                urls: [
                  'https://github.com/Start9Labs/bitcoinknots-startos/releases/download/v26.1.20240513/bitcoind.s9pk',
                ],
                commitment: mockMerkleArchiveCommitment,
                signatures: {},
                publishedAt: Date.now().toString(),
              },
            ],
          ],
          hardwareAcceleration: false,
          userspaceFilesystems: false,
          virtualNetworking: false,
          plugins: [],
        },
      },
      categories: ['bitcoin'],
      otherVersions: {
        '26.1.0:0.1.0': {
          releaseNotes: 'Even better support for Bitcoin and wallets!',
        },
        '#knots:26.1.20240325:0': {
          releaseNotes: 'Even better Knots support for Bitcoin and wallets!',
        },
      },
    },
    lnd: {
      best: {
        '0.18.0:0.0.1': {
          title: 'LND',
          description: mockDescription,
          license: 'mit',
          packageRepo: 'https://github.com/start9labs/lnd-startos',
          upstreamRepo: 'https://github.com/lightningnetwork/lnd',
          marketingUrl: 'https://lightning.engineering/',
          releaseNotes: 'Upstream release and minor fixes.',
          osVersion: '0.4.0',
          sdkVersion: '0.4.0-beta.49',
          gitHash: 'fakehash',
          icon: LND_ICON,
          sourceVersion: null,
          satisfies: [],
          dependencyMetadata: {
            bitcoind: BitcoinDep,
            'btc-rpc-proxy': ProxyDep,
          },
          donationUrl: null,
          s9pks: [
            [
              { arch: null, device: [], ram: null },
              {
                urls: [
                  'https://github.com/Start9Labs/lnd-startos/releases/download/v0.18.0.1/lnd.s9pk',
                ],
                commitment: mockMerkleArchiveCommitment,
                signatures: {},
                publishedAt: Date.now().toString(),
              },
            ],
          ],
          hardwareAcceleration: false,
          userspaceFilesystems: false,
          virtualNetworking: false,
          plugins: [],
        },
      },
      categories: ['bitcoin', 'finance'],
      otherVersions: {
        '0.17.5:0': {
          releaseNotes: 'Upstream release to 0.17.5',
        },
        '0.17.4-beta:1.0-alpha': {
          releaseNotes: 'Upstream release to 0.17.4',
        },
      },
    },
    'btc-rpc-proxy': {
      best: {
        '0.4.0:0': {
          title: 'Bitcoin Proxy',
          description: mockDescription,
          license: 'mit',
          packageRepo: 'https://github.com/Start9Labs/btc-rpc-proxy-wrappers',
          upstreamRepo: 'https://github.com/Kixunil/btc-rpc-proxy',
          marketingUrl: '',
          releaseNotes: 'Major release with breaking changes.',
          osVersion: '0.4.0',
          sdkVersion: '0.4.0-beta.49',
          gitHash: 'fakehash',
          icon: PROXY_ICON,
          sourceVersion: null,
          satisfies: [],
          dependencyMetadata: {
            bitcoind: BitcoinDep,
          },
          donationUrl: null,
          s9pks: [
            [
              { arch: null, device: [], ram: null },
              {
                urls: [
                  'https://github.com/Start9Labs/btc-rpc-proxy-startos/releases/download/v0.4.0/btc-rpc-proxy.s9pk',
                ],
                commitment: mockMerkleArchiveCommitment,
                signatures: {},
                publishedAt: Date.now().toString(),
              },
            ],
          ],
          hardwareAcceleration: false,
          userspaceFilesystems: false,
          virtualNetworking: false,
          plugins: [],
        },
        '#test:0.5.0:0': {
          title: 'Bitcoin Proxy (Test)',
          description: mockDescription,
          license: 'mit',
          packageRepo: 'https://github.com/Start9Labs/btc-rpc-proxy-wrappers',
          upstreamRepo: 'https://github.com/Kixunil/btc-rpc-proxy',
          marketingUrl: '',
          releaseNotes: 'Test flavor that claims compat with 0.3.2.7:0.',
          osVersion: '0.4.0',
          sdkVersion: '0.4.0-beta.49',
          gitHash: 'fakehash',
          icon: PROXY_ICON,
          sourceVersion: null,
          satisfies: ['0.3.2.7:0'],
          dependencyMetadata: {
            bitcoind: BitcoinDep,
          },
          donationUrl: null,
          s9pks: [
            [
              { arch: null, device: [], ram: null },
              {
                urls: [
                  'https://github.com/Start9Labs/btc-rpc-proxy-startos/releases/download/v0.5.0-test/btc-rpc-proxy.s9pk',
                ],
                commitment: mockMerkleArchiveCommitment,
                signatures: {},
                publishedAt: Date.now().toString(),
              },
            ],
          ],
          hardwareAcceleration: false,
          userspaceFilesystems: false,
          virtualNetworking: false,
          plugins: [],
        },
      },
      categories: ['bitcoin', 'networking'],
      otherVersions: {
        '0.3.2.7:0': {
          releaseNotes: 'Upstream release and minor fixes.',
        },
      },
    },
  }

  export const Notifications: T.NotificationWithId[] = [
    {
      id: 1,
      packageId: null,
      createdAt: '2019-12-26T14:20:30.872Z',
      code: 1,
      level: 'success',
      title: 'Backup Complete',
      message: 'StartOS and services have been successfully backed up.',
      data: {
        server: {
          attempted: false,
          error: null,
        },
        packages: {
          bitcoind: {
            error: 'An error ocurred while backing up',
          },
        },
      },
      seen: false,
    },
    {
      id: 2,
      packageId: null,
      createdAt: '2019-12-26T14:20:30.872Z',
      code: 0,
      level: 'warning',
      title: 'SSH Key Added',
      message: 'A new SSH key was added. If you did not do this, shit is bad.',
      data: null,
      seen: false,
    },
    {
      id: 3,
      packageId: null,
      createdAt: '2019-12-26T14:20:30.872Z',
      code: 0,
      level: 'info',
      title: 'SSH Key Removed',
      message: 'A SSH key was removed.',
      data: null,
      seen: false,
    },
    {
      id: 4,
      packageId: 'bitcoind',
      createdAt: '2019-12-26T14:20:30.872Z',
      code: 0,
      level: 'error',
      title: 'Service Crashed',
      message: new Array(3)
        .fill(
          `2021-11-27T18:36:30.451064Z 2021-11-27T18:36:30Z tor: Thread interrupt
        2021-11-27T18:36:30.452833Z 2021-11-27T18:36:30Z Shutdown: In progress...
        2021-11-27T18:36:30.453128Z 2021-11-27T18:36:30Z addcon thread exit
        2021-11-27T18:36:30.453369Z 2021-11-27T18:36:30Z torcontrol thread exit`,
        )
        .join(''),
      data: null,
      seen: false,
    },
    {
      id: 5,
      packageId: null,
      createdAt: '2019-12-26T14:20:30.872Z',
      code: 2,
      level: 'success',
      title: 'Welcome to StartOS 0.4.0!',
      message: 'Click "View Details" to learn all about the new version',
      data: markdown,
      seen: false,
    },
  ]

  export function getMetrics(): T.Metrics {
    return {
      general: {
        temperature: {
          value: '66.8',
          unit: '°C',
        },
      },
      memory: {
        percentageUsed: {
          value: '30.7',
          unit: '%',
        },
        total: {
          value: '31971.10',
          unit: 'MiB',
        },
        available: {
          value: '22150.66',
          unit: 'MiB',
        },
        used: {
          value: '8784.97',
          unit: 'MiB',
        },
        zramTotal: {
          value: '7992.00',
          unit: 'MiB',
        },
        zramAvailable: {
          value: '7882.50',
          unit: 'MiB',
        },
        zramUsed: {
          value: '109.50',
          unit: 'MiB',
        },
      },
      cpu: {
        percentageUsed: {
          value: '8.4',
          unit: '%',
        },
        userSpace: {
          value: '7.0',
          unit: '%',
        },
        kernelSpace: {
          value: '1.4',
          unit: '%',
        },
        wait: {
          value: '0.5',
          unit: '%',
        },
        idle: {
          value: '91.1',
          unit: '%',
        },
      },
      disk: {
        capacity: {
          value: '1851.60',
          unit: 'GB',
        },
        used: {
          value: '859.02',
          unit: 'GB',
        },
        available: {
          value: '992.59',
          unit: 'GB',
        },
        percentageUsed: {
          value: '46.4',
          unit: '%',
        },
      },
    }
  }

  export const ServerLogs: T.LogEntry[] = [
    {
      timestamp: '2022-07-28T03:52:54.808769Z',
      message: '****** START *****',
      bootId: 'hsjnfdklasndhjasvbjamsksajbndjn',
    },
    {
      timestamp: '2019-12-26T14:21:30.872Z',
      message:
        '\u001b[34mPOST \u001b[0;32;49m200\u001b[0m photoview.startos/api/graphql \u001b[0;36;49m1.169406ms\u001b',
      bootId: 'hsjnfdklasndhjasvbjamsksajbndjn',
    },
    {
      timestamp: '2019-12-26T14:22:30.872Z',
      message: '****** FINISH *****',
      bootId: 'gvbwfiuasokdasjndasnjdmfvbahjdmdkfm',
    },
    {
      timestamp: '2019-12-26T15:22:30.872Z',
      message: '****** AGAIN *****',
      bootId: 'gvbwfiuasokdasjndasnjdmfvbahjdmdkfm',
    },
  ]

  export const Sessions: T.SessionList = {
    current: 'b7b1a9cef4284f00af9e9dda6e676177',
    sessions: {
      '9513226517c54ddd8107d6d7b9d8aed7': {
        loggedIn: '2021-07-14T20:49:17.774Z',
        lastActive: '2021-07-14T20:49:17.774Z',
        userAgent: 'AppleWebKit/{WebKit Rev} (KHTML, like Gecko)',
      },
      klndsfjhbwsajkdnaksj: {
        loggedIn: '2021-07-14T20:49:17.774Z',
        lastActive: '2019-07-14T20:49:17.774Z',
        userAgent: 'AppleWebKit/{WebKit Rev} (KHTML, like Gecko)',
      },
      b7b1a9cef4284f00af9e9dda6e676177: {
        loggedIn: '2021-07-14T20:49:17.774Z',
        lastActive: '2021-06-14T20:49:17.774Z',
        userAgent:
          'Mozilla/5.0 (Windows NT 6.1; Win64; x64; rv:47.0) Gecko/20100101 Firefox/47.0',
      },
    },
  }

  export const SshKeys: T.SshKeyResponse[] = [
    {
      createdAt: new Date().toISOString(),
      alg: 'ed25519',
      hostname: 'Matt Key',
      fingerprint: '28:d2:7e:78:61:b4:bf:g2:de:24:15:96:4e:d4:15:53',
    },
    {
      createdAt: new Date().toISOString(),
      alg: 'ed25519',
      hostname: 'Aiden Key',
      fingerprint: '12:f8:7e:78:61:b4:bf:e2:de:24:15:96:4e:d4:72:53',
    },
  ]

  export const SshKey: T.SshKeyResponse = {
    createdAt: new Date().toISOString(),
    alg: 'ed25519',
    hostname: 'Lucy Key',
    fingerprint: '44:44:7e:78:61:b4:bf:g2:de:24:15:96:4e:d4:15:53',
  }

  export const Wifi: T.WifiListInfo = {
    ethernet: true,
    ssids: {
      Goosers: 50,
      Goosers5G: 0,
    },
    connected: 'Goosers',
    country: 'US',
    availableWifi: [
      {
        ssid: 'Goosers a billion',
        strength: 40,
        security: [],
      },
      {
        ssid: 'Bill nye the wifi guy',
        strength: 99,
        security: ['1', '2', '3'],
      },
      {
        ssid: '',
        strength: 40,
        security: [],
      },
    ],
  }

  export const BackupTargets: { [id: string]: T.BackupTarget } = {
    hsbdjhasbasda: {
      type: 'cifs',
      hostname: 'smb://192.169.10.0',
      path: '/Desktop/startos-backups',
      username: 'TestUser',
      mountable: false,
      startOs: {
        '1234-5678-9876-5432': {
          hostname: 'adjective-noun',
          timestamp: new Date().toISOString(),
          version: '0.4.0',
          passwordHash:
            // password is asdfasdf
            '$argon2d$v=19$m=1024,t=1,p=1$YXNkZmFzZGZhc2RmYXNkZg$Ceev1I901G6UwU+hY0sHrFZ56D+o+LNJ',
          wrappedKey: '',
        },
      },
      legacyBackup: null,
    },
    // 'ftcvewdnkemfksdm': {
    //   type: 'disk',
    //   logicalname: 'sdba1',
    //   label: 'Matt Stuff',
    //   capacity: 1000000000000,
    //   used: 0,
    //   model: 'Evo SATA 2.5',
    //   vendor: 'Samsung',
    //   startOs: {},
    // },
    csgashbdjkasnd: {
      type: 'cifs',
      hostname: 'smb://192.169.10.0',
      path: '/Desktop/startos-backups-2',
      username: 'TestUser',
      mountable: true,
      startOs: {
        '1234-5678-9876-5432': {
          hostname: 'adjective-noun',
          timestamp: new Date().toISOString(),
          version: '0.4.0',
          passwordHash:
            // password is asdfasdf
            '$argon2d$v=19$m=1024,t=1,p=1$YXNkZmFzZGZhc2RmYXNkZg$Ceev1I901G6UwU+hY0sHrFZ56D+o+LNJ',
          wrappedKey: '',
        },
      },
      legacyBackup: { size: 5000000000, available: 50000000000 },
    },
    powjefhjbnwhdva: {
      type: 'disk',
      logicalname: 'sdba1',
      label: 'Another Drive',
      capacity: 2000000000000,
      used: 100000000000,
      model: null,
      vendor: 'SSK',
      guid: null,
      filesystem: null,
      startOs: {
        '1234-5678-9876-5432': {
          hostname: 'adjective-noun',
          timestamp: new Date().toISOString(),
          version: '0.4.0',
          passwordHash:
            // password is asdfasdf
            '$argon2d$v=19$m=1024,t=1,p=1$YXNkZmFzZGZhc2RmYXNkZg$Ceev1I901G6UwU+hY0sHrFZ56D+o+LNJ',
          wrappedKey: '',
        },
      },
      legacyBackup: { size: 5000000000000, available: 1000000000 },
    },
  }

  // @TODO 041

  // export const BackupTargets: RR.GetBackupTargetsRes = {
  //   unknownDisks: [
  //     {
  //       logicalname: 'sbc4',
  //       label: 'My Backup Drive',
  //       capacity: 2000000000000,
  //       used: 100000000000,
  //       model: 'T7',
  //       vendor: 'Samsung',
  //       startOs: {},
  //     },
  //   ],
  //   saved: {
  //     hsbdjhasbasda: {
  //       type: 'cifs',
  //       name: 'Embassy Backups',
  //       hostname: 'smb://192.169.10.0',
  //       path: '/Desktop/embassy-backups',
  //       username: 'TestUser',
  //       mountable: true,
  //       startOs: {
  //         abcdefgh: {
  //           hostname: 'adjective-noun.local',
  //           version: '0.4.0',
  //           timestamp: new Date().toISOString(),
  //           passwordHash:
  //             '$argon2d$v=19$m=1024,t=1,p=1$YXNkZmFzZGZhc2RmYXNkZg$Ceev1I901G6UwU+hY0sHrFZ56D+o+LNJ',
  //           wrappedKey: '',
  //         },
  //       },
  //     },
  //     ftcvewdnkemfksdm: {
  //       type: 'cloud',
  //       name: 'Dropbox 1',
  //       provider: 'dropbox',
  //       path: '/Home/backups',
  //       mountable: false,
  //       startOs: {},
  //     },
  //     csgashbdjkasnd: {
  //       type: 'cifs',
  //       name: 'Network Folder 2',
  //       hostname: 'smb://192.169.10.0',
  //       path: '/Desktop/embassy-backups-2',
  //       username: 'TestUser',
  //       mountable: true,
  //       startOs: {},
  //     },
  //     powjefhjbnwhdva: {
  //       type: 'disk',
  //       name: 'Physical Drive 1',
  //       logicalname: 'sdba1',
  //       label: 'Another Drive',
  //       capacity: 2000000000000,
  //       used: 100000000000,
  //       model: null,
  //       vendor: 'SSK',
  //       mountable: true,
  //       path: '/HomeFolder/Documents',
  //       startOs: {
  //         'different-server': {
  //           hostname: 'different-server.local',
  //           version: '0.4.0',
  //           timestamp: new Date().toISOString(),
  //           passwordHash:
  //             '$argon2d$v=19$m=1024,t=1,p=1$YXNkZmFzZGZhc2RmYXNkZg$Ceev1I901G6UwU+hY0sHrFZ56D+o+LNJ',
  //           wrappedKey: '',
  //         },
  //       },
  //     },
  //   },
  // }

  // export const BackupJobs: RR.GetBackupJobsRes = [
  //   {
  //     id: 'lalalalalala-babababababa',
  //     name: 'My Backup Job',
  //     targetId: Object.keys(BackupTargets.saved)[0],
  //     cron: '0 3 * * *',
  //     packageIds: ['bitcoind', 'lnd'],
  //   },
  //   {
  //     id: 'hahahahaha-mwmwmwmwmwmw',
  //     name: 'Another Backup Job',
  //     targetId: Object.keys(BackupTargets.saved)[1],
  //     cron: '0 * * * *',
  //     packageIds: ['lnd'],
  //   },
  // ]

  // export const BackupRuns: RR.GetBackupRunsRes = [
  //   {
  //     id: 'kladhbfweubdsk',
  //     startedAt: new Date().toISOString(),
  //     completedAt: new Date(new Date().valueOf() + 10000).toISOString(),
  //     packageIds: ['bitcoind', 'lnd'],
  //     job: BackupJobs[0],
  //     report: {
  //       server: {
  //         attempted: true,
  //         error: null,
  //       },
  //       packages: {
  //         bitcoind: { error: null },
  //         lnd: { error: null },
  //       },
  //     },
  //   },
  //   {
  //     id: 'kladhbfwhrfeubdsk',
  //     startedAt: new Date().toISOString(),
  //     completedAt: new Date(new Date().valueOf() + 10000).toISOString(),
  //     packageIds: ['bitcoind', 'lnd'],
  //     job: BackupJobs[0],
  //     report: {
  //       server: {
  //         attempted: true,
  //         error: null,
  //       },
  //       packages: {
  //         bitcoind: { error: null },
  //         lnd: { error: null },
  //       },
  //     },
  //   },
  // ]

  export const BackupInfo: T.BackupInfo = {
    version: '0.4.0',
    timestamp: new Date().toISOString(),
    packageBackups: {
      bitcoind: {
        title: 'Bitcoin Core',
        version: '0.21.0:0',
        osVersion: '0.4.0',
        timestamp: new Date().toISOString(),
      },
      'btc-rpc-proxy': {
        title: 'Bitcoin Proxy',
        version: '0.2.2:0',
        osVersion: '0.4.0',
        timestamp: new Date().toISOString(),
      },
    },
  }

  export const ActionResMessage: ActionRes = {
    version: '1',
    title: 'New Password',
    message:
      'Action was run successfully and smoothly and fully and all is good on the western front.',
    result: null,
  }

  export const ActionResSingle: ActionRes = {
    version: '1',
    title: 'New Password',
    message:
      'Action was run successfully and smoothly and fully and all is good on the western front.',
    result: {
      type: 'single',
      copyable: true,
      qr: false,
      masked: true,
      value: 'iwejdoiewdhbew',
    },
  }

  export const ActionResGroup: ActionRes = {
    version: '1',
    title: 'Properties',
    message:
      'Successfully retrieved properties. Here is a bunch of useful information about this service.',
    result: {
      type: 'group',
      value: [
        {
          type: 'single',
          name: 'LND Connect',
          description: 'This is some information about the thing.',
          copyable: true,
          qr: true,
          masked: true,
          value:
            'lndconnect://udlyfq2mxa4355pt7cqlrdipnvk2tsl4jtsdw7zaeekenufwcev2wlad.onion:10009?cert=MIICJTCCAcugAwIBAgIRAOyq85fqAiA3U3xOnwhH678wCgYIKoZIzj0EAwIwODEfMB0GAkUEChMWbG5kIGF1dG9nZW5lcmF0ZWQgY2VydDEVMBMGA1UEAxMMNTc0OTkwMzIyYzZlMB4XDTIwMTAyNjA3MzEyN1oXDTIxMTIyMTA3MzEyN1owODEfMB0GA1UEChMWbG5kIGF1dG9nZW5lcmF0ZWQgY2VydDEVMBMGA1UEAxMMNTc0OTkwMzIyYzZlMFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEKqfhAMMZdY-eFnU5P4bGrQTSx0lo7m8u4V0yYkzUM6jlql_u31_mU2ovLTj56wnZApkEjoPl6fL2yasZA2wiy6OBtTCBsjAOBgNVHQ8BAf8EBAMCAqQwEwYDVR0lBAwwCgYIKwYBBQUHAwEwDwYDVR0TAQH_BAUwAwEB_zAdBgNVHQ4EFgQUYQ9uIO6spltnVCx4rLFL5BvBF9IwWwYDVR0RBFQwUoIMNTc0OTkwMzIyYzZlgglsb2NhbGhvc3SCBHVuaXiCCnVuaXhwYWNrZXSCB2J1ZmNvbm6HBH8AAAGHEAAAAAAAAAAAAAAAAAAAAAGHBKwSAAswCgYIKoZIzj0EAwIDSAAwRQIgVZH2Z2KlyAVY2Q2aIQl0nsvN-OEN49wreFwiBqlxNj4CIQD5_JbpuBFJuf81I5J0FQPtXY-4RppWOPZBb-y6-rkIUQ&macaroon=AgEDbG5kAusBAwoQuA8OUMeQ8Fr2h-f65OdXdRIBMBoWCgdhZGRyZXNzEgRyZWFkEgV3cml0ZRoTCgRpbmZvEgRyZWFkEgV3cml0ZRoXCghpbnZvaWNlcxIEcmVhZBIFd3JpdGUaFAoIbWFjYXJvb24SCGdlbmVyYXRlGhYKB21lc3NhZ2USBHJlYWQSBXdyaXRlGhcKCG9mZmNoYWluEgRyZWFkEgV3cml0ZRoWCgdvbmNoYWluEgRyZWFkEgV3cml0ZRoUCgVwZWVycxIEcmVhZBIFd3JpdGUaGAoGc2lnbmVyEghnZW5lcmF0ZRIEcmVhZAAABiCYsRUoUWuAHAiCSLbBR7b_qULDSl64R8LIU2aqNIyQfA',
        },
        {
          type: 'group',
          name: 'Nested Stuff',
          description: 'This is a nested thing metric',
          value: [
            {
              type: 'single',
              name: 'Last Name',
              description: 'The last name of the user',
              copyable: true,
              qr: true,
              masked: false,
              value: 'Hill',
            },
            {
              type: 'single',
              name: 'Age',
              description: 'The age of the user',
              copyable: false,
              qr: false,
              masked: false,
              value: '35',
            },
            {
              type: 'single',
              name: 'Password',
              description: 'A secret password',
              copyable: true,
              qr: false,
              masked: true,
              value: 'password123',
            },
          ],
        },
        {
          type: 'single',
          name: 'Another Value',
          description: 'Some more information about the service.',
          copyable: false,
          qr: true,
          masked: false,
          value: 'https://guessagain.com',
        },
      ],
    },
  }

  export const getCreateOnionServiceSpec = async (): Promise<IST.InputSpec> =>
    configBuilderToSpec(
      ISB.InputSpec.of({
        ssl: ISB.Value.toggle({
          name: 'SSL',
          description: 'Enable HTTPS for this onion service',
          default: true,
        }),
        privateKey: ISB.Value.text({
          name: 'Private Key',
          description:
            'Optionally provide an existing ed25519 private key to reuse a .onion address. Leave blank to generate a new one.',
          required: false,
          default: null,
          masked: true,
        }),
        urlPluginMetadata: ISB.Value.hidden<{
          packageId: string
          interfaceId: string
          hostId: string
          internalPort: number
        }>(),
      }),
    )

  export const getActionInputSpec = async (): Promise<IST.InputSpec> =>
    configBuilderToSpec(
      ISB.InputSpec.of({
        bitcoin: ISB.Value.object(
          {
            name: 'Bitcoin Settings',
            description:
              'RPC and P2P interface configuration options for Bitcoin Core',
          },
          ISB.InputSpec.of({
            'bitcoind-p2p': ISB.Value.union({
              name: 'P2P Settings',
              description:
                '<p>The Bitcoin Core node to connect to over the peer-to-peer (P2P) interface:</p><ul><li><strong>Bitcoin Core</strong>: The Bitcoin Core service installed on this device</li><li><strong>External Node</strong>: A Bitcoin node running on a different device</li></ul>',
              default: 'internal',
              variants: ISB.Variants.of({
                internal: { name: 'Bitcoin Core', spec: ISB.InputSpec.of({}) },
                external: {
                  name: 'External Node',
                  spec: ISB.InputSpec.of({
                    'p2p-host': ISB.Value.text({
                      name: 'Public Address',
                      required: false,
                      default: null,
                      description:
                        'The public address of your Bitcoin Core server',
                    }),
                    'p2p-port': ISB.Value.number({
                      name: 'P2P Port',
                      description:
                        'The port that your Bitcoin Core P2P server is bound to',
                      required: true,
                      default: 8333,
                      min: 0,
                      max: 65535,
                      integer: true,
                    }),
                  }),
                },
              }),
            }),
          }),
        ),
        color: ISB.Value.color({
          name: 'Color',
          required: false,
          default: null,
        }),
        datetime: ISB.Value.datetime({
          name: 'Datetime',
          required: false,
          default: null,
        }),
        notifications: ISB.Value.triState({
          name: 'Push Notifications',
          default: null,
          description:
            'Three-state toggle with a neutral "Default" middle position',
          footnote: 'Leave on Default to use the upstream configuration',
        }),
        maxConnections: ISB.Value.number({
          name: 'Max Connections',
          required: false,
          default: null,
          integer: true,
          footnote: 'Defaults to 1024 upstream if left blank',
          min: 1,
          max: 65535,
        }),
        // file: ISB.Value.file({
        //   name: 'File',
        //   required: false,
        //   extensions: ['png', 'pdf'],
        // }),
        users: ISB.Value.multiselect({
          name: 'Users',
          default: [],
          maxLength: 2,
          values: {
            matt: 'Matt Hill',
            alex: 'Alex Inkin',
            blue: 'Blue J',
            lucy: 'Lucy',
          },
        }),
        advanced: ISB.Value.object(
          {
            name: 'Advanced',
            description: 'Advanced settings',
          },
          ISB.InputSpec.of({
            rpcsettings: ISB.Value.object(
              {
                name: 'RPC Settings',
                description: 'rpc username and password',
              },
              ISB.InputSpec.of({
                rpcuser2: ISB.Value.text({
                  name: 'RPC Username',
                  required: false,
                  default: 'defaultrpcusername',
                  description: 'rpc username',
                  patterns: [
                    {
                      regex: '^[a-zA-Z]+$',
                      description: 'must contain only letters.',
                    },
                  ],
                }),
                rpcuser: ISB.Value.text({
                  name: 'RPC Username',
                  required: true,
                  default: 'defaultrpcusername',
                  description: 'rpc username',
                  patterns: [
                    {
                      regex: '^[a-zA-Z]+$',
                      description: 'must contain only letters.',
                    },
                  ],
                }),
                rpcpass: ISB.Value.text({
                  name: 'RPC User Password',
                  required: true,
                  default: {
                    charset: 'a-z,A-Z,2-9',
                    len: 20,
                  },
                  description: 'rpc password',
                }),
                rpcpass2: ISB.Value.text({
                  name: 'RPC User Password',
                  required: true,
                  default: {
                    charset: 'a-z,A-Z,2-9',
                    len: 20,
                  },
                  description: 'rpc password',
                }),
              }),
            ),
          }),
        ),
        testnet: ISB.Value.toggle({
          name: 'Testnet',
          default: true,
          description:
            '<ul><li>determines whether your node is running on testnet or mainnet</li></ul><script src="fake"></script>',
          warning: 'Chain will have to resync!',
          immutable: true,
        }),
        'object-list': ISB.Value.list(
          ISB.List.obj(
            {
              name: 'Object List',
              minLength: 0,
              maxLength: 4,
              default: [
                // { 'first-name': 'Admin', 'last-name': 'User', age: 40 },
                // { 'first-name': 'Admin2', 'last-name': 'User', age: 40 },
              ],
              description: 'This is a list of objects, like users or something',
            },
            {
              spec: ISB.InputSpec.of({
                'first-name': ISB.Value.text({
                  name: 'First Name',
                  required: false,
                  description: 'User first name',
                  default: 'Matt',
                }),
                'last-name': ISB.Value.text({
                  name: 'Last Name',
                  required: true,
                  default: {
                    charset: 'a-g,2-9',
                    len: 12,
                  },
                  description: 'User first name',
                  patterns: [
                    {
                      regex: '^[a-zA-Z]+$',
                      description: 'must contain only letters.',
                    },
                  ],
                }),
                age: ISB.Value.number({
                  name: 'Age',
                  description: 'The age of the user',
                  warning: 'User must be at least 18.',
                  required: false,
                  default: null,
                  min: 18,
                  integer: false,
                }),
              }),
              displayAs: `I'm {{last-name}}, {{first-name}} {{last-name}}`,
              uniqueBy: 'last-name',
            },
          ),
        ),
        'union-list': ISB.Value.list(
          ISB.List.obj(
            {
              name: 'Union List',
              minLength: 0,
              maxLength: 2,
              default: [],
              description: 'This is a sample list of unions',
              warning: 'If you change this, things may work.',
            },
            {
              spec: ISB.InputSpec.of({
                union: ISB.Value.union({
                  name: 'Preference',
                  description: null,
                  warning: null,
                  default: 'summer',
                  variants: ISB.Variants.of({
                    summer: {
                      name: 'summer',
                      spec: ISB.InputSpec.of({
                        'favorite-tree': ISB.Value.text({
                          name: 'Favorite Tree',
                          required: true,
                          default: 'Maple',
                          description: 'What is your favorite tree?',
                        }),
                        'favorite-flower': ISB.Value.select({
                          name: 'Favorite Flower',
                          description: 'Select your favorite flower',
                          default: 'none',
                          values: {
                            none: 'none',
                            red: 'red',
                            blue: 'blue',
                            purple: 'purple',
                          },
                        }),
                      }),
                    },
                    winter: {
                      name: 'winter',
                      spec: ISB.InputSpec.of({
                        'like-snow': ISB.Value.toggle({
                          name: 'Like Snow?',
                          default: true,
                          description: 'Do you like snow or not?',
                        }),
                      }),
                    },
                  }),
                }),
              }),
              uniqueBy: 'preference',
            },
          ),
        ),
        'random-select': ISB.Value.dynamicSelect(() => ({
          name: 'Random select',
          description: 'This is not even real.',
          warning: 'Be careful changing this!',
          default: 'option1',
          values: {
            option1: 'option1',
            option2: 'option2',
            option3: 'option3',
            option4:
              'https://qwertyuiopasdfghjklzxcvbnmqwertyuiopasdfghjklzxcvbnm12345.onion',
          },
          disabled: ['option2'],
        })),
        'favorite-number': ISB.Value.number({
          name: 'Favorite Number',
          description: 'Your favorite number of all time',
          warning:
            'Once you set this number, it can never be changed without severe consequences.',
          required: false,
          default: 7,
          integer: false,
          units: 'BTC',
          placeholder: 'Is it 237?',
          min: -100,
          max: 100,
        }),
        rpcsettings: ISB.Value.object(
          {
            name: 'RPC Settings',
            description: 'rpc username and password',
          },
          ISB.InputSpec.of({
            laws: ISB.Value.object(
              {
                name: 'Laws',
                description: 'the law of the realm',
              },
              ISB.InputSpec.of({
                law1: ISB.Value.text({
                  name: 'First Law',
                  required: false,
                  description: 'the first law',
                  default: null,
                }),
                law2: ISB.Value.text({
                  name: 'Second Law',
                  required: false,
                  description: 'the second law',
                  default: null,
                }),
              }),
            ),
            rulemakers: ISB.Value.list(
              ISB.List.obj(
                {
                  name: 'Rule Makers',
                  minLength: 0,
                  maxLength: 2,
                  description: 'the people who make the rules',
                },
                {
                  spec: ISB.InputSpec.of({
                    rulemakername: ISB.Value.text({
                      name: 'Rulemaker Name',
                      required: true,
                      default: {
                        charset: 'a-g,2-9',
                        len: 12,
                      },
                      description: 'the name of the rule maker',
                    }),
                    rulemakerip: ISB.Value.text({
                      name: 'Rulemaker IP',
                      required: true,
                      default: '192.168.1.0',
                      description: 'the ip of the rule maker',
                      patterns: [
                        {
                          regex:
                            '^(([0-9]|[1-9][0-9]|1[0-9]{2}|2[0-4][0-9]|25[0-5])\\.){3}([0-9]|[1-9][0-9]|1[0-9]{2}|2[0-4][0-9]|25[0-5])$',
                          description: 'may only contain numbers and periods',
                        },
                      ],
                    }),
                  }),
                },
              ),
            ),
            rpcuser: ISB.Value.text({
              name: 'RPC Username',
              required: true,
              default: 'defaultrpcusername',
              description: 'rpc username',
              patterns: [
                {
                  regex: '^[a-zA-Z]+$',
                  description: 'must contain only letters.',
                },
              ],
            }),
            rpcpass: ISB.Value.text({
              name: 'RPC User Password',
              required: true,
              default: {
                charset: 'a-z,A-Z,2-9',
                len: 20,
              },
              description: 'rpc password',
              masked: true,
            }),
          }),
        ),
        'bitcoin-node': ISB.Value.union({
          name: 'Bitcoin Node',
          description: 'Options<ul><li>Item 1</li><li>Item 2</li></ul>',
          warning: 'Careful changing this',
          default: 'internal',
          variants: ISB.Variants.of({
            fake: {
              name: 'Fake',
              spec: ISB.InputSpec.of({}),
            },
            internal: {
              name: 'Internal',
              spec: ISB.InputSpec.of({
                listitems: ISB.Value.list(
                  ISB.List.text(
                    {
                      name: 'RPC Allowed IPs',
                      minLength: 1,
                      maxLength: 10,
                      default: ['192.168.1.1'],
                      description:
                        'external ip addresses that are authorized to access your Bitcoin node',
                      warning:
                        'Any IP you allow here will have RPC access to your Bitcoin node.',
                    },
                    {
                      patterns: [
                        {
                          regex:
                            '((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])|((^(([0-9a-fA-F]{1,4}:){7,7}[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,7}:|([0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,5}(:[0-9a-fA-F]{1,4}){1,2}|([0-9a-fA-F]{1,4}:){1,4}(:[0-9a-fA-F]{1,4}){1,3}|([0-9a-fA-F]{1,4}:){1,3}(:[0-9a-fA-F]{1,4}){1,4}|([0-9a-fA-F]{1,4}:){1,2}(:[0-9a-fA-F]{1,4}){1,5}|[0-9a-fA-F]{1,4}:((:[0-9a-fA-F]{1,4}){1,6})|:((:[0-9a-fA-F]{1,4}){1,7}|:)|fe80:(:[0-9a-fA-F]{0,4}){0,4}%[0-9a-zA-Z]{1,}|::(ffff(:0{1,4}){0,1}:){0,1}((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9]).){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])|([0-9a-fA-F]{1,4}:){1,4}:((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9]).){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9]))$)|(^[a-z2-7]{16}\\.onion$)|(^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\\.)+[a-z0-9][a-z0-9-]{0,61}[a-z0-9]$))',
                          description:
                            'must be a valid ipv4, ipv6, or domain name',
                        },
                      ],
                    },
                  ),
                ),
                name: ISB.Value.text({
                  name: 'Name',
                  required: false,
                  default: null,
                  patterns: [
                    {
                      regex: '^[a-zA-Z]+$',
                      description: 'Must contain only letters.',
                    },
                  ],
                }),
              }),
            },
            external: {
              name: 'External',
              spec: ISB.InputSpec.of({
                'emergency-contact': ISB.Value.object(
                  {
                    name: 'Emergency Contact',
                    description: 'The person to contact in case of emergency.',
                  },
                  ISB.InputSpec.of({
                    name: ISB.Value.text({
                      name: 'Name',
                      required: false,
                      default: null,
                      patterns: [
                        {
                          regex: '^[a-zA-Z]+$',
                          description: 'Must contain only letters.',
                        },
                      ],
                    }),
                    email: ISB.Value.text({
                      name: 'Email',
                      inputmode: 'email',
                      required: false,
                      default: null,
                    }),
                  }),
                ),
                'public-domain': ISB.Value.text({
                  name: 'Public Domain',
                  required: true,
                  default: 'bitcoinnode.com',
                  description: 'the public address of the node',
                  patterns: [
                    {
                      regex: '.*',
                      description: 'anything',
                    },
                  ],
                }),
                'private-domain': ISB.Value.text({
                  name: 'Private Domain',
                  required: false,
                  default: null,
                  description: 'the private address of the node',
                  masked: true,
                  inputmode: 'url',
                }),
              }),
            },
          }),
        }),
        port: ISB.Value.number({
          name: 'Port',
          description:
            'the default port for your Bitcoin node. default: 8333, testnet: 18333, regtest: 18444',
          required: true,
          default: 8333,
          min: 1,
          max: 9998,
          step: 1,
          integer: true,
        }),
        'favorite-slogan': ISB.Value.text({
          name: 'Favorite Slogan',
          generate: {
            charset: 'a-z,A-Z,2-9',
            len: 20,
          },
          required: false,
          default: null,
          description:
            'You most favorite slogan in the whole world, used for paying you.',
          masked: true,
        }),
        rpcallowip: ISB.Value.list(
          ISB.List.text(
            {
              name: 'RPC Allowed IPs',
              minLength: 1,
              maxLength: 10,
              default: ['192.168.1.1'],
              description:
                'external ip addresses that are authorized to access your Bitcoin node',
              warning:
                'Any IP you allow here will have RPC access to your Bitcoin node.',
            },
            {
              patterns: [
                {
                  regex:
                    '((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])|((^(([0-9a-fA-F]{1,4}:){7,7}[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,7}:|([0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,5}(:[0-9a-fA-F]{1,4}){1,2}|([0-9a-fA-F]{1,4}:){1,4}(:[0-9a-fA-F]{1,4}){1,3}|([0-9a-fA-F]{1,4}:){1,3}(:[0-9a-fA-F]{1,4}){1,4}|([0-9a-fA-F]{1,4}:){1,2}(:[0-9a-fA-F]{1,4}){1,5}|[0-9a-fA-F]{1,4}:((:[0-9a-fA-F]{1,4}){1,6})|:((:[0-9a-fA-F]{1,4}){1,7}|:)|fe80:(:[0-9a-fA-F]{0,4}){0,4}%[0-9a-zA-Z]{1,}|::(ffff(:0{1,4}){0,1}:){0,1}((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9]).){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])|([0-9a-fA-F]{1,4}:){1,4}:((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9]).){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9]))$)|(^[a-z2-7]{16}\\.onion$)|(^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\\.)+[a-z0-9][a-z0-9-]{0,61}[a-z0-9]$))',
                  description: 'must be a valid ipv4, ipv6, or domain name',
                },
              ],
            },
          ),
        ),
        rpcauth: ISB.Value.list(
          ISB.List.text(
            {
              name: 'RPC Auth',
              minLength: 3,
              description:
                'api keys that are authorized to access your Bitcoin node.',
            },
            {
              patterns: [],
            },
          ),
        ),
      }),
    )

  export const MockConfig = {
    testnet: undefined,
    'object-list': [
      {
        'first-name': 'First',
        'last-name': 'Last',
        age: 30,
      },
      {
        'first-name': 'First2',
        'last-name': 'Last2',
        age: 40,
      },
      {
        'first-name': 'First3',
        'last-name': 'Last3',
        age: 60,
      },
    ],
    'random-select': ['goodbye'],
    'favorite-number': 0,
    rpcsettings: {
      laws: {
        law1: 'The first law Amended',
        law2: 'The second law',
      },
      rpcpass: undefined,
      rpcuser: '123',
      rulemakers: [],
    },
    'bitcoin-node': {
      selection: 'internal',
      value: {
        listitems: ['192.168.1.1', '192.1681.23'],
        name: 'Matt',
      },
      other: {
        external: {
          'public-domain': 'test.com',
        },
      },
    },
    port: 20,
    rpcallowip: undefined,
    rpcauth: ['matt: 8273gr8qwoidm1uid91jeh8y23gdio1kskmwejkdnm'],
    advanced: undefined,
  }

  export const MockDependencyConfig = MockConfig

  export const bitcoind: PackageDataEntry<InstalledState> = {
    stateInfo: {
      state: 'installed',
      manifest: MockManifestBitcoind,
    },
    s9pk: '/media/startos/data/package-data/archive/installed/asdfasdf.s9pk',
    icon: '/assets/img/service-icons/bitcoin-core.svg',
    lastBackup: null,
    statusInfo: {
      error: null,
      desired: { main: 'running' },
      started: new Date().toISOString(),
      health: {},
    },
    actions: {
      config: {
        name: 'Set Config',
        description: 'edit bitcoin.conf',
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
              assignedPort: 80,
              assignedSslPort: 443,
            },
            addresses: {
              enabled: [],
              disabled: [],
              guaAccess: { '[2001:db8:abcd::a3b:2]:1234': 'lan-wan' },
              available: [
                {
                  ssl: true,
                  public: false,
                  hostname: 'adjective-noun.local',
                  port: 1234,
                  metadata: {
                    kind: 'mdns',
                    gateways: ['eth0', 'wlan0'],
                  },
                },
                {
                  ssl: true,
                  public: false,
                  hostname: '192.168.10.11',
                  port: 1234,
                  metadata: { kind: 'ipv4', gateway: 'wlan0' },
                },
                {
                  ssl: true,
                  public: false,
                  hostname: '10.0.0.2',
                  port: 1234,
                  metadata: { kind: 'ipv4', gateway: 'wlan0' },
                },
                {
                  ssl: true,
                  public: false,
                  hostname: 'fe80:cd00:0000:0cde:1257:0000:211e:72cd',
                  port: 1234,
                  metadata: { kind: 'ipv6', gateway: 'eth0', scopeId: 2 },
                },
                {
                  ssl: true,
                  public: false,
                  hostname: 'fe80:cd00:0000:0cde:1257:0000:211e:1234',
                  port: 1234,
                  metadata: { kind: 'ipv6', gateway: 'wlan0', scopeId: 3 },
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
              addSsl: null,
              preferredExternalPort: 443,
              secure: { ssl: true },
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
            },
          },
        },
        bindingRanges: {},
        publicDomains: {},
        privateDomains: {},
        portForwards: [],
      },
      bcdefgh: {
        bindings: {
          8332: {
            enabled: true,
            net: {
              assignedPort: 8332,
              assignedSslPort: null,
            },
            addresses: {
              enabled: [],
              disabled: [],
              guaAccess: {},
              available: [],
            },
            options: {
              addSsl: null,
              preferredExternalPort: 8332,
              secure: { ssl: false },
            },
            interfaces: {
              rpc: {
                id: 'rpc',
                masked: false,
                name: 'RPC',
                description:
                  'Used by dependent services and client wallets for connecting to your node',
                type: 'api',
                addressInfo: {
                  username: null,
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
              assignedPort: 8333,
              assignedSslPort: null,
            },
            addresses: {
              enabled: [],
              disabled: [],
              guaAccess: {},
              available: [],
            },
            options: {
              addSsl: null,
              preferredExternalPort: 8333,
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
      'bitcoind-config': {
        task: {
          packageId: 'bitcoind',
          actionId: 'config',
          severity: 'critical',
          reason:
            'You must run Config before starting Bitcoin for the first time',
        },
        active: true,
      },
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
  }

  export const bitcoinProxy: PackageDataEntry<InstalledState> = {
    stateInfo: {
      state: 'installed',
      manifest: MockManifestBitcoinProxy,
    },
    s9pk: '/media/startos/data/package-data/archive/installed/asdfasdf.s9pk',
    icon: '/assets/img/service-icons/btc-rpc-proxy.png',
    lastBackup: null,
    statusInfo: {
      desired: { main: 'stopped' },
      started: null,
      health: {},
      error: null,
    },
    actions: {},
    currentDependencies: {
      bitcoind: {
        title: BitcoinDep.title,
        icon: BitcoinDep.icon,
        kind: 'running',
        versionRange: '>=26.0.0',
        healthChecks: [],
      },
    },
    hosts: {},
    storeExposedDependents: [],
    outboundGateway: null,
    registry: 'https://registry.start9.com/',
    developerKey: 'developer-key',
    plugin: { url: null },
    tasks: {},
  }

  export const lnd: PackageDataEntry<InstalledState> = {
    stateInfo: {
      state: 'installed',
      manifest: MockManifestLnd,
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
        title: BitcoinDep.title,
        icon: BitcoinDep.icon,
        kind: 'running',
        versionRange: '>=26.0.0',
        healthChecks: [],
      },
      'btc-rpc-proxy': {
        title: Mock.MockManifestBitcoinProxy.title,
        icon: 'assets/img/service-icons/btc-rpc-proxy.png',
        kind: 'exists',
        versionRange: '>2.0.0',
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
  }

  export const LocalPkgs: { [key: string]: PackageDataEntry<InstalledState> } =
    {
      bitcoind: bitcoind,
      'btc-rpc-proxy': bitcoinProxy,
      lnd: lnd,
    }
}
