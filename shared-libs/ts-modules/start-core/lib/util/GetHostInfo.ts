import { Effects } from '../Effects'
import { HostId, PackageId } from '../osBindings'
import { deepEqual } from './deepEqual'
import { fillHost, FilledHost } from './filledAddress'
import { Watchable } from './Watchable'

export class GetHostInfo<Mapped = FilledHost | null> extends Watchable<
  FilledHost | null,
  Mapped
> {
  protected readonly label = 'GetHostInfo'

  constructor(
    effects: Effects,
    readonly opts: { hostId: HostId; packageId?: PackageId },
    options?: {
      map?: (value: FilledHost | null) => Mapped
      eq?: (a: Mapped, b: Mapped) => boolean
    },
  ) {
    super(effects, options)
  }

  protected async fetch(callback?: () => void): Promise<FilledHost | null> {
    const host = await this.effects.getHostInfo({ ...this.opts, callback })
    return host && fillHost(host)
  }
}

/**
 * Reactive reader for one of this package's own hosts.
 *
 * Pass `map` to react to only a slice of the host: `const()` re-runs the
 * calling context when the mapped value changes (compared with `eq`, default
 * deep-equal) rather than on any change to the whole host. Reach an exported
 * interface by walking the host — e.g.
 * `map: h => h?.bindings[80]?.interfaces['ui']`. The host's interface
 * `addressInfo`s come pre-filled, carrying the `filter`/`format`/`nonLocal`/
 * `public`/`toUrl` helpers.
 */
export function getOwnHost(effects: Effects, hostId: HostId): GetHostInfo
export function getOwnHost<Mapped>(
  effects: Effects,
  hostId: HostId,
  map: (host: FilledHost | null) => Mapped,
  eq?: (a: Mapped, b: Mapped) => boolean,
): GetHostInfo<Mapped>
export function getOwnHost<Mapped>(
  effects: Effects,
  hostId: HostId,
  map?: (host: FilledHost | null) => Mapped,
  eq?: (a: Mapped, b: Mapped) => boolean,
): GetHostInfo<Mapped> {
  return new GetHostInfo<Mapped>(
    effects,
    { hostId },
    {
      map: map ?? (a => a as Mapped),
      eq: eq ?? ((a, b) => deepEqual(a, b)),
    },
  )
}

/**
 * Reactive reader for a host on any package (defaults to this package when
 * `packageId` is omitted). Pass `map`/`eq` to narrow `const()` reactivity to a
 * slice of the host — see {@link getOwnHost}.
 */
export function getHost(
  effects: Effects,
  opts: { hostId: HostId; packageId?: PackageId },
): GetHostInfo
export function getHost<Mapped>(
  effects: Effects,
  opts: { hostId: HostId; packageId?: PackageId },
  map: (host: FilledHost | null) => Mapped,
  eq?: (a: Mapped, b: Mapped) => boolean,
): GetHostInfo<Mapped>
export function getHost<Mapped>(
  effects: Effects,
  opts: { hostId: HostId; packageId?: PackageId },
  map?: (host: FilledHost | null) => Mapped,
  eq?: (a: Mapped, b: Mapped) => boolean,
): GetHostInfo<Mapped> {
  return new GetHostInfo<Mapped>(effects, opts, {
    map: map ?? (a => a as Mapped),
    eq: eq ?? ((a, b) => deepEqual(a, b)),
  })
}
