import { ValidatorFn } from '@angular/forms'
import {
  WwanPriority,
  WwanSecurity,
  WwanStopReason,
} from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { i18nKey } from 'src/app/i18n/i18n.providers'

export const BAND_LABELS: Record<string, i18nKey> = {
  '2g': '2.4 GHz',
  '5g': '5 GHz',
}

export const PRIORITY_LABELS: Record<WwanPriority, i18nKey> = {
  backup: 'Backup: used while the WAN port has no connection',
  preferred:
    'Preferred: used while connected, with the WAN port taking over when it drops',
}

export const STOP_MESSAGES: Record<WwanStopReason, i18nKey> = {
  rejected:
    '{ssid} didn’t accept the router, so the router stopped trying to join it. Check the password.',
  unreachable:
    'The router couldn’t find {ssid}, so it stopped trying to join it. Check that the network is on and in range.',
  disabled: 'The uplink to {ssid} is turned off.',
}

/** `ErrorKind` codes in backend/ctrl/src/error.rs. */
const ERROR_MESSAGES: Record<number, i18nKey> = {
  1027: 'This band carries the Wi-Fi uplink. Remove the uplink under WAN › Wireless WAN before turning the band off.',
  1028: 'A Wi-Fi uplink was set up in LuCI. Manage it there, or delete it there to manage the uplink here.',
  1029: 'Wi-Fi on this band is off. Turn it on to join a network.',
}

/** Rethrows a failed RPC, translated where its code has a message. */
export async function translateUplinkErrors<T>(
  i18n: i18nPipe,
  request: () => Promise<T>,
): Promise<T> {
  try {
    return await request()
  } catch (e) {
    const message = ERROR_MESSAGES[(e as { code?: number } | null)?.code ?? -1]
    throw message ? new Error(i18n.transform(message)) : e
  }
}

const SECURITY_LABELS: Record<Exclude<WwanSecurity, 'open'>, string> = {
  wpa2: 'WPA2',
  wpa2Wpa3: 'WPA2/WPA3',
  wpa3: 'WPA3',
}

export function securityLabel(i18n: i18nPipe, security: WwanSecurity): string {
  return security === 'open'
    ? i18n.transform('Open (no password)')
    : SECURITY_LABELS[security]
}

const encoder = new TextEncoder()

/** Bounds the UTF-8 length. An empty value passes. */
export function byteLength(min: number, max: number): ValidatorFn {
  return ({ value }) => {
    const { length } = encoder.encode(value)
    if (!length) return null
    if (length < min) return { minBytes: true }
    return length > max ? { maxBytes: true } : null
  }
}

export const noControlCharacters: ValidatorFn = ({ value }) =>
  /\p{Cc}/u.test(value) ? { control: true } : null
