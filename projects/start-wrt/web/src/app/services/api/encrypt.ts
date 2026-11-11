import * as jose from 'node-jose'
import type { EncryptedWire } from './api.service'

export async function encrypt(
  pubkey: object,
  plaintext: string,
): Promise<EncryptedWire> {
  const key = await jose.JWK.asKey(pubkey)
  const encrypted = await jose.JWE.createEncrypt(key).update(plaintext).final()
  return { encrypted }
}
