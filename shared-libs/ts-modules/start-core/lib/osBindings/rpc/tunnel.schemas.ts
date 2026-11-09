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
export const ApplyParams: z.ZodObject<{ expr: z.ZodString }> =
  /* @__PURE__ */ object({ expr: z.string() }).meta({
    id: 'ApplyParams',
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
export const Dump: z.ZodObject<{ id: z.ZodInt; value: z.ZodUnknown }> =
  /* @__PURE__ */ object({ id: z.int(), value: z.unknown() }).meta({
    id: 'Dump',
  })
export const DumpParams: z.ZodObject<{
  pointer: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  pointer: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'DumpParams',
})
export const GenerateCertParams: z.ZodObject<{
  subject: z.ZodArray<z.ZodString>
}> = /* @__PURE__ */ object({ subject: z.array(z.string()) }).meta({
  id: 'GenerateCertParams',
})
export const Guid: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'Guid',
})

/**
 * Proof that a set of auth keys was unenrolled — removed from the persisted
 * and ephemeral signer stores with any continuations they opened killed.
 * Obtained via [`SignatureAuthContext::unenroll`], or [`Self::unenroll`]
 * from inside a db transaction.
 */
export const HasUnenrolledKeys: z.ZodNull = /* @__PURE__ */ z.null().meta({
  id: 'HasUnenrolledKeys',
  description:
    'Proof that a set of auth keys was unenrolled — removed from the persisted\nand ephemeral signer stores with any continuations they opened killed.\nObtained via [`SignatureAuthContext::unenroll`], or [`Self::unenroll`]\nfrom inside a db transaction.',
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
export const IoFormat: z.ZodEnum<{
  json: 'json'
  'json-pretty': 'json-pretty'
  yaml: 'yaml'
  cbor: 'cbor'
  toml: 'toml'
  'toml-pretty': 'toml-pretty'
}> = /* @__PURE__ */ z
  .enum(['json', 'json-pretty', 'yaml', 'cbor', 'toml', 'toml-pretty'])
  .meta({
    id: 'IoFormat',
  })
export const ListDevicesParams: z.ZodObject<{ subnet: z.ZodString }> =
  /* @__PURE__ */ object({ subnet: z.string() }).meta({
    id: 'ListDevicesParams',
  })
export const ListParams: typeof empty = /* @__PURE__ */ empty.meta({
  id: 'ListParams',
})
export const LogoutParams: typeof empty = /* @__PURE__ */ empty.meta({
  id: 'LogoutParams',
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
export const SetListenParams: z.ZodObject<{ listen: z.ZodString }> =
  /* @__PURE__ */ object({ listen: z.string() }).meta({
    id: 'SetListenParams',
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
export const SubnetParams: z.ZodObject<{ subnet: z.ZodString }> =
  /* @__PURE__ */ object({ subnet: z.string() }).meta({
    id: 'SubnetParams',
  })
export const SubscribeParams: z.ZodObject<{
  pointer: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  pointer: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'SubscribeParams',
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
export const ApplyWithPathParams: z.ZodIntersection<
  z.ZodObject<{
    path: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<z.ZodString>>,
      z.ZodNullable<z.ZodString>
    >
  }>,
  typeof ApplyParams
> = /* @__PURE__ */ z
  .intersection(
    object({
      path: wire(z.string().nullable().optional(), z.string().nullable()),
    }),
    ApplyParams,
  )
  .meta({
    id: 'ApplyWithPathParams',
  })

/**
 * Enrolled sign-ins indexed by canonical DER64 SPKI.
 */
export const AuthKeys: z.ZodRecord<z.ZodString, typeof Session> =
  /* @__PURE__ */ z.record(z.string(), Session).meta({
    id: 'AuthKeys',
    description: 'Enrolled sign-ins indexed by canonical DER64 SPKI.',
  })
export const KillParams: z.ZodObject<{
  pubkeys: z.ZodCodec<
    z.ZodOptional<z.ZodArray<typeof AnyVerifyingKey>>,
    z.ZodArray<typeof AnyVerifyingKey>
  >
  tokens: z.ZodCodec<
    z.ZodOptional<z.ZodArray<z.ZodString>>,
    z.ZodArray<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  pubkeys: wire(z.array(AnyVerifyingKey).optional(), z.array(AnyVerifyingKey)),
  tokens: wire(z.array(z.string()).optional(), z.array(z.string())),
}).meta({
  id: 'KillParams',
})
export const LoginParams: z.ZodObject<{
  password: z.ZodString
  /**
   * The PEM-encoded public key to enroll on a successful login. The login
   * request itself is signed with the matching secret key, so enrollment
   * proves possession.
   */
  pubkey: typeof AnyVerifyingKey
  /**
   * Enroll in memory only, never persisted (kiosk mode, which re-enrolls
   * on every browser restart and would otherwise accumulate keys).
   */
  ephemeral: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  password: z.string(),
  /**
   * The PEM-encoded public key to enroll on a successful login. The login
   * request itself is signed with the matching secret key, so enrollment
   * proves possession.
   */
  pubkey: AnyVerifyingKey.meta({
    description:
      'The PEM-encoded public key to enroll on a successful login. The login\nrequest itself is signed with the matching secret key, so enrollment\nproves possession.',
  }),
  /**
   * Enroll in memory only, never persisted (kiosk mode, which re-enrolls
   * on every browser restart and would otherwise accumulate keys).
   */
  ephemeral: wire(z.boolean().optional(), z.boolean()).meta({
    description:
      'Enroll in memory only, never persisted (kiosk mode, which re-enrolls\non every browser restart and would otherwise accumulate keys).',
  }),
}).meta({
  id: 'LoginParams',
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
export const SubscribeRes: z.ZodObject<{
  dump: typeof Dump
  guid: typeof Guid
}> = /* @__PURE__ */ object({ dump: Dump, guid: Guid }).meta({
  id: 'SubscribeRes',
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

/**
 * The server's sessions, by credential type.
 */
export const Sessions: z.ZodObject<{
  pubkeys: typeof AuthKeys
  tokens: z.ZodRecord<z.ZodString, typeof Session>
}> = /* @__PURE__ */ object({
  pubkeys: AuthKeys,
  tokens: z.record(z.string(), Session),
}).meta({
  id: 'Sessions',
  description: "The server's sessions, by credential type.",
})
export const WgSubnetClients: z.ZodRecord<z.ZodString, typeof WgConfig> =
  /* @__PURE__ */ z.record(z.string(), WgConfig).meta({
    id: 'WgSubnetClients',
  })
export const SessionList: z.ZodObject<{
  current: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  sessions: typeof Sessions
}> = /* @__PURE__ */ object({
  current: wire(z.string().nullable().optional(), z.string().nullable()),
  sessions: Sessions,
}).meta({
  id: 'SessionList',
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
export const Api = {
  _PARAMS: /* @__PURE__ */ empty,
  _CHILDREN: {
    auth: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        key: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            add: {
              _PARAMS: /* @__PURE__ */ AddKeyParams,
              _RETURN: /* @__PURE__ */ z.null(),
            },
            list: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(
                  object({
                    format: wire(
                      IoFormat.nullable().optional(),
                      IoFormat.nullable(),
                    ),
                  }),
                  empty,
                ),
                empty,
              ),
              _RETURN: /* @__PURE__ */ AuthKeys,
            },
            remove: {
              _PARAMS: /* @__PURE__ */ RemoveKeyParams,
              _RETURN: /* @__PURE__ */ z.null(),
            },
          },
        },
        login: {
          _PARAMS: /* @__PURE__ */ LoginParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        logout: {
          _PARAMS: /* @__PURE__ */ LogoutParams,
          _RETURN: /* @__PURE__ */ HasUnenrolledKeys.nullable(),
        },
        session: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            kill: {
              _PARAMS: /* @__PURE__ */ KillParams,
              _RETURN: /* @__PURE__ */ z.null(),
            },
            list: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(
                  object({
                    format: wire(
                      IoFormat.nullable().optional(),
                      IoFormat.nullable(),
                    ),
                  }),
                  empty,
                ),
                ListParams,
              ),
              _RETURN: /* @__PURE__ */ SessionList,
            },
          },
        },
        'set-password': {
          _PARAMS: /* @__PURE__ */ SetPasswordParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
    db: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        apply: {
          _PARAMS: /* @__PURE__ */ ApplyWithPathParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        dump: {
          _PARAMS: /* @__PURE__ */ DumpParams,
          _RETURN: /* @__PURE__ */ Dump,
        },
        subscribe: {
          _PARAMS: /* @__PURE__ */ SubscribeParams,
          _RETURN: /* @__PURE__ */ SubscribeRes,
        },
      },
    },
    device: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        add: {
          _PARAMS: /* @__PURE__ */ AddDeviceParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        list: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(
              object({
                format: wire(
                  IoFormat.nullable().optional(),
                  IoFormat.nullable(),
                ),
              }),
              empty,
            ),
            ListDevicesParams,
          ),
          _RETURN: /* @__PURE__ */ WgSubnetConfig,
        },
        remove: {
          _PARAMS: /* @__PURE__ */ RemoveDeviceParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-auto-port-forward': {
          _PARAMS: /* @__PURE__ */ SetAutoPortForwardParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-dns-injection': {
          _PARAMS: /* @__PURE__ */ SetDnsInjectionParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-kind': {
          _PARAMS: /* @__PURE__ */ SetDeviceKindParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-wan': {
          _PARAMS: /* @__PURE__ */ SetDeviceWanParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'show-config': {
          _PARAMS: /* @__PURE__ */ ShowConfigParams,
          _RETURN: /* @__PURE__ */ z.string(),
        },
      },
    },
    dns: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        add: {
          _PARAMS: /* @__PURE__ */ AddDnsRecordParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        list: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(
              object({
                format: wire(
                  IoFormat.nullable().optional(),
                  IoFormat.nullable(),
                ),
              }),
              empty,
            ),
            empty,
          ),
          _RETURN: /* @__PURE__ */ z.array(DnsRecordEntry),
        },
        remove: {
          _PARAMS: /* @__PURE__ */ RemoveDnsRecordParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
    'http-redirect': {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        list: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(
              object({
                format: wire(
                  IoFormat.nullable().optional(),
                  IoFormat.nullable(),
                ),
              }),
              empty,
            ),
            empty,
          ),
          _RETURN: /* @__PURE__ */ z.array(HttpRedirectStatus),
        },
        'set-enabled': {
          _PARAMS: /* @__PURE__ */ SetHttpRedirectEnabledParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
    pinhole: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        add: {
          _PARAMS: /* @__PURE__ */ AddPinholeParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        remove: {
          _PARAMS: /* @__PURE__ */ RemovePinholeParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-enabled': {
          _PARAMS: /* @__PURE__ */ SetPinholeEnabledParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'update-label': {
          _PARAMS: /* @__PURE__ */ UpdatePinholeLabelParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
    'port-forward': {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        add: {
          _PARAMS: /* @__PURE__ */ AddPortForwardParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        remove: {
          _PARAMS: /* @__PURE__ */ RemovePortForwardParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-enabled': {
          _PARAMS: /* @__PURE__ */ SetPortForwardEnabledParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'update-label': {
          _PARAMS: /* @__PURE__ */ UpdatePortForwardLabelParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
    restart: {
      _PARAMS: /* @__PURE__ */ empty,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    subnet: {
      _PARAMS: /* @__PURE__ */ SubnetParams,
      _CHILDREN: {
        add: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            SubnetParams,
            AddSubnetParams,
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        remove: {
          _PARAMS: /* @__PURE__ */ SubnetParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-dns': {
          _PARAMS: /* @__PURE__ */ z.intersection(
            SubnetParams,
            SetSubnetDnsParams,
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-ipv6': {
          _PARAMS: /* @__PURE__ */ z.intersection(
            SubnetParams,
            SetSubnetIpv6Params,
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-wan': {
          _PARAMS: /* @__PURE__ */ z.intersection(
            SubnetParams,
            SetSubnetWanParams,
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
    update: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        apply: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(
              object({
                format: wire(
                  IoFormat.nullable().optional(),
                  IoFormat.nullable(),
                ),
              }),
              empty,
            ),
            empty,
          ),
          _RETURN: /* @__PURE__ */ TunnelUpdateResult,
        },
        check: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(
              object({
                format: wire(
                  IoFormat.nullable().optional(),
                  IoFormat.nullable(),
                ),
              }),
              empty,
            ),
            empty,
          ),
          _RETURN: /* @__PURE__ */ TunnelUpdateResult,
        },
      },
    },
    web: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        disable: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        enable: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'generate-certificate': {
          _PARAMS: /* @__PURE__ */ GenerateCertParams,
          _RETURN: /* @__PURE__ */ literal_0,
        },
        'get-available-ips': {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(
              object({
                format: wire(
                  IoFormat.nullable().optional(),
                  IoFormat.nullable(),
                ),
              }),
              empty,
            ),
            empty,
          ),
          _RETURN: /* @__PURE__ */ z.array(z.string()),
        },
        'get-certificate': {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(
              object({
                format: wire(
                  IoFormat.nullable().optional(),
                  IoFormat.nullable(),
                ),
              }),
              empty,
            ),
            empty,
          ),
          _RETURN: /* @__PURE__ */ literal_0.nullable(),
        },
        'get-listen': {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(
              object({
                format: wire(
                  IoFormat.nullable().optional(),
                  IoFormat.nullable(),
                ),
              }),
              empty,
            ),
            empty,
          ),
          _RETURN: /* @__PURE__ */ z.string().nullable(),
        },
        'import-certificate': {
          _PARAMS: /* @__PURE__ */ TunnelCertData,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-listen': {
          _PARAMS: /* @__PURE__ */ SetListenParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        uninit: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
  },
}
/**
 * MCP tool descriptions of a schema tree's callable methods, the catalog
 * `rpc_toolkit::mcp::McpServer` serves. Methods without bindings are omitted;
 * an `outputSchema` describes object results only, as MCP requires.
 */
export function toolCatalog(tree: unknown): object[] {
  type JsonSchema = {
    type?: unknown
    allOf?: unknown
    $ref?: unknown
    $defs?: { [name: string]: JsonSchema }
    [keyword: string]: unknown
  }
  type Node = {
    _PARAMS: z.ZodType
    _RETURN?: z.ZodType
    _CHILDREN?: { [name: string]: Node }
    _OPAQUE?: true
  }
  const render = (schema: z.ZodType, io: 'input' | 'output'): JsonSchema => {
    const rendered = z.toJSONSchema(schema, {
      io,
      unrepresentable: 'any',
    }) as JsonSchema
    const ref = rendered.$ref
    const root =
      typeof ref === 'string' && ref.startsWith('#/$defs/')
        ? rendered.$defs?.[ref.slice('#/$defs/'.length)]
        : undefined
    if (!root) return rendered
    const { $ref, ...rest } = rendered
    return { ...rest, ...root }
  }
  const isObject = (schema: JsonSchema) =>
    schema.type === 'object' ||
    (schema.type === undefined && Array.isArray(schema.allOf))
  const tools: object[] = []
  const walk = (node: Node, path: string[]) => {
    if (path.length && node._RETURN && !node._OPAQUE) {
      const input = render(node._PARAMS, 'input')
      if (input.type === undefined || input.type === 'object') {
        const output = render(node._RETURN, 'output')
        tools.push({
          name: path.join('.'),
          inputSchema: { ...input, type: 'object' },
          ...(isObject(output)
            ? { outputSchema: { ...output, type: 'object' } }
            : {}),
        })
      }
    }
    for (const [name, child] of Object.entries(node._CHILDREN ?? {})) {
      walk(child, [...path, name])
    }
  }
  walk(tree as Node, [])
  return tools
}
