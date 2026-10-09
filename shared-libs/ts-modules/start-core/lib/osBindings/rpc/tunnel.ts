export type RpcHandler = {
  _PARAMS: unknown
  _RETURN?: unknown
  _CHILDREN?: { [name: string]: RpcHandler }
}

export type RpcMethod<Root extends RpcHandler> =
  | (Root extends { _RETURN: unknown } ? '' : never)
  | (Root extends { _CHILDREN: infer Children }
      ? {
          [Name in keyof Children & string]: Children[Name] extends RpcHandler
            ? RpcMethod<Children[Name]> extends infer ChildMethod extends string
              ? ChildMethod extends ''
                ? Name
                : `${Name}.${ChildMethod}`
              : never
            : never
        }[keyof Children & string]
      : never)

export type RpcParamType<
  Root extends RpcHandler,
  Method extends string,
> = Method extends ''
  ? Root extends { _RETURN: unknown }
    ? Root['_PARAMS']
    : never
  : Root extends { _CHILDREN: infer Children }
    ? Method extends `${infer Head}.${infer Tail}`
      ? Head extends keyof Children
        ? Children[Head] extends RpcHandler
          ? Root['_PARAMS'] & RpcParamType<Children[Head], Tail>
          : never
        : never
      : Method extends keyof Children
        ? Children[Method] extends RpcHandler
          ? Root['_PARAMS'] & RpcParamType<Children[Method], ''>
          : never
        : never
    : never

export type RpcReturnType<
  Root extends RpcHandler,
  Method extends string,
> = Method extends ''
  ? Root extends { _RETURN: infer Return }
    ? Return
    : never
  : Root extends { _CHILDREN: infer Children }
    ? Method extends `${infer Head}.${infer Tail}`
      ? Head extends keyof Children
        ? Children[Head] extends RpcHandler
          ? RpcReturnType<Children[Head], Tail>
          : never
        : never
      : Method extends keyof Children
        ? Children[Method] extends RpcHandler
          ? RpcReturnType<Children[Method], ''>
          : never
        : never
    : never

export type AddDeviceParamsInput = {
  subnet: string
  name: string
  ip?: string | null
  /**
   * Client (no autoconfig) or Server (gateway-autoconfig on by default).
   */
  kind?: WgClientKindInput
}
export type AddDnsRecordParamsInput = {
  name: string
  type: string
  value: string
  ttl?: number | null
}
export type AddKeyParamsInput = { name: string; key: AnyVerifyingKeyInput }
export type AddPinholeParamsInput = {
  /**
   * The client's global IPv6 (GUA) to expose. Must be an address this tunnel
   * delegates to a client — its subnet needs an IPv6 prefix.
   */
  gua: string
  /**
   * External port opened on the GUA.
   */
  externalPort: number
  /**
   * Destination port on the GUA. Omit for a pure pinhole (no NAT, internal ==
   * external); set a different value for a port remap (e.g. 80 -> 443).
   */
  internalPort?: number | null
  label?: string | null
  /**
   * Number of contiguous ports, counting up from external/internal. Default 1.
   */
  count?: number | null
}
export type AddPortForwardParamsInput = {
  /**
   * External (WAN) port to forward. The external IP is fixed to the target's
   * WAN so return traffic stays symmetric.
   */
  externalPort: number
  target: string
  label?: string | null
  /**
   * Hostnames to SNI-demux on the shared external port. Empty = normal DNAT.
   */
  sni?: string[]
  /**
   * Number of contiguous ports to forward (a PCP PORT_SET range), counting up
   * from both `external_port` and the target port. Defaults to 1. Not valid
   * together with SNI demux.
   */
  count?: number | null
}
export type AddSubnetParamsInput = { name: string }
export type AnyVerifyingKeyInput = string
export type Api = {
  _PARAMS: {}
  _CHILDREN: {
    auth: {
      _PARAMS: {}
      _CHILDREN: {
        key: {
          _PARAMS: {}
          _CHILDREN: {
            add: { _PARAMS: AddKeyParamsInput; _RETURN: null }
            list: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
              _RETURN: AuthKeys
            }
            remove: { _PARAMS: RemoveKeyParamsInput; _RETURN: null }
          }
        }
        login: { _PARAMS: LoginParamsInput; _RETURN: null }
        logout: {
          _PARAMS: LogoutParamsInput
          _RETURN: HasUnenrolledKeys | null
        }
        session: {
          _PARAMS: {}
          _CHILDREN: {
            kill: { _PARAMS: KillParamsInput; _RETURN: null }
            list: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                ListParamsInput
              _RETURN: SessionList
            }
          }
        }
        'set-password': { _PARAMS: SetPasswordParamsInput; _RETURN: null }
      }
    }
    db: {
      _PARAMS: {}
      _CHILDREN: {
        apply: { _PARAMS: ApplyWithPathParamsInput; _RETURN: null }
        dump: { _PARAMS: DumpParamsInput; _RETURN: Dump }
        subscribe: { _PARAMS: SubscribeParamsInput; _RETURN: SubscribeRes }
      }
    }
    device: {
      _PARAMS: {}
      _CHILDREN: {
        add: { _PARAMS: AddDeviceParamsInput; _RETURN: null }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            ListDevicesParamsInput
          _RETURN: WgSubnetConfig
        }
        remove: { _PARAMS: RemoveDeviceParamsInput; _RETURN: null }
        'set-auto-port-forward': {
          _PARAMS: SetAutoPortForwardParamsInput
          _RETURN: null
        }
        'set-dns-injection': {
          _PARAMS: SetDnsInjectionParamsInput
          _RETURN: null
        }
        'set-kind': { _PARAMS: SetDeviceKindParamsInput; _RETURN: null }
        'set-wan': { _PARAMS: SetDeviceWanParamsInput; _RETURN: null }
        'show-config': { _PARAMS: ShowConfigParamsInput; _RETURN: string }
      }
    }
    dns: {
      _PARAMS: {}
      _CHILDREN: {
        add: { _PARAMS: AddDnsRecordParamsInput; _RETURN: null }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: DnsRecordEntry[]
        }
        remove: { _PARAMS: RemoveDnsRecordParamsInput; _RETURN: null }
      }
    }
    'http-redirect': {
      _PARAMS: {}
      _CHILDREN: {
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: HttpRedirectStatus[]
        }
        'set-enabled': {
          _PARAMS: SetHttpRedirectEnabledParamsInput
          _RETURN: null
        }
      }
    }
    pinhole: {
      _PARAMS: {}
      _CHILDREN: {
        add: { _PARAMS: AddPinholeParamsInput; _RETURN: null }
        remove: { _PARAMS: RemovePinholeParamsInput; _RETURN: null }
        'set-enabled': { _PARAMS: SetPinholeEnabledParamsInput; _RETURN: null }
        'update-label': {
          _PARAMS: UpdatePinholeLabelParamsInput
          _RETURN: null
        }
      }
    }
    'port-forward': {
      _PARAMS: {}
      _CHILDREN: {
        add: { _PARAMS: AddPortForwardParamsInput; _RETURN: null }
        remove: { _PARAMS: RemovePortForwardParamsInput; _RETURN: null }
        'set-enabled': {
          _PARAMS: SetPortForwardEnabledParamsInput
          _RETURN: null
        }
        'update-label': {
          _PARAMS: UpdatePortForwardLabelParamsInput
          _RETURN: null
        }
      }
    }
    restart: { _PARAMS: {}; _RETURN: null }
    subnet: {
      _PARAMS: SubnetParamsInput
      _CHILDREN: {
        add: { _PARAMS: AddSubnetParamsInput; _RETURN: null }
        remove: { _PARAMS: {}; _RETURN: null }
        'set-dns': { _PARAMS: SetSubnetDnsParamsInput; _RETURN: null }
        'set-ipv6': { _PARAMS: SetSubnetIpv6ParamsInput; _RETURN: null }
        'set-wan': { _PARAMS: SetSubnetWanParamsInput; _RETURN: null }
      }
    }
    update: {
      _PARAMS: {}
      _CHILDREN: {
        apply: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: TunnelUpdateResult
        }
        check: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: TunnelUpdateResult
        }
      }
    }
    web: {
      _PARAMS: {}
      _CHILDREN: {
        disable: { _PARAMS: {}; _RETURN: null }
        enable: { _PARAMS: {}; _RETURN: null }
        'generate-certificate': {
          _PARAMS: GenerateCertParamsInput
          _RETURN: string
        }
        'get-available-ips': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: string[]
        }
        'get-certificate': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: string | null
        }
        'get-listen': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: string | null
        }
        'import-certificate': { _PARAMS: TunnelCertDataInput; _RETURN: null }
        'set-listen': { _PARAMS: SetListenParamsInput; _RETURN: null }
        uninit: { _PARAMS: {}; _RETURN: null }
      }
    }
  }
}
export type ApplyParamsInput = { expr: string }
export type ApplyWithPathParamsInput = {
  path?: string | null
} & ApplyParamsInput

/**
 * The server's enrolled auth keys, keyed by their PEM encoding. Each enrolled
 * key is a sign-in: it carries the same metadata a session used to (when it
 * was created, when it was last used, and the user agent that enrolled it).
 */
export type AuthKeys = { [key: string]: Session }

/**
 * Per-subnet DNS proxy configuration. WireGuard clients on a subnet are pointed
 * at the subnet's in-tunnel server address (`.1`) for DNS; the forward-only proxy
 * listening there resolves every query against the upstream(s) selected here.
 */
export type DnsConfig =
  | {
      /**
       * Forward to the VPS's own system resolvers.
       */
      type: 'default'
    }
  | ({
      /**
       * Forward to a device on this subnet (its WireGuard IP) on port 53.
       */
      type: 'device'
    } & { ip: string })
  | ({
      /**
       * Forward to operator-specified upstream servers (1-3, optional `:port`).
       */
      type: 'custom'
    } & { servers: string[] })

/**
 * Which upstream a subnet's DNS proxy forwards to. `Device`/`Custom` draw
 * their data from companion fields on [`SetSubnetDnsParams`].
 */
export type DnsModeInput = 'default' | 'device' | 'custom'

/**
 * A DNS record served by the tunnel (injected via RFC 2136 or added manually).
 * `value` is the rdata as text: an IP for A/AAAA, a name for CNAME, etc.
 */
export type DnsRecordEntry = {
  name: string
  type: string
  value: string
  ttl: number
  /**
   * The device IP that injected this, or `null` for a manual record.
   */
  source: string | null
}
export type Dump = { id: number; value: unknown }
export type DumpParamsInput = { pointer?: string | null }
export type GenerateCertParamsInput = { subject: string[] }
export type Guid = string

/**
 * Proof that a set of auth keys was unenrolled — removed from the persisted
 * and ephemeral signer stores with any continuations they opened killed.
 * Obtained via [`SignatureAuthContext::unenroll`], or [`Self::unenroll`]
 * from inside a db transaction.
 */
export type HasUnenrolledKeys = null
export type HttpRedirectStatus = {
  ip: string
  /**
   * Whether the redirect is on for this IP (default true).
   */
  enabled: boolean
  /**
   * Whether a port-forward already occupies port 80 on this IP, in which case
   * the redirect yields and does not bind.
   */
  forwarded: boolean
}
export type IoFormatInput =
  | 'json'
  | 'json-pretty'
  | 'yaml'
  | 'cbor'
  | 'toml'
  | 'toml-pretty'
export type KillParamsInput = { ids: string[] }
export type ListDevicesParamsInput = { subnet: string }
export type ListParamsInput = {}
export type LoginParamsInput = {
  password: string
  /**
   * The PEM-encoded public key to enroll on a successful login. The login
   * request itself is signed with the matching secret key, so enrollment
   * proves possession.
   */
  pubkey: AnyVerifyingKeyInput
  /**
   * Enroll in memory only, never persisted (kiosk mode, which re-enrolls
   * on every browser restart and would otherwise accumulate keys).
   */
  ephemeral?: boolean
}
export type LogoutParamsInput = {}
export type RemoveDeviceParamsInput = { subnet: string; ip: string }
export type RemoveDnsRecordParamsInput = { name: string; type?: string | null }
export type RemoveKeyParamsInput = { key: AnyVerifyingKeyInput }
export type RemovePinholeParamsInput = { gua: string; externalPort: number }
export type RemovePortForwardParamsInput = {
  source: string
  /**
   * Remove a single SNI route on `source`; omit to remove the whole forward.
   */
  hostname?: string | null
}
export type Session = {
  /**
   * A friendly name for the key, if one was assigned at enrollment (e.g.
   * tunnel device keys). UI-enrolled keys are unnamed.
   */
  name: string | null
  loggedIn: string
  lastActive: string
  userAgent: string | null
}
export type SessionList = { current: string | null; sessions: AuthKeys }
export type SetAutoPortForwardParamsInput = {
  subnet: string
  ip: string
  enabled: boolean
}
export type SetDeviceKindParamsInput = {
  subnet: string
  ip: string
  kind: WgClientKindInput
}
export type SetDeviceWanParamsInput = {
  subnet: string
  ip: string
  wanIp?: string | null
}
export type SetDnsInjectionParamsInput = {
  subnet: string
  ip: string
  enabled: boolean
}
export type SetHttpRedirectEnabledParamsInput = {
  /**
   * The public IPv4 whose default-on port-80 HTTP→HTTPS redirect to toggle.
   */
  ip: string
  enabled: boolean
}
export type SetListenParamsInput = { listen: string }
export type SetPasswordParamsInput = { password: string }
export type SetPinholeEnabledParamsInput = {
  gua: string
  externalPort: number
  enabled: boolean
}
export type SetPortForwardEnabledParamsInput = {
  source: string
  enabled: boolean
  /**
   * Toggle a single SNI route on `source`; omit for a DNAT forward.
   */
  hostname?: string | null
}
export type SetSubnetDnsParamsInput = {
  mode: DnsModeInput
  /**
   * The selected device's WireGuard IP; required when `mode` is `device`.
   */
  deviceIp?: string | null
  /**
   * Up to 3 upstream servers (bare IP or `ip:port`); used when `mode` is `custom`.
   */
  servers: string[]
}
export type SetSubnetIpv6ParamsInput = {
  /**
   * The routed IPv6 prefix delegated to this subnet (e.g. a /64 from Hetzner,
   * a /56 from Linode). `null` disables IPv6 on the subnet.
   */
  prefix?: string | null
}
export type SetSubnetWanParamsInput = { wanIp?: string | null }
export type ShowConfigParamsInput = { subnet: string; ip: string }
export type SubnetParamsInput = { subnet: string }
export type SubscribeParamsInput = { pointer?: string | null }
export type SubscribeRes = { dump: Dump; guid: Guid }
export type TunnelCertDataInput = { key: string; cert: string }
export type TunnelUpdateResult = {
  /**
   * "up-to-date", "update-available", or "updating"
   */
  status: string
  /**
   * Currently installed version
   */
  installed: string
  /**
   * Available candidate version
   */
  candidate: string
}
export type UpdatePinholeLabelParamsInput = {
  gua: string
  externalPort: number
  label?: string | null
}
export type UpdatePortForwardLabelParamsInput = {
  source: string
  label?: string | null
  /**
   * Label a single SNI route on `source`; omit to label the DNAT forward.
   */
  hostname?: string | null
}

/**
 * A WireGuard client's role. A `Server` is a StartOS box that may configure the
 * gateway (DNS injection / auto port-forward); a `Client` is a plain peer with
 * no autoconfig. Stored and sticky — toggling the capability flags never changes
 * it; the migration backfills it from those flags.
 */
export type WgClientKind = 'client' | 'server'

/**
 * A WireGuard client's role. A `Server` is a StartOS box that may configure the
 * gateway (DNS injection / auto port-forward); a `Client` is a plain peer with
 * no autoconfig. Stored and sticky — toggling the capability flags never changes
 * it; the migration backfills it from those flags.
 */
export type WgClientKindInput = 'client' | 'server'
export type WgConfig = {
  name: string
  key: string
  psk: string
  /**
   * Client (no autoconfig) vs Server (a StartOS box). Sticky; defaulted by the
   * migration from the capability flags.
   */
  kind: WgClientKind
  /**
   * Whether this device may inject DNS records via RFC 2136. Off by default
   * — only enable for devices you trust (it lets the device add records to
   * the tunnel's DNS for every peer to resolve).
   */
  allowDnsInjection: boolean
  /**
   * Whether this device may auto-create port forwards via PCP/IGD. Off by
   * default — paired with `allow_dns_injection` under one "Gateway
   * Autoconfiguration" toggle, but tracked separately so each capability is
   * gated on its own.
   */
  allowAutoPortForward: boolean
  /**
   * SNAT this device's egress to this WAN IP, overriding the subnet's
   * `wan_ip` / the default masquerade. `None` falls back to the subnet rule.
   */
  wanIp: string | null
}
export type WgSubnetClients = { [key: string]: WgConfig }
export type WgSubnetConfig = {
  name: string
  clients: WgSubnetClients
  dns: DnsConfig
  /**
   * SNAT this subnet's egress to this WAN IP instead of `masquerade`. `None`
   * keeps the default masquerade; a per-device `wan_ip` overrides this.
   */
  wanIp: string | null
  /**
   * Routed IPv6 prefix delegated to this subnet, if any. Each host on the
   * subnet (the server and every client) gets one `/128` out of it with its
   * tunnel IPv4 embedded — see [`crate::tunnel::wg6`].
   */
  ipv6: string | null
}
