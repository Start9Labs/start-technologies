import { z } from 'zod'
const wire = <A extends z.ZodType, B extends z.ZodType>(input: A, output: B) =>
  z.codec(input, output, { decode: v => v as never, encode: v => v as never })
const object = <Shape extends z.core.$ZodLooseShape>(shape: Shape) =>
  z.object(shape).meta({ additionalProperties: true })
const empty: z.ZodType<{}, {}> = /* @__PURE__ */ object({})
const literal_0 = /* @__PURE__ */ z.string()
export const AddDnsRecordParams: z.ZodObject<{
  name: z.ZodString
  type: z.ZodString
  value: z.ZodString
  ttl: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
}> = /* @__PURE__ */ object({
  name: z.string(),
  type: z.string(),
  value: z.string(),
  ttl: wire(z.int().nullable().optional(), z.int().nullable()),
}).meta({
  id: 'AddDnsRecordParams',
})
export const AddPinholeParams: z.ZodObject<{
  /**
   * The client's global IPv6 (GUA) to expose. Must be an address this tunnel
   * delegates to a client — its subnet needs an IPv6 prefix.
   */
  gua: z.ZodString
  /**
   * External port opened on the GUA.
   */
  externalPort: z.ZodInt
  /**
   * Destination port on the GUA. Omit for a pure pinhole (no NAT, internal ==
   * external); set a different value for a port remap (e.g. 80 -> 443).
   */
  internalPort: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  label: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Number of contiguous ports, counting up from external/internal. Default 1.
   */
  count: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
}> = /* @__PURE__ */ object({
  /**
   * The client's global IPv6 (GUA) to expose. Must be an address this tunnel
   * delegates to a client — its subnet needs an IPv6 prefix.
   */
  gua: z.string().meta({
    description:
      "The client's global IPv6 (GUA) to expose. Must be an address this tunnel\ndelegates to a client — its subnet needs an IPv6 prefix.",
  }),
  /**
   * External port opened on the GUA.
   */
  externalPort: z.int().meta({
    description: 'External port opened on the GUA.',
  }),
  /**
   * Destination port on the GUA. Omit for a pure pinhole (no NAT, internal ==
   * external); set a different value for a port remap (e.g. 80 -> 443).
   */
  internalPort: wire(z.int().nullable().optional(), z.int().nullable()).meta({
    description:
      'Destination port on the GUA. Omit for a pure pinhole (no NAT, internal ==\nexternal); set a different value for a port remap (e.g. 80 -> 443).',
  }),
  label: wire(z.string().nullable().optional(), z.string().nullable()),
  /**
   * Number of contiguous ports, counting up from external/internal. Default 1.
   */
  count: wire(z.int().nullable().optional(), z.int().nullable()).meta({
    description:
      'Number of contiguous ports, counting up from external/internal. Default 1.',
  }),
}).meta({
  id: 'AddPinholeParams',
})
export const AddPortForwardParams: z.ZodObject<{
  /**
   * External (WAN) port to forward. The external IP is fixed to the target's
   * WAN so return traffic stays symmetric.
   */
  externalPort: z.ZodInt
  target: z.ZodString
  label: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Hostnames to SNI-demux on the shared external port. Empty = normal DNAT.
   */
  sni: z.ZodCodec<
    z.ZodOptional<z.ZodArray<z.ZodString>>,
    z.ZodArray<z.ZodString>
  >
  /**
   * Number of contiguous ports to forward (a PCP PORT_SET range), counting up
   * from both `external_port` and the target port. Defaults to 1. Not valid
   * together with SNI demux.
   */
  count: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
}> = /* @__PURE__ */ object({
  /**
   * External (WAN) port to forward. The external IP is fixed to the target's
   * WAN so return traffic stays symmetric.
   */
  externalPort: z.int().meta({
    description:
      "External (WAN) port to forward. The external IP is fixed to the target's\nWAN so return traffic stays symmetric.",
  }),
  target: z.string(),
  label: wire(z.string().nullable().optional(), z.string().nullable()),
  /**
   * Hostnames to SNI-demux on the shared external port. Empty = normal DNAT.
   */
  sni: wire(z.array(z.string()).optional(), z.array(z.string())).meta({
    description:
      'Hostnames to SNI-demux on the shared external port. Empty = normal DNAT.',
  }),
  /**
   * Number of contiguous ports to forward (a PCP PORT_SET range), counting up
   * from both `external_port` and the target port. Defaults to 1. Not valid
   * together with SNI demux.
   */
  count: wire(z.int().nullable().optional(), z.int().nullable()).meta({
    description:
      'Number of contiguous ports to forward (a PCP PORT_SET range), counting up\nfrom both `external_port` and the target port. Defaults to 1. Not valid\ntogether with SNI demux.',
  }),
}).meta({
  id: 'AddPortForwardParams',
})
export const AddSubnetParams: z.ZodObject<{ name: z.ZodString }> =
  /* @__PURE__ */ object({ name: z.string() }).meta({
    id: 'AddSubnetParams',
  })
export const AnyVerifyingKey: typeof literal_0 = /* @__PURE__ */ literal_0.meta(
  {
    id: 'AnyVerifyingKey',
  },
)
export const Base64: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'Base64',
})
export const CapabilityVerdict: z.ZodObject<{
  /**
   * `None` = never probed.
   */
  supported: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodBoolean>>,
    z.ZodNullable<z.ZodBoolean>
  >
  at: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  /**
   * `None` = never probed.
   */
  supported: wire(
    z.boolean().nullable().optional(),
    z.boolean().nullable(),
  ).meta({
    description: '`None` = never probed.',
  }),
  at: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'CapabilityVerdict',
})

/**
 * Per-subnet DNS proxy configuration. WireGuard clients on a subnet are pointed
 * at the subnet's in-tunnel server address (`.1`) for DNS; the forward-only proxy
 * listening there resolves every query against the upstream(s) selected here.
 */
export const DnsConfig: z.ZodUnion<
  readonly [
    z.ZodObject<{
      /**
       * Forward to the VPS's own system resolvers.
       */
      type: z.ZodLiteral<'default'>
    }>,
    z.ZodIntersection<
      z.ZodObject<{
        /**
         * Forward to a device on this subnet (its WireGuard IP) on port 53.
         */
        type: z.ZodLiteral<'device'>
      }>,
      z.ZodObject<{ ip: z.ZodString }>
    >,
    z.ZodIntersection<
      z.ZodObject<{
        /**
         * Forward to operator-specified upstream servers (1-3, optional `:port`).
         */
        type: z.ZodLiteral<'custom'>
      }>,
      z.ZodObject<{ servers: z.ZodArray<z.ZodString> }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    object({
      /**
       * Forward to the VPS's own system resolvers.
       */
      type: z.literal('default').meta({
        description: "Forward to the VPS's own system resolvers.",
      }),
    }),
    z.intersection(
      object({
        /**
         * Forward to a device on this subnet (its WireGuard IP) on port 53.
         */
        type: z.literal('device').meta({
          description:
            'Forward to a device on this subnet (its WireGuard IP) on port 53.',
        }),
      }),
      object({ ip: z.string() }),
    ),
    z.intersection(
      object({
        /**
         * Forward to operator-specified upstream servers (1-3, optional `:port`).
         */
        type: z.literal('custom').meta({
          description:
            'Forward to operator-specified upstream servers (1-3, optional `:port`).',
        }),
      }),
      object({ servers: z.array(z.string()) }),
    ),
  ])
  .meta({
    id: 'DnsConfig',
    description:
      "Per-subnet DNS proxy configuration. WireGuard clients on a subnet are pointed\nat the subnet's in-tunnel server address (`.1`) for DNS; the forward-only proxy\nlistening there resolves every query against the upstream(s) selected here.",
  })

/**
 * Which upstream a subnet's DNS proxy forwards to. `Device`/`Custom` draw
 * their data from companion fields on [`SetSubnetDnsParams`].
 */
export const DnsMode: z.ZodEnum<{
  default: 'default'
  device: 'device'
  custom: 'custom'
}> = /* @__PURE__ */ z.enum(['default', 'device', 'custom']).meta({
  id: 'DnsMode',
  description:
    "Which upstream a subnet's DNS proxy forwards to. `Device`/`Custom` draw\ntheir data from companion fields on [`SetSubnetDnsParams`].",
})

/**
 * A DNS record served by the tunnel (injected via RFC 2136 or added manually).
 * `value` is the rdata as text: an IP for A/AAAA, a name for CNAME, etc.
 */
export const DnsRecordEntry: z.ZodObject<{
  name: z.ZodString
  type: z.ZodString
  value: z.ZodString
  ttl: z.ZodInt
  /**
   * The device IP that injected this, or `null` for a manual record.
   */
  source: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  name: z.string(),
  type: z.string(),
  value: z.string(),
  ttl: z.int(),
  /**
   * The device IP that injected this, or `null` for a manual record.
   */
  source: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      'The device IP that injected this, or `null` for a manual record.',
  }),
}).meta({
  id: 'DnsRecordEntry',
  description:
    'A DNS record served by the tunnel (injected via RFC 2136 or added manually).\n`value` is the rdata as text: an IP for A/AAAA, a name for CNAME, etc.',
})
export const DnsSource: z.ZodEnum<{
  gateway: 'gateway'
  static: 'static'
  'public-fallback': 'public-fallback'
}> = /* @__PURE__ */ z.enum(['gateway', 'static', 'public-fallback']).meta({
  id: 'DnsSource',
})
export const GatewayId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'GatewayId',
})
export const GatewayType: z.ZodEnum<{
  'inbound-outbound': 'inbound-outbound'
  'outbound-only': 'outbound-only'
}> = /* @__PURE__ */ z.enum(['inbound-outbound', 'outbound-only']).meta({
  id: 'GatewayType',
})
export const HttpRedirectStatus: z.ZodObject<{
  ip: z.ZodString
  /**
   * Whether the redirect is on for this IP (default true).
   */
  enabled: z.ZodBoolean
  /**
   * Whether a port-forward already occupies port 80 on this IP, in which case
   * the redirect yields and does not bind.
   */
  forwarded: z.ZodBoolean
}> = /* @__PURE__ */ object({
  ip: z.string(),
  /**
   * Whether the redirect is on for this IP (default true).
   */
  enabled: z.boolean().meta({
    description: 'Whether the redirect is on for this IP (default true).',
  }),
  /**
   * Whether a port-forward already occupies port 80 on this IP, in which case
   * the redirect yields and does not bind.
   */
  forwarded: z.boolean().meta({
    description:
      'Whether a port-forward already occupies port 80 on this IP, in which case\nthe redirect yields and does not bind.',
  }),
}).meta({
  id: 'HttpRedirectStatus',
})

/**
 * Per-IPv4 HTTP→HTTPS redirect state. The tunnel runs a redirect on port 80 of
 * every public IPv4 by default; this records the addresses where the user has
 * turned it off (absence = on). The redirect also yields to any port-forward
 * occupying port 80 on that IP, so the two never fight over the port.
 */
export const HttpRedirects: z.ZodObject<{
  disabled: z.ZodCodec<
    z.ZodOptional<z.ZodArray<z.ZodString>>,
    z.ZodArray<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  disabled: wire(z.array(z.string()).optional(), z.array(z.string())),
}).meta({
  id: 'HttpRedirects',
  description:
    'Per-IPv4 HTTP→HTTPS redirect state. The tunnel runs a redirect on port 80 of\nevery public IPv4 by default; this records the addresses where the user has\nturned it off (absence = on). The redirect also yields to any port-forward\noccupying port 80 on that IP, so the two never fight over the port.',
})
export const ListDevicesParams: z.ZodObject<{ subnet: z.ZodString }> =
  /* @__PURE__ */ object({ subnet: z.string() }).meta({
    id: 'ListDevicesParams',
  })
export const NetworkInterfaceType: z.ZodEnum<{
  ethernet: 'ethernet'
  wireless: 'wireless'
  bridge: 'bridge'
  wireguard: 'wireguard'
  loopback: 'loopback'
}> = /* @__PURE__ */ z
  .enum(['ethernet', 'wireless', 'bridge', 'wireguard', 'loopback'])
  .meta({
    id: 'NetworkInterfaceType',
  })
export const Pem: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'Pem',
})

/**
 * One IPv6 GUA firewall entry, keyed by the exposed `[GUA]:external_port`. The
 * destination is always the same GUA. When `internal_port` is `None` (or equals
 * the key's port) it's a pure firewall pinhole — `ct state new accept`, no NAT.
 * When it differs it's a port-only DNAT to `[GUA]:internal_port` (e.g. 80→443),
 * the v6 analogue of a `PortForward::Dnat`.
 */
export const Pinhole: z.ZodObject<{
  label: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  enabled: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  /**
   * Contiguous ports opened, counting up from the key's port; `1` for single.
   */
  count: z.ZodCodec<z.ZodOptional<z.ZodInt>, z.ZodInt>
  /**
   * Destination port on the GUA; `None` means "same as the external (key) port"
   * — a pure pinhole. A different value makes this a port-DNAT.
   */
  internalPort: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  /**
   * Gateway-created (PCP) vs user-added. Drives the UI Manual/Automatic split.
   */
  auto: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  label: wire(z.string().nullable().optional(), z.string().nullable()),
  enabled: wire(z.boolean().optional(), z.boolean()),
  /**
   * Contiguous ports opened, counting up from the key's port; `1` for single.
   */
  count: wire(z.int().optional(), z.int()).meta({
    description:
      "Contiguous ports opened, counting up from the key's port; `1` for single.",
  }),
  /**
   * Destination port on the GUA; `None` means "same as the external (key) port"
   * — a pure pinhole. A different value makes this a port-DNAT.
   */
  internalPort: wire(z.int().nullable().optional(), z.int().nullable()).meta({
    description:
      'Destination port on the GUA; `None` means "same as the external (key) port"\n— a pure pinhole. A different value makes this a port-DNAT.',
  }),
  /**
   * Gateway-created (PCP) vs user-added. Drives the UI Manual/Automatic split.
   */
  auto: wire(z.boolean().optional(), z.boolean()).meta({
    description:
      'Gateway-created (PCP) vs user-added. Drives the UI Manual/Automatic split.',
  }),
}).meta({
  id: 'Pinhole',
  description:
    "One IPv6 GUA firewall entry, keyed by the exposed `[GUA]:external_port`. The\ndestination is always the same GUA. When `internal_port` is `None` (or equals\nthe key's port) it's a pure firewall pinhole — `ct state new accept`, no NAT.\nWhen it differs it's a port-only DNAT to `[GUA]:internal_port` (e.g. 80→443),\nthe v6 analogue of a `PortForward::Dnat`.",
})
export const RemoveDeviceParams: z.ZodObject<{
  subnet: z.ZodString
  ip: z.ZodString
}> = /* @__PURE__ */ object({ subnet: z.string(), ip: z.string() }).meta({
  id: 'RemoveDeviceParams',
})
export const RemoveDnsRecordParams: z.ZodObject<{
  name: z.ZodString
  type: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  name: z.string(),
  type: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'RemoveDnsRecordParams',
})
export const RemovePinholeParams: z.ZodObject<{
  gua: z.ZodString
  externalPort: z.ZodInt
}> = /* @__PURE__ */ object({ gua: z.string(), externalPort: z.int() }).meta({
  id: 'RemovePinholeParams',
})
export const RemovePortForwardParams: z.ZodObject<{
  source: z.ZodString
  /**
   * Remove a single SNI route on `source`; omit to remove the whole forward.
   */
  hostname: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  source: z.string(),
  /**
   * Remove a single SNI route on `source`; omit to remove the whole forward.
   */
  hostname: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      'Remove a single SNI route on `source`; omit to remove the whole forward.',
  }),
}).meta({
  id: 'RemovePortForwardParams',
})
export const Session: z.ZodObject<{
  /**
   * A friendly name for the key, if one was assigned at enrollment (e.g.
   * tunnel device keys). UI-enrolled keys are unnamed.
   */
  name: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  loggedIn: z.ZodCodec<z.ZodOptional<z.ZodString>, z.ZodString>
  lastActive: z.ZodCodec<z.ZodOptional<z.ZodString>, z.ZodString>
  userAgent: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  /**
   * A friendly name for the key, if one was assigned at enrollment (e.g.
   * tunnel device keys). UI-enrolled keys are unnamed.
   */
  name: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      'A friendly name for the key, if one was assigned at enrollment (e.g.\ntunnel device keys). UI-enrolled keys are unnamed.',
  }),
  loggedIn: wire(z.string().optional(), z.string()),
  lastActive: wire(z.string().optional(), z.string()),
  userAgent: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'Session',
})
export const SetAutoPortForwardParams: z.ZodObject<{
  subnet: z.ZodString
  ip: z.ZodString
  enabled: z.ZodBoolean
}> = /* @__PURE__ */ object({
  subnet: z.string(),
  ip: z.string(),
  enabled: z.boolean(),
}).meta({
  id: 'SetAutoPortForwardParams',
})
export const SetDeviceWanParams: z.ZodObject<{
  subnet: z.ZodString
  ip: z.ZodString
  wanIp: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  subnet: z.string(),
  ip: z.string(),
  wanIp: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'SetDeviceWanParams',
})
export const SetDnsInjectionParams: z.ZodObject<{
  subnet: z.ZodString
  ip: z.ZodString
  enabled: z.ZodBoolean
}> = /* @__PURE__ */ object({
  subnet: z.string(),
  ip: z.string(),
  enabled: z.boolean(),
}).meta({
  id: 'SetDnsInjectionParams',
})
export const SetHttpRedirectEnabledParams: z.ZodObject<{
  /**
   * The public IPv4 whose default-on port-80 HTTP→HTTPS redirect to toggle.
   */
  ip: z.ZodString
  enabled: z.ZodBoolean
}> = /* @__PURE__ */ object({
  /**
   * The public IPv4 whose default-on port-80 HTTP→HTTPS redirect to toggle.
   */
  ip: z.string().meta({
    description:
      'The public IPv4 whose default-on port-80 HTTP→HTTPS redirect to toggle.',
  }),
  enabled: z.boolean(),
}).meta({
  id: 'SetHttpRedirectEnabledParams',
})
export const SetPasswordParams: z.ZodObject<{ password: z.ZodString }> =
  /* @__PURE__ */ object({ password: z.string() }).meta({
    id: 'SetPasswordParams',
  })
export const SetPinholeEnabledParams: z.ZodObject<{
  gua: z.ZodString
  externalPort: z.ZodInt
  enabled: z.ZodBoolean
}> = /* @__PURE__ */ object({
  gua: z.string(),
  externalPort: z.int(),
  enabled: z.boolean(),
}).meta({
  id: 'SetPinholeEnabledParams',
})
export const SetPortForwardEnabledParams: z.ZodObject<{
  source: z.ZodString
  enabled: z.ZodBoolean
  /**
   * Toggle a single SNI route on `source`; omit for a DNAT forward.
   */
  hostname: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  source: z.string(),
  enabled: z.boolean(),
  /**
   * Toggle a single SNI route on `source`; omit for a DNAT forward.
   */
  hostname: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      'Toggle a single SNI route on `source`; omit for a DNAT forward.',
  }),
}).meta({
  id: 'SetPortForwardEnabledParams',
})
export const SetSubnetIpv6Params: z.ZodObject<{
  /**
   * The routed IPv6 prefix delegated to this subnet (e.g. a /64 from Hetzner,
   * a /56 from Linode). `null` disables IPv6 on the subnet.
   */
  prefix: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  /**
   * The routed IPv6 prefix delegated to this subnet (e.g. a /64 from Hetzner,
   * a /56 from Linode). `null` disables IPv6 on the subnet.
   */
  prefix: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      'The routed IPv6 prefix delegated to this subnet (e.g. a /64 from Hetzner,\na /56 from Linode). `null` disables IPv6 on the subnet.',
  }),
}).meta({
  id: 'SetSubnetIpv6Params',
})
export const SetSubnetWanParams: z.ZodObject<{
  wanIp: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  wanIp: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'SetSubnetWanParams',
})
export const ShowConfigParams: z.ZodObject<{
  subnet: z.ZodString
  ip: z.ZodString
  /**
   * Overrides the generated WireGuard endpoint without changing the WAN assignment.
   */
  endpoint: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  subnet: z.string(),
  ip: z.string(),
  /**
   * Overrides the generated WireGuard endpoint without changing the WAN assignment.
   */
  endpoint: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      'Overrides the generated WireGuard endpoint without changing the WAN assignment.',
  }),
}).meta({
  id: 'ShowConfigParams',
})

/**
 * One SNI-demultiplexed hostname route on a shared external port.
 */
export const SniRoute: z.ZodObject<{
  target: z.ZodString
  label: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  enabled: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  /**
   * Gateway-created vs user-added. Drives the UI Manual/Automatic split.
   */
  auto: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  target: z.string(),
  label: wire(z.string().nullable().optional(), z.string().nullable()),
  enabled: wire(z.boolean().optional(), z.boolean()),
  /**
   * Gateway-created vs user-added. Drives the UI Manual/Automatic split.
   */
  auto: wire(z.boolean().optional(), z.boolean()).meta({
    description:
      'Gateway-created vs user-added. Drives the UI Manual/Automatic split.',
  }),
}).meta({
  id: 'SniRoute',
  description:
    'One SNI-demultiplexed hostname route on a shared external port.',
})
export const SubnetParams: z.ZodObject<{ subnet: z.ZodString }> =
  /* @__PURE__ */ object({ subnet: z.string() }).meta({
    id: 'SubnetParams',
  })
export const TunnelCertData: z.ZodObject<{
  key: typeof literal_0
  cert: typeof literal_0
}> = /* @__PURE__ */ object({
  key: literal_0.meta({
    description:
      'Standard Base64 DER with padding. Deserialization also accepts legacy PEM armor.',
  }),
  cert: literal_0,
}).meta({
  id: 'TunnelCertData',
})
export const TunnelUpdateResult: z.ZodObject<{
  /**
   * "up-to-date", "update-available", or "updating"
   */
  status: z.ZodString
  /**
   * Currently installed version
   */
  installed: z.ZodString
  /**
   * Available candidate version
   */
  candidate: z.ZodString
}> = /* @__PURE__ */ object({
  /**
   * "up-to-date", "update-available", or "updating"
   */
  status: z.string().meta({
    description: '"up-to-date", "update-available", or "updating"',
  }),
  /**
   * Currently installed version
   */
  installed: z.string().meta({
    description: 'Currently installed version',
  }),
  /**
   * Available candidate version
   */
  candidate: z.string().meta({
    description: 'Available candidate version',
  }),
}).meta({
  id: 'TunnelUpdateResult',
})
export const UpdatePinholeLabelParams: z.ZodObject<{
  gua: z.ZodString
  externalPort: z.ZodInt
  label: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  gua: z.string(),
  externalPort: z.int(),
  label: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'UpdatePinholeLabelParams',
})
export const UpdatePortForwardLabelParams: z.ZodObject<{
  source: z.ZodString
  label: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Label a single SNI route on `source`; omit to label the DNAT forward.
   */
  hostname: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  source: z.string(),
  label: wire(z.string().nullable().optional(), z.string().nullable()),
  /**
   * Label a single SNI route on `source`; omit to label the DNAT forward.
   */
  hostname: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      'Label a single SNI route on `source`; omit to label the DNAT forward.',
  }),
}).meta({
  id: 'UpdatePortForwardLabelParams',
})

/**
 * A WireGuard client's role. A `Server` is a StartOS box that may configure the
 * gateway (DNS injection / auto port-forward); a `Client` is a plain peer with
 * no autoconfig. Stored and sticky — toggling the capability flags never changes
 * it; the migration backfills it from those flags.
 */
export const WgClientKind: z.ZodEnum<{ client: 'client'; server: 'server' }> =
  /* @__PURE__ */ z.enum(['client', 'server']).meta({
    id: 'WgClientKind',
    description:
      "A WireGuard client's role. A `Server` is a StartOS box that may configure the\ngateway (DNS injection / auto port-forward); a `Client` is a plain peer with\nno autoconfig. Stored and sticky — toggling the capability flags never changes\nit; the migration backfills it from those flags.",
  })
export const AddDeviceParams: z.ZodObject<{
  subnet: z.ZodString
  name: z.ZodString
  ip: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Client (no autoconfig) or Server (gateway-autoconfig on by default).
   */
  kind: z.ZodCodec<z.ZodOptional<typeof WgClientKind>, typeof WgClientKind>
}> = /* @__PURE__ */ object({
  subnet: z.string(),
  name: z.string(),
  ip: wire(z.string().nullable().optional(), z.string().nullable()),
  /**
   * Client (no autoconfig) or Server (gateway-autoconfig on by default).
   */
  kind: wire(WgClientKind.optional(), WgClientKind).meta({
    description:
      'Client (no autoconfig) or Server (gateway-autoconfig on by default).',
  }),
}).meta({
  id: 'AddDeviceParams',
})
export const AddKeyParams: z.ZodObject<{
  name: z.ZodString
  key: typeof AnyVerifyingKey
}> = /* @__PURE__ */ object({ name: z.string(), key: AnyVerifyingKey }).meta({
  id: 'AddKeyParams',
})

/**
 * Enrolled sign-ins indexed by canonical DER64 SPKI.
 */
export const AuthKeys: z.ZodRecord<z.ZodString, typeof Session> =
  /* @__PURE__ */ z.record(z.string(), Session).meta({
    id: 'AuthKeys',
    description: 'Enrolled sign-ins indexed by canonical DER64 SPKI.',
  })
export const DnsRecords: z.ZodArray<typeof DnsRecordEntry> = /* @__PURE__ */ z
  .array(DnsRecordEntry)
  .meta({
    id: 'DnsRecords',
  })

/**
 * Per-gateway DNS intent and effective upstreams. `static_servers` is the persisted
 * override; `servers` and `source` are derived from it and the link configuration.
 * Without an override or gateway-provided resolvers, the public fallback is used.
 */
export const DnsSettings: z.ZodObject<{
  staticServers: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<z.ZodString>>>,
    z.ZodNullable<z.ZodArray<z.ZodString>>
  >
  servers: z.ZodArray<z.ZodString>
  source: typeof DnsSource
}> = /* @__PURE__ */ object({
  staticServers: wire(
    z.array(z.string()).nullable().optional(),
    z.array(z.string()).nullable(),
  ),
  servers: z.array(z.string()),
  source: DnsSource,
}).meta({
  id: 'DnsSettings',
  description:
    'Per-gateway DNS intent and effective upstreams. `static_servers` is the persisted\noverride; `servers` and `source` are derived from it and the link configuration.\nWithout an override or gateway-provided resolvers, the public fallback is used.',
})

/**
 * Whether the gateway reachable via this interface speaks each port-mapping
 * protocol, as last probed. Fed by the watcher's periodic probes and by
 * failure/success evidence from the port-map client, and synced to the db so a
 * chronically uncooperative gateway is visible (and skipped) instead of being
 * retried forever.
 */
export const GatewayPortMapCapabilities: z.ZodObject<{
  pcp: typeof CapabilityVerdict
  natPmp: typeof CapabilityVerdict
  upnp: typeof CapabilityVerdict
  /**
   * The PCP server answers ANNOUNCE with the Start9 capability marker, i.e.
   * it honors OPTION_HOSTNAME (SNI demux).
   */
  pcpHostname: typeof CapabilityVerdict
}> = /* @__PURE__ */ object({
  pcp: CapabilityVerdict,
  natPmp: CapabilityVerdict,
  upnp: CapabilityVerdict,
  /**
   * The PCP server answers ANNOUNCE with the Start9 capability marker, i.e.
   * it honors OPTION_HOSTNAME (SNI demux).
   */
  pcpHostname: CapabilityVerdict.meta({
    description:
      'The PCP server answers ANNOUNCE with the Start9 capability marker, i.e.\nit honors OPTION_HOSTNAME (SNI demux).',
  }),
}).meta({
  id: 'GatewayPortMapCapabilities',
  description:
    "Whether the gateway reachable via this interface speaks each port-mapping\nprotocol, as last probed. Fed by the watcher's periodic probes and by\nfailure/success evidence from the port-map client, and synced to the db so a\nchronically uncooperative gateway is visible (and skipped) instead of being\nretried forever.",
})
export const IpInfo: z.ZodObject<{
  name: z.ZodString
  scopeId: z.ZodInt
  deviceType: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof NetworkInterfaceType>>,
    z.ZodNullable<typeof NetworkInterfaceType>
  >
  subnets: z.ZodArray<z.ZodString>
  lanIp: z.ZodArray<z.ZodString>
  wanIp: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  ntpServers: z.ZodArray<z.ZodString>
  dnsServers: z.ZodArray<z.ZodString>
  /**
   * Lowest IPv4 default-route metric in `main`, or `None` when absent.
   */
  defaultRouteV4: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  /**
   * Lowest IPv6 default-route metric in `main`, or `None` when absent.
   */
  defaultRouteV6: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
}> = /* @__PURE__ */ object({
  name: z.string(),
  scopeId: z.int(),
  deviceType: wire(
    NetworkInterfaceType.nullable().optional(),
    NetworkInterfaceType.nullable(),
  ),
  subnets: z.array(z.string()),
  lanIp: z.array(z.string()),
  wanIp: wire(z.string().nullable().optional(), z.string().nullable()),
  ntpServers: z.array(z.string()),
  dnsServers: z.array(z.string()),
  /**
   * Lowest IPv4 default-route metric in `main`, or `None` when absent.
   */
  defaultRouteV4: wire(z.int().nullable().optional(), z.int().nullable()).meta({
    description:
      'Lowest IPv4 default-route metric in `main`, or `None` when absent.',
  }),
  /**
   * Lowest IPv6 default-route metric in `main`, or `None` when absent.
   */
  defaultRouteV6: wire(z.int().nullable().optional(), z.int().nullable()).meta({
    description:
      'Lowest IPv6 default-route metric in `main`, or `None` when absent.',
  }),
}).meta({
  id: 'IpInfo',
})
export const Pinholes6: z.ZodRecord<z.ZodString, typeof Pinhole> =
  /* @__PURE__ */ z.record(z.string(), Pinhole).meta({
    id: 'Pinholes6',
  })

/**
 * One external-port forward: an nftables DNAT or an SNI-demultiplexed shared
 * port. Mutually exclusive for a given external address.
 */
export const PortForward: z.ZodUnion<
  readonly [
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'dnat'> }>,
      z.ZodObject<{
        target: z.ZodString
        label: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodString>>,
          z.ZodNullable<z.ZodString>
        >
        enabled: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
        /**
         * Contiguous ports forwarded (a PCP PORT_SET range); `1` for single-port.
         */
        count: z.ZodCodec<z.ZodOptional<z.ZodInt>, z.ZodInt>
        /**
         * Gateway-created (PCP/UPnP) vs user-added. Drives the UI Manual/Automatic split.
         */
        auto: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
      }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'sni'> }>,
      z.ZodObject<{
        /**
         * hostname (lowercase; may be `*.suffix`) -> route.
         */
        routes: z.ZodRecord<z.ZodString, typeof SniRoute>
        /**
         * Hostname-less catch-all for this shared external port. Traffic whose
         * SNI matches no `routes` entry — or that carries no SNI (bare-IP TLS,
         * non-TLS) — is spliced here instead of being dropped. `None` closes the
         * port to unmatched traffic. This lets a bare public IP and named
         * domains share one external port, the bare IP acting as the fallback.
         */
        fallback: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<typeof SniRoute>>,
          z.ZodNullable<typeof SniRoute>
        >
      }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(
      object({ kind: z.literal('dnat') }),
      object({
        target: z.string(),
        label: wire(z.string().nullable().optional(), z.string().nullable()),
        enabled: wire(z.boolean().optional(), z.boolean()),
        /**
         * Contiguous ports forwarded (a PCP PORT_SET range); `1` for single-port.
         */
        count: wire(z.int().optional(), z.int()).meta({
          description:
            'Contiguous ports forwarded (a PCP PORT_SET range); `1` for single-port.',
        }),
        /**
         * Gateway-created (PCP/UPnP) vs user-added. Drives the UI Manual/Automatic split.
         */
        auto: wire(z.boolean().optional(), z.boolean()).meta({
          description:
            'Gateway-created (PCP/UPnP) vs user-added. Drives the UI Manual/Automatic split.',
        }),
      }),
    ),
    z.intersection(
      object({ kind: z.literal('sni') }),
      object({
        /**
         * hostname (lowercase; may be `*.suffix`) -> route.
         */
        routes: z.record(z.string(), SniRoute).meta({
          description: 'hostname (lowercase; may be `*.suffix`) -> route.',
        }),
        /**
         * Hostname-less catch-all for this shared external port. Traffic whose
         * SNI matches no `routes` entry — or that carries no SNI (bare-IP TLS,
         * non-TLS) — is spliced here instead of being dropped. `None` closes the
         * port to unmatched traffic. This lets a bare public IP and named
         * domains share one external port, the bare IP acting as the fallback.
         */
        fallback: wire(
          SniRoute.nullable().optional(),
          SniRoute.nullable(),
        ).meta({
          description:
            'Hostname-less catch-all for this shared external port. Traffic whose\nSNI matches no `routes` entry — or that carries no SNI (bare-IP TLS,\nnon-TLS) — is spliced here instead of being dropped. `None` closes the\nport to unmatched traffic. This lets a bare public IP and named\ndomains share one external port, the bare IP acting as the fallback.',
        }),
      }),
    ),
  ])
  .meta({
    id: 'PortForward',
    description:
      'One external-port forward: an nftables DNAT or an SNI-demultiplexed shared\nport. Mutually exclusive for a given external address.',
  })
export const RemoveKeyParams: z.ZodObject<{ key: typeof AnyVerifyingKey }> =
  /* @__PURE__ */ object({ key: AnyVerifyingKey }).meta({
    id: 'RemoveKeyParams',
  })
export const SetDeviceKindParams: z.ZodObject<{
  subnet: z.ZodString
  ip: z.ZodString
  kind: typeof WgClientKind
}> = /* @__PURE__ */ object({
  subnet: z.string(),
  ip: z.string(),
  kind: WgClientKind,
}).meta({
  id: 'SetDeviceKindParams',
})
export const SetSubnetDnsParams: z.ZodObject<{
  mode: typeof DnsMode
  /**
   * The selected device's WireGuard IP; required when `mode` is `device`.
   */
  deviceIp: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Up to 3 upstream servers (bare IP or `ip:port`); used when `mode` is `custom`.
   */
  servers: z.ZodArray<z.ZodString>
}> = /* @__PURE__ */ object({
  mode: DnsMode,
  /**
   * The selected device's WireGuard IP; required when `mode` is `device`.
   */
  deviceIp: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      "The selected device's WireGuard IP; required when `mode` is `device`.",
  }),
  /**
   * Up to 3 upstream servers (bare IP or `ip:port`); used when `mode` is `custom`.
   */
  servers: z.array(z.string()).meta({
    description:
      'Up to 3 upstream servers (bare IP or `ip:port`); used when `mode` is `custom`.',
  }),
}).meta({
  id: 'SetSubnetDnsParams',
})
export const WebserverInfo: z.ZodObject<{
  enabled: z.ZodBoolean
  listen: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  certificate: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof TunnelCertData>>,
    z.ZodNullable<typeof TunnelCertData>
  >
}> = /* @__PURE__ */ object({
  enabled: z.boolean(),
  listen: wire(z.string().nullable().optional(), z.string().nullable()),
  certificate: wire(
    TunnelCertData.nullable().optional(),
    TunnelCertData.nullable(),
  ),
}).meta({
  id: 'WebserverInfo',
})
export const WgConfig: z.ZodObject<{
  name: z.ZodString
  key: typeof literal_0
  psk: typeof literal_0
  /**
   * Client (no autoconfig) vs Server (a StartOS box). Sticky; defaulted by the
   * migration from the capability flags.
   */
  kind: z.ZodCodec<z.ZodOptional<typeof WgClientKind>, typeof WgClientKind>
  /**
   * Whether this device may inject DNS records via RFC 2136. Off by default
   * — only enable for devices you trust (it lets the device add records to
   * the tunnel's DNS for every peer to resolve).
   */
  allowDnsInjection: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  /**
   * Whether this device may auto-create port forwards via PCP/IGD. Off by
   * default — paired with `allow_dns_injection` under one "Gateway
   * Autoconfiguration" toggle, but tracked separately so each capability is
   * gated on its own.
   */
  allowAutoPortForward: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  /**
   * SNAT this device's egress to this WAN IP, overriding the subnet's
   * `wan_ip` / the default masquerade. `None` falls back to the subnet rule.
   */
  wanIp: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  name: z.string(),
  key: literal_0,
  psk: literal_0,
  /**
   * Client (no autoconfig) vs Server (a StartOS box). Sticky; defaulted by the
   * migration from the capability flags.
   */
  kind: wire(WgClientKind.optional(), WgClientKind).meta({
    description:
      'Client (no autoconfig) vs Server (a StartOS box). Sticky; defaulted by the\nmigration from the capability flags.',
  }),
  /**
   * Whether this device may inject DNS records via RFC 2136. Off by default
   * — only enable for devices you trust (it lets the device add records to
   * the tunnel's DNS for every peer to resolve).
   */
  allowDnsInjection: wire(z.boolean().optional(), z.boolean()).meta({
    description:
      "Whether this device may inject DNS records via RFC 2136. Off by default\n— only enable for devices you trust (it lets the device add records to\nthe tunnel's DNS for every peer to resolve).",
  }),
  /**
   * Whether this device may auto-create port forwards via PCP/IGD. Off by
   * default — paired with `allow_dns_injection` under one "Gateway
   * Autoconfiguration" toggle, but tracked separately so each capability is
   * gated on its own.
   */
  allowAutoPortForward: wire(z.boolean().optional(), z.boolean()).meta({
    description:
      'Whether this device may auto-create port forwards via PCP/IGD. Off by\ndefault — paired with `allow_dns_injection` under one "Gateway\nAutoconfiguration" toggle, but tracked separately so each capability is\ngated on its own.',
  }),
  /**
   * SNAT this device's egress to this WAN IP, overriding the subnet's
   * `wan_ip` / the default masquerade. `None` falls back to the subnet rule.
   */
  wanIp: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      "SNAT this device's egress to this WAN IP, overriding the subnet's\n`wan_ip` / the default masquerade. `None` falls back to the subnet rule.",
  }),
}).meta({
  id: 'WgConfig',
})
export const NetworkInterfaceInfo: z.ZodCodec<
  z.ZodObject<{
    /**
     * Exact names eligible for host-only DNS bootstrap over the underlay.
     */
    wireguardEndpointDomains: z.ZodOptional<z.ZodArray<z.ZodString>>
    dns: z.ZodOptional<typeof DnsSettings>
    name: z.ZodOptional<z.ZodNullable<z.ZodString>>
    secure: z.ZodOptional<z.ZodNullable<z.ZodBoolean>>
    ipInfo: z.ZodOptional<z.ZodNullable<typeof IpInfo>>
    type: z.ZodOptional<z.ZodNullable<typeof GatewayType>>
    portMap: z.ZodOptional<typeof GatewayPortMapCapabilities>
    /**
     * The gateway's resolver accepted our last RFC 2136 DNS UPDATE — evidence
     * from the update client (`net::dns_update`). A WireGuard gateway only
     * serves the injected `<hostname>.local` while this is `supported`, so
     * only then is the name listed on it.
     */
    dnsUpdate: z.ZodOptional<typeof CapabilityVerdict>
  }>,
  z.ZodObject<{
    /**
     * Exact names eligible for host-only DNS bootstrap over the underlay.
     */
    wireguardEndpointDomains: z.ZodArray<z.ZodString>
    dns: typeof DnsSettings
    name: z.ZodNullable<z.ZodString>
    secure: z.ZodNullable<z.ZodBoolean>
    ipInfo: z.ZodNullable<typeof IpInfo>
    type: typeof GatewayType
    portMap: typeof GatewayPortMapCapabilities
    /**
     * The gateway's resolver accepted our last RFC 2136 DNS UPDATE — evidence
     * from the update client (`net::dns_update`). A WireGuard gateway only
     * serves the injected `<hostname>.local` while this is `supported`, so
     * only then is the name listed on it.
     */
    dnsUpdate: typeof CapabilityVerdict
  }>
> = /* @__PURE__ */ wire(
  object({
    /**
     * Exact names eligible for host-only DNS bootstrap over the underlay.
     */
    wireguardEndpointDomains: z.array(z.string()).optional().meta({
      description:
        'Exact names eligible for host-only DNS bootstrap over the underlay.',
    }),
    dns: DnsSettings.optional(),
    name: z.string().nullable().optional(),
    secure: z.boolean().nullable().optional(),
    ipInfo: IpInfo.nullable().optional(),
    type: GatewayType.nullable().optional(),
    portMap: GatewayPortMapCapabilities.optional(),
    /**
     * The gateway's resolver accepted our last RFC 2136 DNS UPDATE — evidence
     * from the update client (`net::dns_update`). A WireGuard gateway only
     * serves the injected `<hostname>.local` while this is `supported`, so
     * only then is the name listed on it.
     */
    dnsUpdate: CapabilityVerdict.optional().meta({
      description:
        "The gateway's resolver accepted our last RFC 2136 DNS UPDATE — evidence\nfrom the update client (`net::dns_update`). A WireGuard gateway only\nserves the injected `<hostname>.local` while this is `supported`, so\nonly then is the name listed on it.",
    }),
  }),
  object({
    /**
     * Exact names eligible for host-only DNS bootstrap over the underlay.
     */
    wireguardEndpointDomains: z.array(z.string()).meta({
      description:
        'Exact names eligible for host-only DNS bootstrap over the underlay.',
    }),
    dns: DnsSettings,
    name: z.string().nullable(),
    secure: z.boolean().nullable(),
    ipInfo: IpInfo.nullable(),
    type: GatewayType,
    portMap: GatewayPortMapCapabilities,
    /**
     * The gateway's resolver accepted our last RFC 2136 DNS UPDATE — evidence
     * from the update client (`net::dns_update`). A WireGuard gateway only
     * serves the injected `<hostname>.local` while this is `supported`, so
     * only then is the name listed on it.
     */
    dnsUpdate: CapabilityVerdict.meta({
      description:
        "The gateway's resolver accepted our last RFC 2136 DNS UPDATE — evidence\nfrom the update client (`net::dns_update`). A WireGuard gateway only\nserves the injected `<hostname>.local` while this is `supported`, so\nonly then is the name listed on it.",
    }),
  }),
).meta({
  id: 'NetworkInterfaceInfo',
})
export const PortForwards: z.ZodRecord<z.ZodString, typeof PortForward> =
  /* @__PURE__ */ z.record(z.string(), PortForward).meta({
    id: 'PortForwards',
  })
export const WgSubnetClients: z.ZodRecord<z.ZodString, typeof WgConfig> =
  /* @__PURE__ */ z.record(z.string(), WgConfig).meta({
    id: 'WgSubnetClients',
  })
export const WgSubnetConfig: z.ZodObject<{
  name: z.ZodString
  clients: typeof WgSubnetClients
  dns: z.ZodCodec<z.ZodOptional<typeof DnsConfig>, typeof DnsConfig>
  /**
   * SNAT this subnet's egress to this WAN IP instead of `masquerade`. `None`
   * keeps the default masquerade; a per-device `wan_ip` overrides this.
   */
  wanIp: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Routed IPv6 prefix delegated to this subnet, if any. Each host on the
   * subnet (the server and every client) gets one `/128` out of it with its
   * tunnel IPv4 embedded — see [`crate::tunnel::wg6`].
   */
  ipv6: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  name: z.string(),
  clients: WgSubnetClients,
  dns: wire(DnsConfig.optional(), DnsConfig),
  /**
   * SNAT this subnet's egress to this WAN IP instead of `masquerade`. `None`
   * keeps the default masquerade; a per-device `wan_ip` overrides this.
   */
  wanIp: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      "SNAT this subnet's egress to this WAN IP instead of `masquerade`. `None`\nkeeps the default masquerade; a per-device `wan_ip` overrides this.",
  }),
  /**
   * Routed IPv6 prefix delegated to this subnet, if any. Each host on the
   * subnet (the server and every client) gets one `/128` out of it with its
   * tunnel IPv4 embedded — see [`crate::tunnel::wg6`].
   */
  ipv6: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      'Routed IPv6 prefix delegated to this subnet, if any. Each host on the\nsubnet (the server and every client) gets one `/128` out of it with its\ntunnel IPv4 embedded — see [`crate::tunnel::wg6`].',
  }),
}).meta({
  id: 'WgSubnetConfig',
})
export const WgSubnetMap: z.ZodRecord<z.ZodString, typeof WgSubnetConfig> =
  /* @__PURE__ */ z.record(z.string(), WgSubnetConfig).meta({
    id: 'WgSubnetMap',
  })
export const WgServer: z.ZodObject<{
  port: z.ZodInt
  key: typeof literal_0
  subnets: typeof WgSubnetMap
}> = /* @__PURE__ */ object({
  port: z.int(),
  key: literal_0,
  subnets: WgSubnetMap,
}).meta({
  id: 'WgServer',
})
export const TunnelDatabase: z.ZodObject<{
  webserver: typeof WebserverInfo
  password: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Same key as the StartOS private db, so a 1.1.x db upgrades by serde
   * default (empty — everyone signs in again) with no migration.
   */
  sessionPubkeys: z.ZodCodec<z.ZodOptional<typeof AuthKeys>, typeof AuthKeys>
  gateways: z.ZodRecord<z.ZodString, typeof NetworkInterfaceInfo>
  wg: typeof WgServer
  portForwards: typeof PortForwards
  /**
   * IPv6 GUA firewall pinholes: inbound to a client's own global address is
   * accepted (no NAT — the GUA is directly routable), keyed by the exposed
   * `[GUA]:port`. The v4 analogue is a `PortForward::Dnat`.
   */
  pinholes6: z.ZodCodec<z.ZodOptional<typeof Pinholes6>, typeof Pinholes6>
  dnsRecords: z.ZodCodec<z.ZodOptional<typeof DnsRecords>, typeof DnsRecords>
  httpRedirects: z.ZodCodec<
    z.ZodOptional<typeof HttpRedirects>,
    typeof HttpRedirects
  >
}> = /* @__PURE__ */ object({
  webserver: WebserverInfo,
  password: wire(z.string().nullable().optional(), z.string().nullable()),
  /**
   * Same key as the StartOS private db, so a 1.1.x db upgrades by serde
   * default (empty — everyone signs in again) with no migration.
   */
  sessionPubkeys: wire(AuthKeys.optional(), AuthKeys).meta({
    description:
      'Same key as the StartOS private db, so a 1.1.x db upgrades by serde\ndefault (empty — everyone signs in again) with no migration.',
  }),
  gateways: z.record(z.string(), NetworkInterfaceInfo),
  wg: WgServer,
  portForwards: PortForwards,
  /**
   * IPv6 GUA firewall pinholes: inbound to a client's own global address is
   * accepted (no NAT — the GUA is directly routable), keyed by the exposed
   * `[GUA]:port`. The v4 analogue is a `PortForward::Dnat`.
   */
  pinholes6: wire(Pinholes6.optional(), Pinholes6).meta({
    description:
      "IPv6 GUA firewall pinholes: inbound to a client's own global address is\naccepted (no NAT — the GUA is directly routable), keyed by the exposed\n`[GUA]:port`. The v4 analogue is a `PortForward::Dnat`.",
  }),
  dnsRecords: wire(DnsRecords.optional(), DnsRecords),
  httpRedirects: wire(HttpRedirects.optional(), HttpRedirects),
}).meta({
  id: 'TunnelDatabase',
})
