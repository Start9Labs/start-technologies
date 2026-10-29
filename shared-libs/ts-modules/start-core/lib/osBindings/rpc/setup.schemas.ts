import { z } from 'zod'
const wire = <A extends z.ZodType, B extends z.ZodType>(input: A, output: B) =>
  z.codec(input, output, { decode: v => v as never, encode: v => v as never })
const object = <Shape extends z.core.$ZodLooseShape>(shape: Shape) =>
  z.object(shape).meta({ additionalProperties: true })
const empty: z.ZodType<{}, {}> = /* @__PURE__ */ object({})
const literal_0 = /* @__PURE__ */ z.string()
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
export const Cifs: z.ZodObject<{
  hostname: z.ZodString
  path: z.ZodString
  username: z.ZodString
  password: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  hostname: z.string(),
  path: z.string(),
  username: z.string(),
  password: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'Cifs',
})
export const DataDrive: z.ZodObject<{
  stablePath: z.ZodString
  wipe: z.ZodBoolean
}> = /* @__PURE__ */ object({ stablePath: z.string(), wipe: z.boolean() }).meta(
  {
    id: 'DataDrive',
  },
)
export const DnsSource: z.ZodEnum<{
  gateway: 'gateway'
  static: 'static'
  'public-fallback': 'public-fallback'
}> = /* @__PURE__ */ z.enum(['gateway', 'static', 'public-fallback']).meta({
  id: 'DnsSource',
})
export const EncryptedWire: z.ZodObject<{ encrypted: z.ZodUnknown }> =
  /* @__PURE__ */ object({ encrypted: z.unknown() }).meta({
    id: 'EncryptedWire',
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
export const Guid: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'Guid',
})
export const HostId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'HostId',
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
export const KeyboardOptions: z.ZodObject<{
  layout: z.ZodString
  keymap: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  model: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  variant: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  options: z.ZodCodec<
    z.ZodOptional<z.ZodArray<z.ZodString>>,
    z.ZodArray<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  layout: z.string(),
  keymap: wire(z.string().nullable().optional(), z.string().nullable()),
  model: wire(z.string().nullable().optional(), z.string().nullable()),
  variant: wire(z.string().nullable().optional(), z.string().nullable()),
  options: wire(z.array(z.string()).optional(), z.array(z.string())),
}).meta({
  id: 'KeyboardOptions',
})
export const LogEntry: z.ZodObject<{
  timestamp: z.ZodString
  message: z.ZodString
  bootId: z.ZodString
}> = /* @__PURE__ */ object({
  timestamp: z.string(),
  message: z.string(),
  bootId: z.string(),
}).meta({
  id: 'LogEntry',
})
export const NamedProgress: z.ZodObject<{
  name: z.ZodString
  progress: typeof Progress
}> = /* @__PURE__ */ object({
  name: z.string(),
  get progress(): typeof Progress {
    return Progress
  },
}).meta({
  id: 'NamedProgress',
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
export const PackageId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'PackageId',
})
export const PartitionTable: z.ZodEnum<{ mbr: 'mbr'; gpt: 'gpt' }> =
  /* @__PURE__ */ z.enum(['mbr', 'gpt']).meta({
    id: 'PartitionTable',
  })
export const ProgressUnits: z.ZodEnum<{ bytes: 'bytes'; steps: 'steps' }> =
  /* @__PURE__ */ z.enum(['bytes', 'steps']).meta({
    id: 'ProgressUnits',
  })
export const QueryDnsParams: z.ZodObject<{ fqdn: z.ZodString }> =
  /* @__PURE__ */ object({ fqdn: z.string() }).meta({
    id: 'QueryDnsParams',
  })

/**
 * What a public domain currently resolves to, per address family. A public
 * domain is DualStack: the operator points an `A` record at the gateway's WAN
 * IPv4 and an `AAAA` at the box's IPv6 GUA. Either may be absent.
 */
export const QueryDnsRes: z.ZodObject<{
  ipv4: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  ipv6: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  ipv4: wire(z.string().nullable().optional(), z.string().nullable()),
  ipv6: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'QueryDnsRes',
  description:
    "What a public domain currently resolves to, per address family. A public\ndomain is DualStack: the operator points an `A` record at the gateway's WAN\nIPv4 and an `AAAA` at the box's IPv6 GUA. Either may be absent.",
})

/**
 * A migration or backup recovery source with a caller-selected backup password representation.
 */
export const RecoverySource = <Password extends z.ZodType>(
  Password: Password,
) => {
  const self = z.union([
    z.intersection(
      object({ type: z.literal('migrate') }),
      object({ guid: z.string() }),
    ),
    z.intersection(
      object({ type: z.literal('backup') }),
      object({
        target: BackupTargetFS,
        password: Password,
        serverId: z.string(),
      }),
    ),
  ])
  return self
}
export const ServerHostname: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'ServerHostname',
})
export const SetLanguageParams: z.ZodObject<{ language: z.ZodString }> =
  /* @__PURE__ */ object({ language: z.string() }).meta({
    id: 'SetLanguageParams',
  })
export const SetupInfo: z.ZodObject<{
  guid: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  attach: z.ZodBoolean
  mokEnrolled: z.ZodBoolean
  /**
   * The whole disk the OS is installed on, recorded by whatever installed it
   * (os_install, or init_resize for a pre-installed image). When the device
   * is pre-installed the wizard fixes this as the OS drive and asks only for
   * a data drive.
   */
  osDrive: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  guid: wire(z.string().nullable().optional(), z.string().nullable()),
  attach: z.boolean(),
  mokEnrolled: z.boolean(),
  /**
   * The whole disk the OS is installed on, recorded by whatever installed it
   * (os_install, or init_resize for a pre-installed image). When the device
   * is pre-installed the wizard fixes this as the OS drive and asks only for
   * a data drive.
   */
  osDrive: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      'The whole disk the OS is installed on, recorded by whatever installed it\n(os_install, or init_resize for a pre-installed image). When the device\nis pre-installed the wizard fixes this as the OS drive and asks only for\na data drive.',
  }),
}).meta({
  id: 'SetupInfo',
})
export const AttachParams: z.ZodObject<{
  password: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof EncryptedWire>>,
    z.ZodNullable<typeof EncryptedWire>
  >
  guid: z.ZodString
  kiosk: z.ZodBoolean
}> = /* @__PURE__ */ object({
  password: wire(EncryptedWire.nullable().optional(), EncryptedWire.nullable()),
  guid: z.string(),
  kiosk: z.boolean(),
}).meta({
  id: 'AttachParams',
})
export const BackupTargetFS: z.ZodUnion<
  readonly [
    z.ZodIntersection<
      z.ZodObject<{ type: z.ZodLiteral<'disk'> }>,
      z.ZodObject<{ logicalname: z.ZodString }>
    >,
    z.ZodIntersection<z.ZodObject<{ type: z.ZodLiteral<'cifs'> }>, typeof Cifs>,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(
      object({ type: z.literal('disk') }),
      object({ logicalname: z.string() }),
    ),
    z.intersection(object({ type: z.literal('cifs') }), Cifs),
  ])
  .meta({
    id: 'BackupTargetFS',
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
export const FullProgress: z.ZodObject<{
  overall: typeof Progress
  phases: z.ZodArray<typeof NamedProgress>
}> = /* @__PURE__ */ object({
  get overall(): typeof Progress {
    return Progress
  },
  phases: z.array(NamedProgress),
}).meta({
  id: 'FullProgress',
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
export const InstallOsParams: z.ZodObject<{
  osDrive: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  dataDrive: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof DataDrive>>,
    z.ZodNullable<typeof DataDrive>
  >
}> = /* @__PURE__ */ object({
  osDrive: wire(z.string().nullable().optional(), z.string().nullable()),
  dataDrive: wire(DataDrive.nullable().optional(), DataDrive.nullable()),
}).meta({
  id: 'InstallOsParams',
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
export const LogFollowResponse: z.ZodObject<{
  startCursor: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  guid: typeof Guid
}> = /* @__PURE__ */ object({
  startCursor: wire(z.string().nullable().optional(), z.string().nullable()),
  guid: Guid,
}).meta({
  id: 'LogFollowResponse',
})
export const LogResponse: z.ZodObject<{
  entries: z.ZodArray<typeof LogEntry>
  startCursor: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  endCursor: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  entries: z.array(LogEntry),
  startCursor: wire(z.string().nullable().optional(), z.string().nullable()),
  endCursor: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'LogResponse',
})
export const RequiresHostId: z.ZodObject<{ host: typeof HostId }> =
  /* @__PURE__ */ object({ host: HostId }).meta({
    id: 'RequiresHostId',
  })
export const RequiresPackageId: z.ZodObject<{ package: typeof PackageId }> =
  /* @__PURE__ */ object({ package: PackageId }).meta({
    id: 'RequiresPackageId',
  })
export const SetupExecuteParams: z.ZodObject<{
  guid: z.ZodString
  password: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof EncryptedWire>>,
    z.ZodNullable<typeof EncryptedWire>
  >
  recoverySource: z.ZodCodec<
    z.ZodOptional<
      z.ZodNullable<ReturnType<typeof RecoverySource<typeof EncryptedWire>>>
    >,
    z.ZodNullable<ReturnType<typeof RecoverySource<typeof EncryptedWire>>>
  >
  kiosk: z.ZodBoolean
  hostname: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  guid: z.string(),
  password: wire(EncryptedWire.nullable().optional(), EncryptedWire.nullable()),
  recoverySource: wire(
    RecoverySource(EncryptedWire).nullable().optional(),
    RecoverySource(EncryptedWire).nullable(),
  ),
  kiosk: z.boolean(),
  hostname: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'SetupExecuteParams',
})
export const SetupResult: z.ZodObject<{
  hostname: typeof ServerHostname
  rootCa: typeof literal_0
  needsRestart: z.ZodBoolean
}> = /* @__PURE__ */ object({
  hostname: ServerHostname,
  rootCa: literal_0,
  needsRestart: z.boolean(),
}).meta({
  id: 'SetupResult',
})

/**
 * The public view of a backup found on a target: enough to identify it, and none of
 * [`BackupUnencryptedMetadata`]'s key material.
 */
export const StartOsRecoveryInfo: z.ZodObject<{
  hostname: typeof ServerHostname
  version: z.ZodString
  timestamp: z.ZodString
}> = /* @__PURE__ */ object({
  hostname: ServerHostname,
  version: z.string(),
  timestamp: z.string(),
}).meta({
  id: 'StartOsRecoveryInfo',
  description:
    "The public view of a backup found on a target: enough to identify it, and none of\n[`BackupUnencryptedMetadata`]'s key material.",
})
export const VerifyCifsParams: z.ZodObject<{
  hostname: z.ZodString
  path: z.ZodString
  username: z.ZodString
  password: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof EncryptedWire>>,
    z.ZodNullable<typeof EncryptedWire>
  >
}> = /* @__PURE__ */ object({
  hostname: z.string(),
  path: z.string(),
  username: z.string(),
  password: wire(EncryptedWire.nullable().optional(), EncryptedWire.nullable()),
}).meta({
  id: 'VerifyCifsParams',
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
export const PartitionInfo: z.ZodObject<{
  logicalname: z.ZodString
  stablePath: z.ZodString
  label: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  capacity: z.ZodInt
  used: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  available: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  startOs: z.ZodRecord<z.ZodString, typeof StartOsRecoveryInfo>
  legacyBackup: z.ZodBoolean
  guid: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  filesystem: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  logicalname: z.string(),
  stablePath: z.string(),
  label: wire(z.string().nullable().optional(), z.string().nullable()),
  capacity: z.int(),
  used: wire(z.int().nullable().optional(), z.int().nullable()),
  available: wire(z.int().nullable().optional(), z.int().nullable()),
  startOs: z.record(z.string(), StartOsRecoveryInfo),
  legacyBackup: z.boolean(),
  guid: wire(z.string().nullable().optional(), z.string().nullable()),
  filesystem: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'PartitionInfo',
})
export const Progress: z.ZodUnion<
  readonly [
    z.ZodNull,
    z.ZodBoolean,
    z.ZodObject<{
      done: z.ZodInt
      total: z.ZodCodec<
        z.ZodOptional<z.ZodNullable<z.ZodInt>>,
        z.ZodNullable<z.ZodInt>
      >
      units: z.ZodCodec<
        z.ZodOptional<z.ZodNullable<typeof ProgressUnits>>,
        z.ZodNullable<typeof ProgressUnits>
      >
    }>,
    typeof FullProgress,
  ]
> = /* @__PURE__ */ z
  .union([
    z.null(),
    z.boolean(),
    object({
      done: z.int(),
      total: wire(z.int().nullable().optional(), z.int().nullable()),
      units: wire(
        ProgressUnits.nullable().optional(),
        ProgressUnits.nullable(),
      ),
    }),
    FullProgress.meta({
      description:
        'A sub-tree of progress. Used by `NamedProgress.progress` so a phase\ncan carry its own `FullProgress` with sub-phases. Producers are\nexpected to keep `FullProgress.overall` scalar (not nested again).',
    }),
  ])
  .meta({
    id: 'Progress',
  })
export const SetupProgress: z.ZodObject<{
  progress: typeof FullProgress
  guid: typeof Guid
}> = /* @__PURE__ */ object({ progress: FullProgress, guid: Guid }).meta({
  id: 'SetupProgress',
})
export const DiskInfo: z.ZodObject<{
  logicalname: z.ZodString
  stablePath: z.ZodString
  partitionTable: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof PartitionTable>>,
    z.ZodNullable<typeof PartitionTable>
  >
  vendor: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  model: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  partitions: z.ZodArray<typeof PartitionInfo>
  capacity: z.ZodInt
  guid: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  filesystem: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  logicalname: z.string(),
  stablePath: z.string(),
  partitionTable: wire(
    PartitionTable.nullable().optional(),
    PartitionTable.nullable(),
  ),
  vendor: wire(z.string().nullable().optional(), z.string().nullable()),
  model: wire(z.string().nullable().optional(), z.string().nullable()),
  partitions: z.array(PartitionInfo),
  capacity: z.int(),
  guid: wire(z.string().nullable().optional(), z.string().nullable()),
  filesystem: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'DiskInfo',
})
export const NetworkGatewayInfo: z.ZodObject<{
  id: typeof GatewayId
  info: typeof NetworkInterfaceInfo
}> = /* @__PURE__ */ object({ id: GatewayId, info: NetworkInterfaceInfo }).meta(
  {
    id: 'NetworkGatewayInfo',
  },
)
export const SetupStatusRes: z.ZodUnion<
  readonly [
    z.ZodObject<{ status: z.ZodLiteral<'needs-install'> }>,
    z.ZodIntersection<
      z.ZodObject<{ status: z.ZodLiteral<'incomplete'> }>,
      typeof SetupInfo
    >,
    z.ZodIntersection<
      z.ZodObject<{ status: z.ZodLiteral<'running'> }>,
      typeof SetupProgress
    >,
    z.ZodIntersection<
      z.ZodObject<{ status: z.ZodLiteral<'complete'> }>,
      typeof SetupResult
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    object({ status: z.literal('needs-install') }),
    z.intersection(object({ status: z.literal('incomplete') }), SetupInfo),
    z.intersection(object({ status: z.literal('running') }), SetupProgress),
    z.intersection(object({ status: z.literal('complete') }), SetupResult),
  ])
  .meta({
    id: 'SetupStatusRes',
  })

/**
 * Active DNS authority and its bootstrap gateway.
 */
export const DnsStatus: z.ZodObject<{
  /**
   * True until stored network policy takes authority.
   */
  bootstrap: z.ZodBoolean
  /**
   * Present only before stored network policy takes authority.
   */
  gateway: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof NetworkGatewayInfo>>,
    z.ZodNullable<typeof NetworkGatewayInfo>
  >
}> = /* @__PURE__ */ object({
  /**
   * True until stored network policy takes authority.
   */
  bootstrap: z.boolean().meta({
    description: 'True until stored network policy takes authority.',
  }),
  /**
   * Present only before stored network policy takes authority.
   */
  gateway: wire(
    NetworkGatewayInfo.nullable().optional(),
    NetworkGatewayInfo.nullable(),
  ).meta({
    description: 'Present only before stored network policy takes authority.',
  }),
}).meta({
  id: 'DnsStatus',
  description: 'Active DNS authority and its bootstrap gateway.',
})
export const Api = {
  _PARAMS: /* @__PURE__ */ empty,
  _CHILDREN: {
    auth: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        session: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
        token: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
      },
    },
    backup: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        target: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            cifs: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
          },
        },
      },
    },
    db: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: { put: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} } },
    },
    diagnostic: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        disk: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            forget: {
              _PARAMS: /* @__PURE__ */ empty,
              _RETURN: /* @__PURE__ */ z.null(),
            },
            repair: {
              _PARAMS: /* @__PURE__ */ empty,
              _RETURN: /* @__PURE__ */ z.null(),
            },
          },
        },
      },
    },
    disk: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        repair: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
    'git-info': {
      _PARAMS: /* @__PURE__ */ empty,
      _RETURN: /* @__PURE__ */ z.string(),
    },
    init: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
    kiosk: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
    net: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        acme: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
        dns: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            query: {
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
                QueryDnsParams,
              ),
              _RETURN: /* @__PURE__ */ QueryDnsRes,
            },
            status: {
              _PARAMS: /* @__PURE__ */ empty,
              _RETURN: /* @__PURE__ */ DnsStatus,
            },
          },
        },
        forward: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
        gateway: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
        ssl: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
        tunnel: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
        vhost: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
      },
    },
    notification: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: { push: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} } },
    },
    package: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        action: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
        backup: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
        host: {
          _PARAMS: /* @__PURE__ */ RequiresPackageId,
          _CHILDREN: {
            address: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                RequiresPackageId,
                RequiresHostId,
              ),
              _CHILDREN: {
                domain: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    RequiresPackageId,
                    RequiresHostId,
                  ),
                  _CHILDREN: {
                    private: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        RequiresPackageId,
                        RequiresHostId,
                      ),
                      _CHILDREN: {},
                    },
                    public: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        RequiresPackageId,
                        RequiresHostId,
                      ),
                      _CHILDREN: {},
                    },
                  },
                },
              },
            },
            binding: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                RequiresPackageId,
                RequiresHostId,
              ),
              _CHILDREN: {},
            },
          },
        },
      },
    },
    server: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        'device-info': {
          _PARAMS: /* @__PURE__ */ z.intersection(
            object({
              format: wire(IoFormat.nullable().optional(), IoFormat.nullable()),
            }),
            empty,
          ),
          _CHILDREN: {},
        },
        experimental: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
        host: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            address: {
              _PARAMS: /* @__PURE__ */ empty,
              _CHILDREN: {
                domain: {
                  _PARAMS: /* @__PURE__ */ empty,
                  _CHILDREN: {
                    private: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
                    public: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
                  },
                },
              },
            },
            binding: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
          },
        },
        metrics: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            object({
              format: wire(IoFormat.nullable().optional(), IoFormat.nullable()),
            }),
            empty,
          ),
          _CHILDREN: {},
        },
        'update-signers': { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
      },
    },
    setup: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        attach: {
          _PARAMS: /* @__PURE__ */ AttachParams,
          _RETURN: /* @__PURE__ */ SetupProgress,
        },
        cifs: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            verify: {
              _PARAMS: /* @__PURE__ */ VerifyCifsParams,
              _RETURN: /* @__PURE__ */ z.record(
                z.string(),
                StartOsRecoveryInfo,
              ),
            },
          },
        },
        complete: {
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
          _RETURN: /* @__PURE__ */ SetupResult,
        },
        disk: {
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
              _RETURN: /* @__PURE__ */ z.array(DiskInfo),
            },
          },
        },
        execute: {
          _PARAMS: /* @__PURE__ */ SetupExecuteParams,
          _RETURN: /* @__PURE__ */ SetupProgress,
        },
        exit: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'get-pubkey': {
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
          _RETURN: /* @__PURE__ */ z.record(z.string(), z.unknown()),
        },
        'install-os': {
          _PARAMS: /* @__PURE__ */ InstallOsParams,
          _RETURN: /* @__PURE__ */ SetupInfo,
        },
        logs: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            object({
              limit: wire(z.int().nullable().optional(), z.int().nullable()),
              cursor: wire(
                z.string().nullable().optional(),
                z.string().nullable(),
              ),
              boot: wire(
                z.union([z.number(), z.string()]).nullable().optional(),
                z.union([z.number(), z.string()]).nullable(),
              ),
              before: wire(z.boolean().optional(), z.boolean()),
            }),
            empty,
          ),
          _RETURN: /* @__PURE__ */ LogResponse,
          _CHILDREN: {
            follow: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                object({
                  limit: wire(
                    z.int().nullable().optional(),
                    z.int().nullable(),
                  ),
                  cursor: wire(
                    z.string().nullable().optional(),
                    z.string().nullable(),
                  ),
                  boot: wire(
                    z.union([z.number(), z.string()]).nullable().optional(),
                    z.union([z.number(), z.string()]).nullable(),
                  ),
                  before: wire(z.boolean().optional(), z.boolean()),
                }),
                empty,
              ),
              _RETURN: /* @__PURE__ */ LogFollowResponse,
            },
          },
        },
        restart: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-keyboard': {
          _PARAMS: /* @__PURE__ */ KeyboardOptions,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-language': {
          _PARAMS: /* @__PURE__ */ SetLanguageParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        shutdown: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        status: {
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
          _RETURN: /* @__PURE__ */ SetupStatusRes,
        },
      },
    },
    ssh: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
    util: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
    wifi: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        available: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
        country: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
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
