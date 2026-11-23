import { utils } from '@start9labs/start-core'

function byCodeUnits(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0
}

/** Orders IP addresses and CIDR subnets numerically. */
export function byIp(a: string, b: string): number {
  return (
    utils.IpAddress.parse(a.split('/')[0]!).cmp(b.split('/')[0]!) ||
    byCodeUnits(a, b)
  )
}

/** A record's entries ordered by their IP or CIDR keys. */
export function entriesByIp<V>(record: Record<string, V>): [string, V][] {
  return Object.entries(record).sort(([a], [b]) => byIp(a, b))
}

/** A record's entries in the key order the backend's maps iterate in. */
export function entriesByKey<V>(record: Record<string, V>): [string, V][] {
  return Object.entries(record).sort(([a], [b]) => byCodeUnits(a, b))
}
