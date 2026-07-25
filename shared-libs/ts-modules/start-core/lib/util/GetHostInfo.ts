import { Effects } from '../Effects'
import { HostId, PackageId } from '../osBindings'
import { deepEqual } from './deepEqual'
import { fillHost, filledAddress, FilledHost } from './filledAddress'
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

/**
 * Reactive reader for the bridge address (`10.0.3.1:<port>`) a dependency's
 * binding is reachable at from another container.
 *
 * Resolves the binding's own derived address rather than `net.assignedPort` /
 * `net.assignedSslPort`: which of those are populated is a property of how the
 * *dependency* bound the port — `assignedSslPort` carries a port that speaks
 * TLS, whether StartOS terminates it (`addSsl`) or the container serves its own
 * certificate, `assignedPort` a plaintext one, and an `http`/`ws` binding
 * (`secure: null` plus a generated `addSsl`) carries *both*. Reading either
 * directly asserts whether a dependency's port speaks TLS and resolves `null`
 * the day that changes.
 *
 * Works for a binding with no exported interface, so it also resolves
 * bridge-only ports such as tor's SOCKS proxy.
 *
 * @param opts.ssl - Required for a binding that publishes both a plaintext and
 * a TLS address (`protocol: 'http'`/`'ws'`, or `secure: null` with `addSsl`).
 * The filter is a no-op when omitted, so the lookup takes whichever leg sorts
 * first rather than the one you meant. Omit it for a single-leg binding, where
 * passing the wrong value resolves `null` — a TLS-passthrough binding
 * (`secure: {ssl: true}` with `addSsl: null`, e.g. LND's gRPC) publishes its one
 * address flagged `ssl: true`, so `ssl: false` there resolves nothing.
 * @param opts.fallbackPort - Resolve to `<osIp>:<fallbackPort>` instead of
 * `null` while the dependency is absent. Only for a flag that should be passed
 * unconditionally against an allocator-guaranteed port, such as tor's 9050.
 */
export class GetBridgeAddress<
  A extends string | null = string | null,
> extends Watchable<A> {
  protected readonly label = 'GetBridgeAddress'

  constructor(
    effects: Effects,
    readonly opts: {
      hostId: HostId
      packageId?: PackageId
      internalPort: number
      ssl?: boolean
      fallbackPort?: number
    },
  ) {
    super(effects)
  }

  protected async fetch(callback?: () => void): Promise<A> {
    const { hostId, packageId, internalPort, ssl, fallbackPort } = this.opts
    const host = await this.effects.getHostInfo({ hostId, packageId, callback })
    const addr =
      host &&
      filledAddress(host, {
        hostId,
        internalPort,
        username: null,
        scheme: null,
        sslScheme: null,
        suffix: '',
      }).bridge.filter({
        kind: 'ipv4',
        predicate: a => ssl === undefined || a.ssl === ssl,
      }).hostnames[0]
    if (addr?.port != null) return `${addr.hostname}:${addr.port}` as A
    if (fallbackPort === undefined) return null as A
    return `${await this.effects.getOsIp()}:${fallbackPort}` as A
  }
}

/** @see {@link GetBridgeAddress} */
export function getBridgeAddress(
  effects: Effects,
  opts: {
    hostId: HostId
    packageId?: PackageId
    internalPort: number
    ssl?: boolean
    fallbackPort: number
  },
): GetBridgeAddress<string>
export function getBridgeAddress(
  effects: Effects,
  opts: {
    hostId: HostId
    packageId?: PackageId
    internalPort: number
    ssl?: boolean
    fallbackPort?: undefined
  },
): GetBridgeAddress<string | null>
export function getBridgeAddress(
  effects: Effects,
  opts: {
    hostId: HostId
    packageId?: PackageId
    internalPort: number
    ssl?: boolean
    fallbackPort?: number
  },
): GetBridgeAddress<string> | GetBridgeAddress<string | null> {
  return new GetBridgeAddress(effects, opts)
}
