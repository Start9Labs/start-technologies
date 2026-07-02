/**
 * Timezone helpers.
 *
 * We deliberately keep NO timezone data here. The authoritative IANA → POSIX
 * mapping and the list of selectable zones come from the device's own LuCI
 * zoneinfo table (`system.get-timezones` / backend `resolve_posix_tz`, both
 * backed by `ubus call luci getTimezones`). The browser supplies only the
 * detected zone name; everything below is pure formatting via `Intl`.
 */

/** The browser's current IANA timezone, e.g. "America/Denver" or "UTC". */
export function getBrowserTimezone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone
}

/**
 * Human-readable dropdown label for an IANA name, e.g.
 * "(GMT-6) America/Denver". The offset is computed live via `Intl` so it stays
 * correct under DST without any stored table.
 */
export function getTimezoneLabel(iana: string): string {
  const name = iana.replaceAll('_', ' ')
  try {
    const offset =
      new Intl.DateTimeFormat('en-US', {
        timeZone: iana,
        timeZoneName: 'shortOffset',
      })
        .formatToParts(new Date())
        .find(p => p.type === 'timeZoneName')?.value ?? 'GMT'
    return `(${offset}) ${name}`
  } catch {
    // Browser doesn't recognize this zone (tzdata version skew) — show the name.
    return name
  }
}
