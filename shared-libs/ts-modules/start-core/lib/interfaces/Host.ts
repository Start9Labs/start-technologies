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
