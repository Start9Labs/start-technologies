import { T } from '@start9labs/start-core'

export function sshKeyRows(keys: T.SshKeys): T.SshKeyResponse[] {
  return Object.entries(keys).map(([fingerprint, key]) => {
    const [alg = '', , ...comment] = key.value.split(' ')

    return {
      fingerprint,
      alg,
      hostname: comment.join(' '),
      createdAt: key.created_at,
    }
  })
}
