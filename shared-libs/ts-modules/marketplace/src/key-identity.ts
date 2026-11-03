import { Pipe } from '@angular/core'
import { sha256 } from '@noble/hashes/sha256'
import { auth, T } from '@start9labs/start-core'

export type IdentityKey = Pick<
  T.TrustedKey,
  'name' | 'publicKey' | 'gpgExports' | 'contact'
>

export function registrySigningKeys(
  info: T.RegistryInfo,
): Record<string, IdentityKey> {
  return Object.fromEntries(
    Object.values(info.trustSigners).flatMap(signer =>
      signer.keys.map(id => [
        id,
        {
          name: signer.name,
          publicKey: id,
          gpgExports: signer.gpgExports,
          contact: signer.contact,
        },
      ]),
    ),
  )
}

export function keyType({
  gpgExports,
}: Pick<T.TrustedKey, 'gpgExports'>): 'GPG' | 'PEM' {
  return gpgExports.length ? 'GPG' : 'PEM'
}

/** GPG user IDs, or the email addresses registries publish for a plain key. */
export function keyIdentities(key: IdentityKey): string[] {
  const canonical = auth.normalizeDer64(key.publicKey)
  const userIds = key.gpgExports.flatMap(
    exported => exported.identities[canonical] ?? [],
  )

  return [
    ...new Set(
      userIds.length
        ? userIds
        : key.contact.flatMap(contact =>
            'email' in contact ? [contact.email] : [],
          ),
    ),
  ]
}

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
  for (const contact of key?.contact ?? []) {
    if ('email' in contact) emails.add(contact.email)
  }
  const fingerprints = new Set(
    key?.gpgExports.flatMap(
      exported => exported.keyFingerprints[canonical] ?? [],
    ),
  )
  return {
    name: key?.name || [...names].join(', ') || null,
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
    return signingKeyIdentity(key.publicKey, key)
  }
}
