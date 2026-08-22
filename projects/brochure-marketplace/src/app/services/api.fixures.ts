import { GetPackagesRes } from '@start9labs/marketplace'
import { T } from '@start9labs/start-core'
import { BTC_ICON, LND_ICON, PROXY_ICON, REGISTRY_ICON } from './api-icons'

const mockMerkleArchiveCommitment: T.MerkleArchiveCommitment = {
  rootSighash: 'fakehash',
  rootMaxsize: 0,
}

const mockDescription = {
  short: 'Lorem ipsum dolor sit amet',
  long: 'Lorem ipsum dolor sit amet, <p>consectetur adipiscing elit</p>, sed do eiusmod <i>tempor</i> incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.',
}

export namespace Mock {
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

  export const BitcoinDep: T.DependencyMetadata = {
    title: 'Bitcoin',
    icon: BTC_ICON,
    optional: false,
    // Localized (Record) form — mirrors what live registries return and
    // exercises the LocaleString branch that used to render "[object Object]".
    description: {
      en_US: 'Needed to run',
      de_DE: 'Zum Ausführen erforderlich',
    },
  }

  export const ProxyDep: T.DependencyMetadata = {
    title: 'Bitcoin Proxy',
    icon: PROXY_ICON,
    optional: true,
    description: 'Needed to run',
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
          hardwareVirtualization: false,
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
          hardwareVirtualization: false,
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
          hardwareVirtualization: false,
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
          hardwareVirtualization: false,
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
          hardwareVirtualization: false,
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
}
