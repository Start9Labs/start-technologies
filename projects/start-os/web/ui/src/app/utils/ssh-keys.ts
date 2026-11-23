import { T } from '@start9labs/start-core'

// Stored lines retain `authorized_keys` options, including quoted spaces.
/** Keys in the order they were added. */
export function sshKeyRows(keys: T.SshKeys): T.SshKeyResponse[] {
  return Object.entries(keys)
    .sort(
      ([aPrint, a], [bPrint, b]) =>
        a.created_at.localeCompare(b.created_at) ||
        aPrint.localeCompare(bPrint),
    )
    .map(([fingerprint, key]) => {
      const [, alg = '', hostname = ''] =
        key.value.match(
          /^(?:(?:[^\s"\\]|\\.|"(?:[^"\\]|\\.)*")+\s+)*?((?:sk-)?(?:ssh-|ecdsa-)\S+)\s+[A-Za-z0-9+/]+={0,2}(?:\s(.*))?$/s,
        ) ?? []

      return {
        fingerprint,
        alg,
        hostname,
        createdAt: key.created_at,
      }
    })
}
