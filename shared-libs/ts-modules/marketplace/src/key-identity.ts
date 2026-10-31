import { Pipe } from '@angular/core'
import { sha256 } from '@noble/hashes/sha256'
import { auth, T } from '@start9labs/start-core'

export type IdentityKey = Pick<
  T.TrustedKeyring,
  'name' | 'publicKeys' | 'gpgExports'
>

/** Fingerprints use OpenPGP packets or SHA-256 over canonical SPKI. */
export function signingKeyIdentity(publicKey: string, key?: IdentityKey) {
  const canonical = auth.normalizeDer64(publicKey)
  const userIds = new Set(
    key?.gpgExports.flatMap(exported => exported.identities[canonical] ?? []),
  )
  const names = new Set<string>()
  const emails = new Set<string>()
  for (const userId of userIds) {
    const match = /^(.*?)\s*<([^<>]+)>$/.exec(userId)
    if (match) {
      if (match[1]) names.add(match[1])
      emails.add(match[2]!)
    } else {
      names.add(userId)
    }
  }
  const fingerprints = new Set(
    key?.gpgExports.flatMap(
      exported => exported.keyFingerprints[canonical] ?? [],
    ),
  )
  return {
    name: [...names].join(', ') || key?.name || null,
    emails: [...emails],
    fingerprints: fingerprints.size
      ? [...fingerprints].map(value =>
          value
            .toUpperCase()
            .match(/.{1,4}/g)!
            .join(' '),
        )
      : [
          `SHA256:${auth.bytesToBase64(sha256(auth.base64ToBytes(canonical))).replace(/=+$/, '')}`,
        ],
  }
}

@Pipe({ name: 'keyIdentity' })
export class KeyIdentityPipe {
  transform(key: IdentityKey) {
    return signingKeyIdentity(key.publicKeys[0]!, key)
  }
}
