import {
  WwanPriority,
  WwanSecurity,
  WwanStopReason,
} from 'src/app/services/api/api.service'
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

// Protocol names; only "Open" is translated.
export const SECURITY_LABELS: Record<WwanSecurity, string> = {
  open: 'Open',
  wpa2: 'WPA2',
  wpa2Wpa3: 'WPA2/WPA3',
  wpa3: 'WPA3',
}
