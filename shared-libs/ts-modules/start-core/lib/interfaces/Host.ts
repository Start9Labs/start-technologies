import { z } from '../zExport'
import { Effects } from '../Effects'
import { Origin } from './Origin'
import { RangeOrigin } from './RangeOrigin'
import { AddSslOptions } from '../osBindings'
import { Security } from '../osBindings'
import { BindOptions } from '../osBindings'
import { AlpnInfo } from '../osBindings'
import { ProxyAuth } from '../osBindings'
import { BasicCredential } from '../osBindings'
import { UpstreamCertValidation } from '../osBindings'

export {
  AddSslOptions,
  Security,
  BindOptions,
  ProxyAuth,
  BasicCredential,
  UpstreamCertValidation,
}

export const knownProtocols = {
  http: {
    secure: null,
    defaultPort: 80,
    withSsl: 'https',
    alpn: { specified: ['http/1.1'] } as AlpnInfo,
    addXForwardedHeaders: true,
  },
  https: {
    secure: { ssl: true },
    defaultPort: 443,
    addXForwardedHeaders: true,
  },
  ws: {
    secure: null,
    defaultPort: 80,
    withSsl: 'wss',
    alpn: { specified: ['http/1.1'] } as AlpnInfo,
    addXForwardedHeaders: true,
  },
  wss: {
    secure: { ssl: true },
    defaultPort: 443,
    addXForwardedHeaders: true,
  },
  ssh: {
    secure: { ssl: false },
    defaultPort: 22,
    addXForwardedHeaders: false,
  },
  dns: {
    secure: { ssl: false },
    defaultPort: 53,
    addXForwardedHeaders: false,
  },
} as const

export type Scheme = string | null

type KnownProtocols = typeof knownProtocols
type ProtocolsWithSslVariants = {
  [K in keyof KnownProtocols]: KnownProtocols[K] extends {
    withSsl: string
  }
    ? K
    : never
}[keyof KnownProtocols]
type NotProtocolsWithSslVariants = Exclude<
  keyof KnownProtocols,
  ProtocolsWithSslVariants
>

type BindOptionsByKnownProtocol =
  | {
      protocol: ProtocolsWithSslVariants
      preferredExternalPort?: number
      addSsl?: Partial<AddSslOptions>
    }
  | {
      protocol: NotProtocolsWithSslVariants
      preferredExternalPort?: number
      addSsl?: Partial<AddSslOptions>
    }
export type BindOptionsByProtocol =
  | BindOptionsByKnownProtocol
  | (BindOptions & { protocol: null })

const hasStringProtocol = (v: unknown): v is { protocol: string } =>
  z.object({ protocol: z.string() }).safeParse(v).success

/**
 * Hard cap on how many ports a single {@link MultiHost.bindPortRange} call
 * can claim. Mirrors `MAX_BIND_PORT_RANGE_SIZE` in StartOS's bind effect.
 */
export const MAX_BIND_PORT_RANGE_SIZE = 500

export type BindPortRangeOptions = {
  /** First internal (container-side) port in the range. */
  internalStartPort: number
  /**
   * First external (host-side / WAN) port in the range. May differ from
   * `internalStartPort`: the forward maps the external range onto the
   * internal range by offset (`externalStartPort + i` → `internalStartPort
   * + i`) via an nft verdict map. Use the same value for the common
   * port-preserving case.
   */
  externalStartPort: number
  /**
   * Number of contiguous ports in the range. Must be in `[2, 500]`. For a
   * single port use {@link MultiHost.bindPort}.
   */
  numberOfPorts: number
}

export class MultiHost {
  constructor(
    readonly options: {
      effects: Effects
      id: string
    },
  ) {}

  /**
   * @description Use this function to bind the host to an internal port and configured options for protocol, security, and external port.
   *
   * @param internalPort - The internal port to be bound.
   * @param options - The protocol options for this binding.
   * @returns A multi-origin that is capable of exporting one or more service interfaces.
   * @example
   * In this example, we bind a previously created multi-host to port 80, then select the http protocol and request an external port of 8332.
   *
   * ```
    const uiMultiOrigin = await uiMulti.bindPort(80, {
      protocol: 'http',
      preferredExternalPort: 8332,
    })
   * ```
   */
  async bindPort(
    internalPort: number,
    options: BindOptionsByProtocol,
  ): Promise<Origin> {
    if (hasStringProtocol(options)) {
      return await this.bindPortForKnown(options, internalPort)
    } else {
      return await this.bindPortForUnknown(internalPort, options)
    }
  }

  /**
   * Bind a contiguous range of UDP+TCP ports to this host.
   *
   * Intended for real-time / WebRTC servers (coturn, RTP, SIP) and other
   * pooled-port protocols (bitcoin ZMQ, FTP data) that need a public port
   * range. The whole range is allocated atomically; any partial collision
   * with already-bound external ports is a hard error.
   *
   * `externalStartPort` may differ from `internalStartPort` — the forward
   * maps the external range onto the internal range by offset.
   *
   * Constraints:
   * - `numberOfPorts` must be in `[2, {@link MAX_BIND_PORT_RANGE_SIZE}]`. For
   *   a single port use {@link MultiHost.bindPort}.
   * - All `numberOfPorts` external ports starting at `externalStartPort`
   *   must be free and non-restricted.
   *
   * Returns a {@link RangeOrigin}, on which you call `.export(builder)` with a
   * `sdk.createRangeInterface(...)` builder to register the range's single,
   * restricted `api` service interface (no SSL, no path/query/auth).
   *
   * @example
   * ```
   * const turn = await sdk.MultiHost.of(effects, 'turn-relay').bindPortRange({
   *   internalStartPort: 49152,
   *   externalStartPort: 49152,
   *   numberOfPorts: 100,
   * })
   * await turn.export(
   *   sdk.createRangeInterface(effects, {
   *     id: 'turn-relay',
   *     name: 'TURN Relay',
   *     description: 'WebRTC media relay ports',
   *   }),
   * )
   * ```
   */
  async bindPortRange(options: BindPortRangeOptions): Promise<RangeOrigin> {
    const { internalStartPort, externalStartPort, numberOfPorts } = options
    if (!Number.isInteger(numberOfPorts) || numberOfPorts < 2) {
      throw new Error(
        `numberOfPorts must be an integer >= 2; use bindPort for a single port`,
      )
    }
    if (numberOfPorts > MAX_BIND_PORT_RANGE_SIZE) {
      throw new Error(
        `numberOfPorts (${numberOfPorts}) exceeds maximum (${MAX_BIND_PORT_RANGE_SIZE})`,
      )
    }
    // Both ranges must be in-bounds and above the reserved/privileged range.
    // StartOS additionally rejects a few specific ports (e.g. 5353, 5432,
    // 6010) server-side at allocation time.
    for (const [name, start] of [
      ['internalStartPort', internalStartPort],
      ['externalStartPort', externalStartPort],
    ] as const) {
      if (!Number.isInteger(start) || start <= 1024) {
        throw new Error(
          `${name} (${start}) must be an integer greater than 1024; ports <= 1024 are reserved`,
        )
      }
      if (start + numberOfPorts - 1 > 65535) {
        throw new Error(
          `${name} range [${start}, ${start + numberOfPorts - 1}] is out of bounds`,
        )
      }
    }
    await this.options.effects.bindRange({
      id: this.options.id,
      internalStartPort,
      externalStartPort,
      numberOfPorts,
    })

    return new RangeOrigin(
      this,
      internalStartPort,
      externalStartPort,
      numberOfPorts,
    )
  }

  /**
   * Permanently remove this host and everything under it.
   *
   * This undoes the host itself, not a `setupInterfaces` pass. That pass ends
   * by *disabling* every binding it did not just declare, which keeps the row,
   * its external port claim and the user's per-address choices, so an address
   * survives a pass that omitted it. Retiring deletes:
   *
   * - every binding and port range on this host, and their service interfaces
   * - the user's public and private domains for this host
   * - the user's per-address enable/disable and WAN opt-in choices
   * - the external port reservations, which return to the server's pool
   *
   * Irreversible, and it destroys configuration the user created, so name the
   * host in your release notes when a release retires one. Binding this id
   * again creates a new, empty host with none of that setup.
   *
   * Call it from a migration's `up()`, in the version that stops binding the
   * host. Retiring an id you still bind just recreates it, minus the user's
   * setup.
   *
   * @returns `true` if a host was removed, `false` if there was none — the
   * normal result on a re-run, not an error.
   *
   * @example
   * ```
   * // 2.0 renamed this host to `ui`; drop the 1.x one.
   * await sdk.MultiHost.of(effects, 'ui-multi').retire()
   * ```
   */
  async retire(): Promise<boolean> {
    return this.options.effects.retireHost({ id: this.options.id })
  }

  /**
   * Permanently remove a single binding from this host — the inverse of
   * {@link MultiHost.bindPort} — for a service that dropped one port but kept
   * the rest. Removes whichever of the single port and the port range is bound
   * at `internalPort`, and both if both are.
   *
   * Takes the binding's service interfaces with it and returns its external
   * ports to the server's pool. Binding the port again creates a fresh binding
   * on the surviving host. The host keeps its domains: retiring the last
   * binding does not retire the host, so a domain can be left addressing
   * nothing — retire the host itself if it is going away.
   *
   * @param internalPort - the container-side port passed to
   * {@link MultiHost.bindPort}, or the `internalStartPort` passed to
   * {@link MultiHost.bindPortRange}
   * @returns `true` if something was removed, `false` if nothing was bound
   * there — the normal result on a re-run, not an error.
   *
   * @example
   * ```
   * // upstream dropped the bundled metrics listener in 3.0
   * await sdk.MultiHost.of(effects, 'api').retirePort(9090)
   * ```
   */
  async retirePort(internalPort: number): Promise<boolean> {
    return this.options.effects.retireBinding({
      id: this.options.id,
      internalPort,
    })
  }

  private async bindPortForUnknown(
    internalPort: number,
    options: {
      preferredExternalPort: number
      addSsl: AddSslOptions | null
      secure: { ssl: boolean } | null
    },
  ) {
    const binderOptions = {
      id: this.options.id,
      internalPort,
      ...options,
    }
    await this.options.effects.bind(binderOptions)

    return new Origin(this, internalPort, null, null)
  }

  private async bindPortForKnown(
    options: BindOptionsByKnownProtocol,
    internalPort: number,
  ) {
    const protoInfo = knownProtocols[options.protocol]
    const preferredExternalPort =
      options.preferredExternalPort ||
      knownProtocols[options.protocol].defaultPort
    const sslProto = this.getSslProto(options)
    const addSsl = sslProto
      ? {
          addXForwardedHeaders: knownProtocols[sslProto].addXForwardedHeaders,
          preferredExternalPort: knownProtocols[sslProto].defaultPort,
          scheme: sslProto,
          alpn: 'alpn' in protoInfo ? protoInfo.alpn : null,
          auth: null as ProxyAuth | null,
          ...('addSsl' in options ? options.addSsl : null),
        }
      : options.addSsl
        ? {
            addXForwardedHeaders: false,
            preferredExternalPort: 443,
            scheme: sslProto,
            alpn: null,
            auth: null as ProxyAuth | null,
            ...options.addSsl,
          }
        : null

    const secure: Security | null = protoInfo.secure ?? null

    await this.options.effects.bind({
      id: this.options.id,
      internalPort,
      preferredExternalPort,
      addSsl,
      secure,
    })

    return new Origin(this, internalPort, options.protocol, sslProto)
  }

  private getSslProto(options: BindOptionsByKnownProtocol) {
    const proto = options.protocol
    const protoInfo = knownProtocols[proto]
    if (inObject('noAddSsl', options) && options.noAddSsl) return null
    if ('withSsl' in protoInfo && protoInfo.withSsl) return protoInfo.withSsl
    if (protoInfo.secure?.ssl) return proto
    return null
  }
}

function inObject<Key extends string>(
  key: Key,
  obj: any,
): obj is { [K in Key]: unknown } {
  return key in obj
}
