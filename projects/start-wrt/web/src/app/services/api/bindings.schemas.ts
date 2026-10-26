import { z } from 'zod'
const wire = <A extends z.ZodType, B extends z.ZodType>(input: A, output: B) =>
  z.codec(input, output, { decode: v => v as never, encode: v => v as never })
const object = <Shape extends z.core.$ZodLooseShape>(shape: Shape) =>
  z.object(shape).meta({ additionalProperties: true })
const empty: z.ZodType<{}, {}> = /* @__PURE__ */ object({})
const literal_0 = /* @__PURE__ */ z.string()
export const ActivityDeleteParams: z.ZodObject<{ id: z.ZodInt }> =
  /* @__PURE__ */ object({ id: z.int() }).meta({
    id: 'ActivityDeleteParams',
  })
export const ActivityEntry: z.ZodObject<{
  id: z.ZodInt
  timestamp: z.ZodString
  category: z.ZodString
  action: z.ZodString
  success: z.ZodBoolean
  summary: z.ZodString
  error: z.ZodOptional<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  id: z.int(),
  timestamp: z.string(),
  category: z.string(),
  action: z.string(),
  success: z.boolean(),
  summary: z.string(),
  error: z.string().nullable().optional(),
}).meta({
  id: 'ActivityEntry',
})
export const ActivityListParams: z.ZodObject<{
  offset: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  limit: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
}> = /* @__PURE__ */ object({
  offset: wire(z.int().nullable().optional(), z.int().nullable()),
  limit: wire(z.int().nullable().optional(), z.int().nullable()),
}).meta({
  id: 'ActivityListParams',
})

/**
 * A published port that will be removed because the device it forwards to can
 * no longer be reached at its current address — its security profile is changing
 * (ethernet-port or WiFi-password reassignment) or its WiFi password is being
 * deleted (disconnecting it) — so its DNAT rule, bound to the device's old
 * subnet, would otherwise break. Surfaced to the UI to confirm the deletion.
 */
export const AffectedPublishedPort: z.ZodObject<{
  id: z.ZodString
  label: z.ZodString
  device_mac: z.ZodString
  device_name: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  id: z.string(),
  label: z.string(),
  device_mac: z.string(),
  device_name: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'AffectedPublishedPort',
  description:
    "A published port that will be removed because the device it forwards to can\nno longer be reached at its current address — its security profile is changing\n(ethernet-port or WiFi-password reassignment) or its WiFi password is being\ndeleted (disconnecting it) — so its DNAT rule, bound to the device's old\nsubnet, would otherwise break. Surfaced to the UI to confirm the deletion.",
})
export const AnyVerifyingKey: typeof literal_0 = /* @__PURE__ */ literal_0.meta(
  {
    id: 'AnyVerifyingKey',
  },
)
export const ApChannel: z.ZodObject<{
  channel: z.ZodInt
  /**
   * Widths in MHz whose every bonded channel is usable.
   */
  widths: z.ZodArray<z.ZodInt>
}> = /* @__PURE__ */ object({
  channel: z.int(),
  /**
   * Widths in MHz whose every bonded channel is usable.
   */
  widths: z.array(z.int()).meta({
    description: 'Widths in MHz whose every bonded channel is usable.',
  }),
}).meta({
  id: 'ApChannel',
})

/**
 * One automatic port use for the published-ports UI.
 */
export const AutomaticPortUse: z.ZodObject<{
  /**
   * UCI section name (`apf_<mac>_<extport>`).
   */
  id: z.ZodString
  /**
   * Which mechanism created it ("PCP", "UPnP", or "SNI").
   */
  kind: z.ZodString
  device_mac: z.ZodString
  device_name: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Forward target address on the LAN.
   */
  internal_ip: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Internal port (or range).
   */
  ports: z.ZodString
  /**
   * External port (or range) on the WAN.
   */
  public_ports: z.ZodString
  /**
   * Seconds until the lease expires if not renewed (None when the daemon
   * isn't tracking it yet, e.g. right after boot).
   */
  expires_secs: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  /**
   * Hostname for an SNI route; absent for plain forwards.
   */
  hostname: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  /**
   * UCI section name (`apf_<mac>_<extport>`).
   */
  id: z.string().meta({
    description: 'UCI section name (`apf_<mac>_<extport>`).',
  }),
  /**
   * Which mechanism created it ("PCP", "UPnP", or "SNI").
   */
  kind: z.string().meta({
    description: 'Which mechanism created it ("PCP", "UPnP", or "SNI").',
  }),
  device_mac: z.string(),
  device_name: wire(z.string().nullable().optional(), z.string().nullable()),
  /**
   * Forward target address on the LAN.
   */
  internal_ip: wire(
    z.string().nullable().optional(),
    z.string().nullable(),
  ).meta({
    description: 'Forward target address on the LAN.',
  }),
  /**
   * Internal port (or range).
   */
  ports: z.string().meta({
    description: 'Internal port (or range).',
  }),
  /**
   * External port (or range) on the WAN.
   */
  public_ports: z.string().meta({
    description: 'External port (or range) on the WAN.',
  }),
  /**
   * Seconds until the lease expires if not renewed (None when the daemon
   * isn't tracking it yet, e.g. right after boot).
   */
  expires_secs: wire(z.int().nullable().optional(), z.int().nullable()).meta({
    description:
      "Seconds until the lease expires if not renewed (None when the daemon\nisn't tracking it yet, e.g. right after boot).",
  }),
  /**
   * Hostname for an SNI route; absent for plain forwards.
   */
  hostname: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description: 'Hostname for an SNI route; absent for plain forwards.',
  }),
}).meta({
  id: 'AutomaticPortUse',
  description: 'One automatic port use for the published-ports UI.',
})
export const BlackoutSetResult: z.ZodObject<{
  /**
   * True (and nothing applied) when a window covers the current time and the
   * caller hasn't confirmed yet. False once the change is applied.
   */
  pendingImmediateBlackout: z.ZodBoolean
}> = /* @__PURE__ */ object({
  /**
   * True (and nothing applied) when a window covers the current time and the
   * caller hasn't confirmed yet. False once the change is applied.
   */
  pendingImmediateBlackout: z.boolean().meta({
    description:
      "True (and nothing applied) when a window covers the current time and the\ncaller hasn't confirmed yet. False once the change is applied.",
  }),
}).meta({
  id: 'BlackoutSetResult',
})
export const BlackoutWindow: z.ZodObject<{
  startTime: z.ZodString
  endTime: z.ZodString
  days: z.ZodTuple<
    [
      z.ZodBoolean,
      z.ZodBoolean,
      z.ZodBoolean,
      z.ZodBoolean,
      z.ZodBoolean,
      z.ZodBoolean,
      z.ZodBoolean,
    ],
    null
  >
}> = /* @__PURE__ */ object({
  startTime: z.string(),
  endTime: z.string(),
  days: z.tuple([
    z.boolean(),
    z.boolean(),
    z.boolean(),
    z.boolean(),
    z.boolean(),
    z.boolean(),
    z.boolean(),
  ]),
}).meta({
  id: 'BlackoutWindow',
})
export const CheckInitializedRes: z.ZodObject<{ initialized: z.ZodBoolean }> =
  /* @__PURE__ */ object({ initialized: z.boolean() }).meta({
    id: 'CheckInitializedRes',
  })
export const DataUsagePeriod: z.ZodEnum<{
  week: 'week'
  month: 'month'
  '3months': '3months'
}> = /* @__PURE__ */ z.enum(['week', 'month', '3months']).meta({
  id: 'DataUsagePeriod',
})
export const DataUsagePoint: z.ZodObject<{
  timestamp: z.ZodInt
  upload: z.ZodInt
  download: z.ZodInt
}> = /* @__PURE__ */ object({
  timestamp: z.int(),
  upload: z.int(),
  download: z.int(),
}).meta({
  id: 'DataUsagePoint',
})
export const DdnsProvider: z.ZodEnum<{
  dyndns: 'dyndns'
  noip: 'noip'
  cloudflare: 'cloudflare'
  duckdns: 'duckdns'
  freedns: 'freedns'
}> = /* @__PURE__ */ z
  .enum(['dyndns', 'noip', 'cloudflare', 'duckdns', 'freedns'])
  .meta({
    id: 'DdnsProvider',
  })
export const DeleteArgs: z.ZodObject<{
  /**
   * Profile interface name
   */
  profile: z.ZodString
}> = /* @__PURE__ */ object({
  /**
   * Profile interface name
   */
  profile: z.string().meta({
    description: 'Profile interface name',
  }),
}).meta({
  id: 'DeleteArgs',
})
export const DeviceMacReq: z.ZodObject<{ mac: z.ZodString }> =
  /* @__PURE__ */ object({ mac: z.string() }).meta({
    id: 'DeviceMacReq',
  })
export const DeviceStatus: z.ZodEnum<{ online: 'online'; offline: 'offline' }> =
  /* @__PURE__ */ z.enum(['online', 'offline']).meta({
    id: 'DeviceStatus',
  })
export const DeviceUpdateReq: z.ZodObject<{
  mac: z.ZodString
  /**
   * Absent leaves the assigned name untouched; empty clears it. Otherwise
   * it must pass [`validate_device_name`].
   */
  name: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  ipv4_static: z.ZodBoolean
  ipv4: z.ZodString
}> = /* @__PURE__ */ object({
  mac: z.string(),
  /**
   * Absent leaves the assigned name untouched; empty clears it. Otherwise
   * it must pass [`validate_device_name`].
   */
  name: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      'Absent leaves the assigned name untouched; empty clears it. Otherwise\nit must pass [`validate_device_name`].',
  }),
  ipv4_static: z.boolean(),
  ipv4: z.string(),
}).meta({
  id: 'DeviceUpdateReq',
})
export const DirGetArgs: z.ZodObject<{ path: z.ZodString }> =
  /* @__PURE__ */ object({ path: z.string() }).meta({
    id: 'DirGetArgs',
  })
export const DiskState: z.ZodObject<{
  /**
   * An eMMC device was found.
   */
  emmcFound: z.ZodBoolean
  /**
   * The eMMC has existing firmware (rootfs partition present).
   */
  hasFirmware: z.ZodBoolean
}> = /* @__PURE__ */ object({
  /**
   * An eMMC device was found.
   */
  emmcFound: z.boolean().meta({
    description: 'An eMMC device was found.',
  }),
  /**
   * The eMMC has existing firmware (rootfs partition present).
   */
  hasFirmware: z.boolean().meta({
    description: 'The eMMC has existing firmware (rootfs partition present).',
  }),
}).meta({
  id: 'DiskState',
})
export const DnsMode: z.ZodEnum<{ isp: 'isp'; custom: 'custom' }> =
  /* @__PURE__ */ z.enum(['isp', 'custom']).meta({
    id: 'DnsMode',
  })

/**
 * A single DNS server entry with protocol info.
 * `ssl: false` = plain UDP on port 53.
 * `ssl: true`  = DNS-over-HTTPS via SmartDNS (`server-https`).
 */
export const DnsServer: z.ZodObject<{
  address: z.ZodString
  ssl: z.ZodBoolean
}> = /* @__PURE__ */ object({ address: z.string(), ssl: z.boolean() }).meta({
  id: 'DnsServer',
  description:
    'A single DNS server entry with protocol info.\n`ssl: false` = plain UDP on port 53.\n`ssl: true`  = DNS-over-HTTPS via SmartDNS (`server-https`).',
})
export const ExecReq: z.ZodObject<{
  command: z.ZodString
  args: z.ZodArray<z.ZodString>
  /**
   * Timeout in milliseconds
   */
  timeout: z.ZodInt
}> = /* @__PURE__ */ object({
  command: z.string(),
  args: z.array(z.string()),
  /**
   * Timeout in milliseconds
   */
  timeout: z.int().meta({
    description: 'Timeout in milliseconds',
  }),
}).meta({
  id: 'ExecReq',
})
export const ExecRes: z.ZodObject<{
  stdout: z.ZodString
  stderr: z.ZodString
  exitCode: z.ZodInt
}> = /* @__PURE__ */ object({
  stdout: z.string(),
  stderr: z.string(),
  exitCode: z.int(),
}).meta({
  id: 'ExecRes',
})
export const FileContents: z.ZodObject<{
  contents: z.ZodString
  modified: z.ZodString
}> = /* @__PURE__ */ object({
  contents: z.string(),
  modified: z.string(),
}).meta({
  id: 'FileContents',
})
export const FileType: z.ZodEnum<{
  'regular-file': 'regular-file'
  directory: 'directory'
  symlink: 'symlink'
  'block-device': 'block-device'
  'char-device': 'char-device'
  fifo: 'fifo'
  socket: 'socket'
}> = /* @__PURE__ */ z
  .enum([
    'regular-file',
    'directory',
    'symlink',
    'block-device',
    'char-device',
    'fifo',
    'socket',
  ])
  .meta({
    id: 'FileType',
  })
export const FollowLogsParams: typeof empty = /* @__PURE__ */ empty.meta({
  id: 'FollowLogsParams',
})
export const GetArgs: z.ZodObject<{ names: z.ZodArray<z.ZodString> }> =
  /* @__PURE__ */ object({ names: z.array(z.string()) }).meta({
    id: 'GetArgs',
  })
export const GetFileArgs: z.ZodObject<{ path: z.ZodString }> =
  /* @__PURE__ */ object({ path: z.string() }).meta({
    id: 'GetFileArgs',
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
export const InjectedDnsRecord: z.ZodObject<{
  name: z.ZodString
  rtype: z.ZodString
  value: z.ZodString
  ttl: z.ZodInt
  /**
   * The injecting device's address, when known.
   */
  source: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Owning LAN device MAC (uppercase); absent for a WireGuard peer.
   */
  owner_mac: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Owning inbound-VPN peer public key; absent for a LAN device.
   */
  owner_peer: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Display name of the owning LAN device, when one is known.
   */
  device_name: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Profile interface whose subnet the record was injected from.
   */
  profile: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  name: z.string(),
  rtype: z.string(),
  value: z.string(),
  ttl: z.int(),
  /**
   * The injecting device's address, when known.
   */
  source: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description: "The injecting device's address, when known.",
  }),
  /**
   * Owning LAN device MAC (uppercase); absent for a WireGuard peer.
   */
  owner_mac: wire(z.string().nullable().optional(), z.string().nullable()).meta(
    {
      description:
        'Owning LAN device MAC (uppercase); absent for a WireGuard peer.',
    },
  ),
  /**
   * Owning inbound-VPN peer public key; absent for a LAN device.
   */
  owner_peer: wire(
    z.string().nullable().optional(),
    z.string().nullable(),
  ).meta({
    description: 'Owning inbound-VPN peer public key; absent for a LAN device.',
  }),
  /**
   * Display name of the owning LAN device, when one is known.
   */
  device_name: wire(
    z.string().nullable().optional(),
    z.string().nullable(),
  ).meta({
    description: 'Display name of the owning LAN device, when one is known.',
  }),
  /**
   * Profile interface whose subnet the record was injected from.
   */
  profile: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description: 'Profile interface whose subnet the record was injected from.',
  }),
}).meta({
  id: 'InjectedDnsRecord',
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
export const KillParams: z.ZodObject<{ ids: z.ZodArray<z.ZodString> }> =
  /* @__PURE__ */ object({ ids: z.array(z.string()) }).meta({
    id: 'KillParams',
  })
export const LanIpv4Response: z.ZodObject<{
  address: z.ZodString
  netmask: z.ZodString
}> = /* @__PURE__ */ object({ address: z.string(), netmask: z.string() }).meta({
  id: 'LanIpv4Response',
})
export const LanIpv4SetRequest: z.ZodObject<{
  address: z.ZodString
  /**
   * When true, forcibly delete VPN peers that would break due to block change.
   */
  force: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  address: z.string(),
  /**
   * When true, forcibly delete VPN peers that would break due to block change.
   */
  force: wire(z.boolean().optional(), z.boolean()).meta({
    description:
      'When true, forcibly delete VPN peers that would break due to block change.',
  }),
}).meta({
  id: 'LanIpv4SetRequest',
})
export const LanIpv6Response: z.ZodObject<{
  slaac: z.ZodBoolean
  dhcpv6: z.ZodBoolean
  /**
   * LAN prefix length in effect: the configured length, lengthened to the
   * delegated prefix when shorter.
   */
  prefix: z.ZodInt
  /**
   * Current IPv6 address (if assigned)
   */
  ip6addr: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Length of the global prefix the ISP delegated; null without one.
   */
  delegated_prefix: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
}> = /* @__PURE__ */ object({
  slaac: z.boolean(),
  dhcpv6: z.boolean(),
  /**
   * LAN prefix length in effect: the configured length, lengthened to the
   * delegated prefix when shorter.
   */
  prefix: z.int().meta({
    description:
      'LAN prefix length in effect: the configured length, lengthened to the\ndelegated prefix when shorter.',
  }),
  /**
   * Current IPv6 address (if assigned)
   */
  ip6addr: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description: 'Current IPv6 address (if assigned)',
  }),
  /**
   * Length of the global prefix the ISP delegated; null without one.
   */
  delegated_prefix: wire(
    z.int().nullable().optional(),
    z.int().nullable(),
  ).meta({
    description:
      'Length of the global prefix the ISP delegated; null without one.',
  }),
}).meta({
  id: 'LanIpv6Response',
})
export const LanIpv6SetRequest: z.ZodObject<{
  slaac: z.ZodBoolean
  dhcpv6: z.ZodBoolean
  /**
   * Absent keeps the configured prefix length.
   */
  prefix: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
}> = /* @__PURE__ */ object({
  slaac: z.boolean(),
  dhcpv6: z.boolean(),
  /**
   * Absent keeps the configured prefix length.
   */
  prefix: wire(z.int().nullable().optional(), z.int().nullable()).meta({
    description: 'Absent keeps the configured prefix length.',
  }),
}).meta({
  id: 'LanIpv6SetRequest',
})
export const ListParams: typeof empty = /* @__PURE__ */ empty.meta({
  id: 'ListParams',
})
export const LogEntry: z.ZodObject<{
  timestamp: z.ZodString
  message: z.ZodString
}> = /* @__PURE__ */ object({
  timestamp: z.string(),
  message: z.string(),
}).meta({
  id: 'LogEntry',
})
export const LogoutParams: typeof empty = /* @__PURE__ */ empty.meta({
  id: 'LogoutParams',
})
export const MacStrategy: z.ZodEnum<{ router: 'router'; custom: 'custom' }> =
  /* @__PURE__ */ z.enum(['router', 'custom']).meta({
    id: 'MacStrategy',
  })
export const OutboundVpnCreateRequest: z.ZodObject<{
  label: z.ZodString
  target: z.ZodString
  /**
   * Raw WireGuard .conf file contents
   */
  config: z.ZodString
  /**
   * Replaces the config's `DNS =` servers. Absent keeps them.
   */
  dns: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<z.ZodString>>>,
    z.ZodNullable<z.ZodArray<z.ZodString>>
  >
}> = /* @__PURE__ */ object({
  label: z.string(),
  target: z.string(),
  /**
   * Raw WireGuard .conf file contents
   */
  config: z.string().meta({
    description: 'Raw WireGuard .conf file contents',
  }),
  /**
   * Replaces the config's `DNS =` servers. Absent keeps them.
   */
  dns: wire(
    z.array(z.string()).nullable().optional(),
    z.array(z.string()).nullable(),
  ).meta({
    description: "Replaces the config's `DNS =` servers. Absent keeps them.",
  }),
}).meta({
  id: 'OutboundVpnCreateRequest',
})
export const OutboundVpnCreateResponse: z.ZodObject<{ id: z.ZodString }> =
  /* @__PURE__ */ object({ id: z.string() }).meta({
    id: 'OutboundVpnCreateResponse',
  })
export const OutboundVpnDeleteRequest: z.ZodObject<{ id: z.ZodString }> =
  /* @__PURE__ */ object({ id: z.string() }).meta({
    id: 'OutboundVpnDeleteRequest',
  })
export const OutboundVpnSetEnabledRequest: z.ZodObject<{
  id: z.ZodString
  enabled: z.ZodBoolean
}> = /* @__PURE__ */ object({ id: z.string(), enabled: z.boolean() }).meta({
  id: 'OutboundVpnSetEnabledRequest',
})
export const OutboundVpnStatus: z.ZodEnum<{
  disabled: 'disabled'
  connected: 'connected'
  idle: 'idle'
  'not-responding': 'not-responding'
}> = /* @__PURE__ */ z
  .enum(['disabled', 'connected', 'idle', 'not-responding'])
  .meta({
    id: 'OutboundVpnStatus',
  })
export const OutboundVpnUpdateRequest: z.ZodObject<{
  id: z.ZodString
  label: z.ZodString
  target: z.ZodString
  /**
   * Desired interface MTU. `None` (or absent) restores the default: the
   * chain MTU for a VPN connecting through another, else the kernel's.
   */
  mtu: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  /**
   * Replaces the resolvers reached through the tunnel. Absent keeps them.
   */
  dns: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<z.ZodString>>>,
    z.ZodNullable<z.ZodArray<z.ZodString>>
  >
}> = /* @__PURE__ */ object({
  id: z.string(),
  label: z.string(),
  target: z.string(),
  /**
   * Desired interface MTU. `None` (or absent) restores the default: the
   * chain MTU for a VPN connecting through another, else the kernel's.
   */
  mtu: wire(z.int().nullable().optional(), z.int().nullable()).meta({
    description:
      "Desired interface MTU. `None` (or absent) restores the default: the\nchain MTU for a VPN connecting through another, else the kernel's.",
  }),
  /**
   * Replaces the resolvers reached through the tunnel. Absent keeps them.
   */
  dns: wire(
    z.array(z.string()).nullable().optional(),
    z.array(z.string()).nullable(),
  ).meta({
    description:
      'Replaces the resolvers reached through the tunnel. Absent keeps them.',
  }),
}).meta({
  id: 'OutboundVpnUpdateRequest',
})

/**
 * Response from adding a peer
 */
export const PeerAddResponse: z.ZodObject<{
  /**
   * WireGuard client configuration. Its private key is a placeholder when the request supplied the public key.
   */
  client_config: z.ZodString
  /**
   * The public key of the peer
   */
  public_key: z.ZodString
  /**
   * The assigned IP address
   */
  ip: z.ZodString
}> = /* @__PURE__ */ object({
  /**
   * WireGuard client configuration. Its private key is a placeholder when the request supplied the public key.
   */
  client_config: z.string().meta({
    description:
      'WireGuard client configuration. Its private key is a placeholder when the request supplied the public key.',
  }),
  /**
   * The public key of the peer
   */
  public_key: z.string().meta({
    description: 'The public key of the peer',
  }),
  /**
   * The assigned IP address
   */
  ip: z.string().meta({
    description: 'The assigned IP address',
  }),
}).meta({
  id: 'PeerAddResponse',
  description: 'Response from adding a peer',
})
export const PeerDeleteArgs: z.ZodObject<{
  /**
   * Profile interface name
   */
  profile: z.ZodString
  /**
   * Public key of the peer to delete
   */
  public_key: z.ZodString
}> = /* @__PURE__ */ object({
  /**
   * Profile interface name
   */
  profile: z.string().meta({
    description: 'Profile interface name',
  }),
  /**
   * Public key of the peer to delete
   */
  public_key: z.string().meta({
    description: 'Public key of the peer to delete',
  }),
}).meta({
  id: 'PeerDeleteArgs',
})
export const PeerRenameArgs: z.ZodObject<{
  /**
   * Profile interface name
   */
  profile: z.ZodString
  /**
   * Public key of the peer to rename
   */
  public_key: z.ZodString
  /**
   * New human-readable name for the peer
   */
  name: z.ZodString
}> = /* @__PURE__ */ object({
  /**
   * Profile interface name
   */
  profile: z.string().meta({
    description: 'Profile interface name',
  }),
  /**
   * Public key of the peer to rename
   */
  public_key: z.string().meta({
    description: 'Public key of the peer to rename',
  }),
  /**
   * New human-readable name for the peer
   */
  name: z.string().meta({
    description: 'New human-readable name for the peer',
  }),
}).meta({
  id: 'PeerRenameArgs',
})
export const PeerSetDnsInjectionArgs: z.ZodObject<{
  /**
   * Profile interface name
   */
  profile: z.ZodString
  /**
   * Public key of the peer
   */
  public_key: z.ZodString
  /**
   * Grant rather than revoke the permission
   */
  allow: z.ZodBoolean
}> = /* @__PURE__ */ object({
  /**
   * Profile interface name
   */
  profile: z.string().meta({
    description: 'Profile interface name',
  }),
  /**
   * Public key of the peer
   */
  public_key: z.string().meta({
    description: 'Public key of the peer',
  }),
  /**
   * Grant rather than revoke the permission
   */
  allow: z.boolean().meta({
    description: 'Grant rather than revoke the permission',
  }),
}).meta({
  id: 'PeerSetDnsInjectionArgs',
})
export const ProfileId: z.ZodObject<{
  fullname: z.ZodString
  interface: z.ZodString
  vlan_tag: z.ZodInt
}> = /* @__PURE__ */ object({
  fullname: z.string(),
  interface: z.string(),
  vlan_tag: z.int(),
}).meta({
  id: 'ProfileId',
})
export const ProfileIdOpt: z.ZodObject<{
  fullname: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  interface: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  vlan_tag: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
}> = /* @__PURE__ */ object({
  fullname: wire(z.string().nullable().optional(), z.string().nullable()),
  interface: wire(z.string().nullable().optional(), z.string().nullable()),
  vlan_tag: wire(z.int().nullable().optional(), z.int().nullable()),
}).meta({
  id: 'ProfileIdOpt',
})
export const Protocol: z.ZodEnum<{
  tcp: 'tcp'
  udp: 'udp'
  'tcp+udp': 'tcp+udp'
}> = /* @__PURE__ */ z.enum(['tcp', 'udp', 'tcp+udp']).meta({
  id: 'Protocol',
})
export const PublishedPortStatus: z.ZodEnum<{
  active: 'active'
  partial: 'partial'
  paused: 'paused'
  error: 'error'
  disabled: 'disabled'
}> = /* @__PURE__ */ z
  .enum(['active', 'partial', 'paused', 'error', 'disabled'])
  .meta({
    id: 'PublishedPortStatus',
  })
export const ResetPasswordParams: z.ZodObject<{
  oldPassword: z.ZodString
  newPassword: z.ZodString
}> = /* @__PURE__ */ object({
  oldPassword: z.string(),
  newPassword: z.string(),
}).meta({
  id: 'ResetPasswordParams',
})
export const ScheduleGetParams: z.ZodObject<{ interface: z.ZodString }> =
  /* @__PURE__ */ object({ interface: z.string() }).meta({
    id: 'ScheduleGetParams',
  })
export const ScheduleWindow: z.ZodObject<{
  startTime: z.ZodString
  endTime: z.ZodString
  days: z.ZodTuple<
    [
      z.ZodBoolean,
      z.ZodBoolean,
      z.ZodBoolean,
      z.ZodBoolean,
      z.ZodBoolean,
      z.ZodBoolean,
      z.ZodBoolean,
    ],
    null
  >
}> = /* @__PURE__ */ object({
  startTime: z.string(),
  endTime: z.string(),
  days: z.tuple([
    z.boolean(),
    z.boolean(),
    z.boolean(),
    z.boolean(),
    z.boolean(),
    z.boolean(),
    z.boolean(),
  ]),
}).meta({
  id: 'ScheduleWindow',
})
export const Section: z.ZodObject<{
  ty: z.ZodString
  name: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  options: z.ZodRecord<z.ZodString, z.ZodString>
  lists: z.ZodRecord<z.ZodString, z.ZodArray<z.ZodString>>
}> = /* @__PURE__ */ object({
  ty: z.string(),
  name: wire(z.string().nullable().optional(), z.string().nullable()),
  options: z.record(z.string(), z.string()),
  lists: z.record(z.string(), z.array(z.string())),
}).meta({
  id: 'Section',
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
export const SetAutoForwardRequest: z.ZodObject<{
  mac: z.ZodString
  allow: z.ZodBoolean
}> = /* @__PURE__ */ object({ mac: z.string(), allow: z.boolean() }).meta({
  id: 'SetAutoForwardRequest',
})
export const SetDnsInjectionReq: z.ZodObject<{
  mac: z.ZodString
  allow: z.ZodBoolean
}> = /* @__PURE__ */ object({ mac: z.string(), allow: z.boolean() }).meta({
  id: 'SetDnsInjectionReq',
})
export const SetFileArgs: z.ZodObject<{
  path: z.ZodString
  contents: z.ZodString
  modified: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  path: z.string(),
  contents: z.string(),
  modified: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'SetFileArgs',
})
export const SetInitialPasswordParams: z.ZodObject<{
  password: z.ZodString
  /**
   * IANA timezone name, e.g. "America/New_York". An unknown name is skipped.
   */
  timezone: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  password: z.string(),
  /**
   * IANA timezone name, e.g. "America/New_York". An unknown name is skipped.
   */
  timezone: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      'IANA timezone name, e.g. "America/New_York". An unknown name is skipped.',
  }),
}).meta({
  id: 'SetInitialPasswordParams',
})
export const SetPreferencesReq: z.ZodObject<{
  language: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  theme: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  remoteAccess: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  language: wire(z.string().nullable().optional(), z.string().nullable()),
  theme: wire(z.string().nullable().optional(), z.string().nullable()),
  remoteAccess: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'SetPreferencesReq',
})
export const SetRouterAdvertisementsReq: z.ZodObject<{
  mac: z.ZodString
  allow: z.ZodBoolean
}> = /* @__PURE__ */ object({ mac: z.string(), allow: z.boolean() }).meta({
  id: 'SetRouterAdvertisementsReq',
})
export const SetTimezoneParams: z.ZodObject<{
  /**
   * IANA timezone name, e.g. "America/New_York"
   */
  timezone: z.ZodString
}> = /* @__PURE__ */ object({
  /**
   * IANA timezone name, e.g. "America/New_York"
   */
  timezone: z.string().meta({
    description: 'IANA timezone name, e.g. "America/New_York"',
  }),
}).meta({
  id: 'SetTimezoneParams',
})
export const SniPortUse: z.ZodObject<{
  ports: z.ZodString
  hostnames: z.ZodArray<z.ZodString>
  devices: z.ZodArray<z.ZodString>
}> = /* @__PURE__ */ object({
  ports: z.string(),
  hostnames: z.array(z.string()),
  devices: z.array(z.string()),
}).meta({
  id: 'SniPortUse',
})
export const SpeedData: z.ZodObject<{
  up: z.ZodCodec<z.ZodNumber, z.ZodNullable<z.ZodNumber>>
  down: z.ZodCodec<z.ZodNumber, z.ZodNullable<z.ZodNumber>>
}> = /* @__PURE__ */ object({
  up: wire(z.number(), z.number().nullable()),
  down: wire(z.number(), z.number().nullable()),
}).meta({
  id: 'SpeedData',
})
export const SshKeyAddParams: z.ZodObject<{ key: z.ZodString }> =
  /* @__PURE__ */ object({ key: z.string() }).meta({
    id: 'SshKeyAddParams',
  })
export const SshKeyDeleteParams: z.ZodObject<{ fingerprint: z.ZodString }> =
  /* @__PURE__ */ object({ fingerprint: z.string() }).meta({
    id: 'SshKeyDeleteParams',
  })
export const SshKeyResponse: z.ZodObject<{
  algorithm: z.ZodString
  fingerprint: z.ZodString
  hostname: z.ZodString
}> = /* @__PURE__ */ object({
  algorithm: z.string(),
  fingerprint: z.string(),
  hostname: z.string(),
}).meta({
  id: 'SshKeyResponse',
})
export const SystemInfoResponse: z.ZodObject<{
  version: z.ZodString
  gitHash: z.ZodString
  language: z.ZodString
  date: z.ZodString
  theme: z.ZodString
  remoteAccess: z.ZodString
  timezone: z.ZodString
}> = /* @__PURE__ */ object({
  version: z.string(),
  gitHash: z.string(),
  language: z.string(),
  date: z.string(),
  theme: z.string(),
  remoteAccess: z.string(),
  timezone: z.string(),
}).meta({
  id: 'SystemInfoResponse',
})
export const UpdateSystemParams: z.ZodObject<{
  registry: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  targetVersion: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  registry: wire(z.string().nullable().optional(), z.string().nullable()),
  targetVersion: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'UpdateSystemParams',
})
export const UpdateSystemRes: z.ZodObject<{
  target: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  progress: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  target: wire(z.string().nullable().optional(), z.string().nullable()),
  progress: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'UpdateSystemRes',
})
export const VerifyPasswordParams: z.ZodObject<{ password: z.ZodString }> =
  /* @__PURE__ */ object({ password: z.string() }).meta({
    id: 'VerifyPasswordParams',
  })
export const VersionInfo: z.ZodObject<{
  version: z.ZodString
  releaseNotes: z.ZodString
}> = /* @__PURE__ */ object({
  version: z.string(),
  releaseNotes: z.string(),
}).meta({
  id: 'VersionInfo',
})

/**
 * VPN server configuration for set/create requests
 */
export const VpnServerConfig: z.ZodObject<{
  /**
   * Human-readable label for the VPN server
   */
  label: z.ZodString
  /**
   * Whether the VPN server is enabled (listening)
   */
  enabled: z.ZodBoolean
  /**
   * Listen port for WireGuard connections
   */
  listen_port: z.ZodInt
  /**
   * Public endpoint address for inbound client connections (hostname or IP)
   */
  endpoint: z.ZodString
  /**
   * Private key. If not provided, a new key will be generated.
   */
  private_key: z.ZodOptional<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  /**
   * Human-readable label for the VPN server
   */
  label: z.string().meta({
    description: 'Human-readable label for the VPN server',
  }),
  /**
   * Whether the VPN server is enabled (listening)
   */
  enabled: z.boolean().meta({
    description: 'Whether the VPN server is enabled (listening)',
  }),
  /**
   * Listen port for WireGuard connections
   */
  listen_port: z.int().meta({
    description: 'Listen port for WireGuard connections',
  }),
  /**
   * Public endpoint address for inbound client connections (hostname or IP)
   */
  endpoint: z.string().meta({
    description:
      'Public endpoint address for inbound client connections (hostname or IP)',
  }),
  /**
   * Private key. If not provided, a new key will be generated.
   */
  private_key: z.string().nullable().optional().meta({
    description: 'Private key. If not provided, a new key will be generated.',
  }),
}).meta({
  id: 'VpnServerConfig',
  description: 'VPN server configuration for set/create requests',
})

/**
 * Peer configuration for VPN server
 */
export const VpnServerPeer: z.ZodObject<{
  /**
   * Human-readable name for the peer
   */
  name: z.ZodString
  /**
   * Assigned IP address within the profile's subnet (auto-allocated if not specified)
   */
  ip: z.ZodOptional<z.ZodNullable<z.ZodString>>
  /**
   * Public key. If not provided when adding a peer, a key pair will be generated.
   */
  public_key: z.ZodOptional<z.ZodNullable<z.ZodString>>
  /**
   * Pre-shared key (auto-generated if not provided)
   */
  preshared_key: z.ZodOptional<z.ZodNullable<z.ZodString>>
  /**
   * Route all traffic (LAN + WAN) through the tunnel. Default (false/absent) = split tunnel (LAN only).
   */
  route_all: z.ZodOptional<z.ZodNullable<z.ZodBoolean>>
  /**
   * May publish DNS records into the router's resolver. Default (false/absent) = denied.
   */
  allow_dns_injection: z.ZodOptional<z.ZodNullable<z.ZodBoolean>>
}> = /* @__PURE__ */ object({
  /**
   * Human-readable name for the peer
   */
  name: z.string().meta({
    description: 'Human-readable name for the peer',
  }),
  /**
   * Assigned IP address within the profile's subnet (auto-allocated if not specified)
   */
  ip: z.string().nullable().optional().meta({
    description:
      "Assigned IP address within the profile's subnet (auto-allocated if not specified)",
  }),
  /**
   * Public key. If not provided when adding a peer, a key pair will be generated.
   */
  public_key: z.string().nullable().optional().meta({
    description:
      'Public key. If not provided when adding a peer, a key pair will be generated.',
  }),
  /**
   * Pre-shared key (auto-generated if not provided)
   */
  preshared_key: z.string().nullable().optional().meta({
    description: 'Pre-shared key (auto-generated if not provided)',
  }),
  /**
   * Route all traffic (LAN + WAN) through the tunnel. Default (false/absent) = split tunnel (LAN only).
   */
  route_all: z.boolean().nullable().optional().meta({
    description:
      'Route all traffic (LAN + WAN) through the tunnel. Default (false/absent) = split tunnel (LAN only).',
  }),
  /**
   * May publish DNS records into the router's resolver. Default (false/absent) = denied.
   */
  allow_dns_injection: z.boolean().nullable().optional().meta({
    description:
      "May publish DNS records into the router's resolver. Default (false/absent) = denied.",
  }),
}).meta({
  id: 'VpnServerPeer',
  description: 'Peer configuration for VPN server',
})
export const WanAccess: z.ZodUnion<
  readonly [
    z.ZodLiteral<'ALL'>,
    z.ZodLiteral<'NONE'>,
    z.ZodObject<{ whitelist: z.ZodArray<z.ZodString> }>,
    z.ZodObject<{ blacklist: z.ZodArray<z.ZodString> }>,
  ]
> = /* @__PURE__ */ z
  .union([
    z.literal('ALL'),
    z.literal('NONE'),
    object({ whitelist: z.array(z.string()) }),
    object({ blacklist: z.array(z.string()) }),
  ])
  .meta({
    id: 'WanAccess',
  })
export const WanIpv4Mode: z.ZodEnum<{
  dhcp: 'dhcp'
  static: 'static'
  pppoe: 'pppoe'
}> = /* @__PURE__ */ z.enum(['dhcp', 'static', 'pppoe']).meta({
  id: 'WanIpv4Mode',
})
export const WanIpv6Mode: z.ZodEnum<{
  disabled: 'disabled'
  slaac: 'slaac'
  dhcpv6: 'dhcpv6'
  static: 'static'
  '6rd': '6rd'
}> = /* @__PURE__ */ z
  .enum(['disabled', 'slaac', 'dhcpv6', 'static', '6rd'])
  .meta({
    id: 'WanIpv6Mode',
  })
export const WifiRadio: z.ZodObject<{
  band: z.ZodString
  channel: z.ZodString
  /**
   * Channel width in MHz. Null when the radio's mode sets none; null on
   * `set` keeps the current width.
   */
  width: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  enabled: z.ZodBoolean
  broadcast: z.ZodBoolean
}> = /* @__PURE__ */ object({
  band: z.string(),
  channel: z.string(),
  /**
   * Channel width in MHz. Null when the radio's mode sets none; null on
   * `set` keeps the current width.
   */
  width: wire(z.int().nullable().optional(), z.int().nullable()).meta({
    description:
      "Channel width in MHz. Null when the radio's mode sets none; null on\n`set` keeps the current width.",
  }),
  enabled: z.boolean(),
  broadcast: z.boolean(),
}).meta({
  id: 'WifiRadio',
})
export const ActivityListResponse: z.ZodObject<{
  entries: z.ZodArray<typeof ActivityEntry>
  total: z.ZodInt
}> = /* @__PURE__ */ object({
  entries: z.array(ActivityEntry),
  total: z.int(),
}).meta({
  id: 'ActivityListResponse',
})

/**
 * The server's enrolled auth keys, keyed by their PEM encoding. Each enrolled
 * key is a sign-in: it carries the same metadata a session used to (when it
 * was created, when it was last used, and the user agent that enrolled it).
 */
export const AuthKeys: z.ZodRecord<z.ZodString, typeof Session> =
  /* @__PURE__ */ z.record(z.string(), Session).meta({
    id: 'AuthKeys',
    description:
      "The server's enrolled auth keys, keyed by their PEM encoding. Each enrolled\nkey is a sign-in: it carries the same metadata a session used to (when it\nwas created, when it was last used, and the user agent that enrolled it).",
  })
export const BackupCreateRes: z.ZodObject<{
  guid: typeof Guid
  filename: z.ZodString
}> = /* @__PURE__ */ object({ guid: Guid, filename: z.string() }).meta({
  id: 'BackupCreateRes',
})
export const BackupRestoreRes: z.ZodObject<{ upload: typeof Guid }> =
  /* @__PURE__ */ object({ upload: Guid }).meta({
    id: 'BackupRestoreRes',
  })
export const BandChannels: z.ZodObject<{
  channels: z.ZodArray<typeof ApChannel>
  /**
   * Widths automatic channel selection can serve without radar-detection channels.
   */
  autoWidths: z.ZodArray<z.ZodInt>
}> = /* @__PURE__ */ object({
  channels: z.array(ApChannel),
  /**
   * Widths automatic channel selection can serve without radar-detection channels.
   */
  autoWidths: z.array(z.int()).meta({
    description:
      'Widths automatic channel selection can serve without radar-detection channels.',
  }),
}).meta({
  id: 'BandChannels',
})
export const BlackoutWindows: z.ZodObject<{
  windows: z.ZodArray<typeof BlackoutWindow>
}> = /* @__PURE__ */ object({ windows: z.array(BlackoutWindow) }).meta({
  id: 'BlackoutWindows',
})
export const DataUsageReq: z.ZodObject<{
  mac: z.ZodString
  period: typeof DataUsagePeriod
}> = /* @__PURE__ */ object({ mac: z.string(), period: DataUsagePeriod }).meta({
  id: 'DataUsageReq',
})
export const Device: z.ZodObject<{
  mac: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Fully-resolved display name: UCI static name → live DHCP hostname →
   * remembered hostname (cache) → `device-<mac>` placeholder.
   */
  name: z.ZodString
  /**
   * The UCI static name; `None` when `name` is resolved from elsewhere.
   * What the rename form edits.
   */
  custom_name: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Raw DHCP lease hostname (may be "*").
   */
  hostname: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  status: typeof DeviceStatus
  connection: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  ipv4: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  ipv6: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  ipv4_static: z.ZodBoolean
  /**
   * Whether this device may auto-create port forwards via PCP/UPnP
   * (default off; set via `devices set-auto-forward`).
   */
  allow_auto_port_forward: z.ZodBoolean
  /**
   * Whether this device may publish DNS records into the router's resolver
   * (`devices set-dns-injection`; `vpn-server peer-set-dns-injection` for a
   * VPN peer).
   */
  allow_dns_injection: z.ZodBoolean
  /**
   * Whether the router forwards this device's IPv6 router advertisements
   * to the rest of the LAN (`devices set-router-advertisements`). Always
   * false for a VPN peer.
   */
  allow_router_advertisements: z.ZodBoolean
  security_profile: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  speed: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof SpeedData>>,
    z.ZodNullable<typeof SpeedData>
  >
  data_usage: z.ZodCodec<
    z.ZodOptional<
      z.ZodNullable<z.ZodCodec<z.ZodNumber, z.ZodNullable<z.ZodNumber>>>
    >,
    z.ZodNullable<z.ZodCodec<z.ZodNumber, z.ZodNullable<z.ZodNumber>>>
  >
}> = /* @__PURE__ */ object({
  mac: wire(z.string().nullable().optional(), z.string().nullable()),
  /**
   * Fully-resolved display name: UCI static name → live DHCP hostname →
   * remembered hostname (cache) → `device-<mac>` placeholder.
   */
  name: z.string().meta({
    description:
      'Fully-resolved display name: UCI static name → live DHCP hostname →\nremembered hostname (cache) → `device-<mac>` placeholder.',
  }),
  /**
   * The UCI static name; `None` when `name` is resolved from elsewhere.
   * What the rename form edits.
   */
  custom_name: wire(
    z.string().nullable().optional(),
    z.string().nullable(),
  ).meta({
    description:
      'The UCI static name; `None` when `name` is resolved from elsewhere.\nWhat the rename form edits.',
  }),
  /**
   * Raw DHCP lease hostname (may be "*").
   */
  hostname: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description: 'Raw DHCP lease hostname (may be "*").',
  }),
  status: DeviceStatus,
  connection: wire(z.string().nullable().optional(), z.string().nullable()),
  ipv4: wire(z.string().nullable().optional(), z.string().nullable()),
  ipv6: wire(z.string().nullable().optional(), z.string().nullable()),
  ipv4_static: z.boolean(),
  /**
   * Whether this device may auto-create port forwards via PCP/UPnP
   * (default off; set via `devices set-auto-forward`).
   */
  allow_auto_port_forward: z.boolean().meta({
    description:
      'Whether this device may auto-create port forwards via PCP/UPnP\n(default off; set via `devices set-auto-forward`).',
  }),
  /**
   * Whether this device may publish DNS records into the router's resolver
   * (`devices set-dns-injection`; `vpn-server peer-set-dns-injection` for a
   * VPN peer).
   */
  allow_dns_injection: z.boolean().meta({
    description:
      "Whether this device may publish DNS records into the router's resolver\n(`devices set-dns-injection`; `vpn-server peer-set-dns-injection` for a\nVPN peer).",
  }),
  /**
   * Whether the router forwards this device's IPv6 router advertisements
   * to the rest of the LAN (`devices set-router-advertisements`). Always
   * false for a VPN peer.
   */
  allow_router_advertisements: z.boolean().meta({
    description:
      "Whether the router forwards this device's IPv6 router advertisements\nto the rest of the LAN (`devices set-router-advertisements`). Always\nfalse for a VPN peer.",
  }),
  security_profile: wire(
    z.string().nullable().optional(),
    z.string().nullable(),
  ),
  speed: wire(SpeedData.nullable().optional(), SpeedData.nullable()),
  data_usage: wire(
    wire(z.number(), z.number().nullable()).nullable().optional(),
    wire(z.number(), z.number().nullable()).nullable(),
  ),
}).meta({
  id: 'Device',
})
export const DiagnosticsCreateRes: z.ZodObject<{
  guid: typeof Guid
  filename: z.ZodString
}> = /* @__PURE__ */ object({ guid: Guid, filename: z.string() }).meta({
  id: 'DiagnosticsCreateRes',
})
export const DirEntry: z.ZodObject<{
  name: z.ZodString
  size: z.ZodInt
  blocks: z.ZodInt
  io_block: z.ZodInt
  file_type: typeof FileType
  device: z.ZodInt
  inode: z.ZodInt
  links: z.ZodInt
  mode: z.ZodInt
  uid: z.ZodInt
  gid: z.ZodInt
  access: z.ZodString
  modify: z.ZodString
  change: z.ZodString
}> = /* @__PURE__ */ object({
  name: z.string(),
  size: z.int(),
  blocks: z.int(),
  io_block: z.int(),
  file_type: FileType,
  device: z.int(),
  inode: z.int(),
  links: z.int(),
  mode: z.int(),
  uid: z.int(),
  gid: z.int(),
  access: z.string(),
  modify: z.string(),
  change: z.string(),
}).meta({
  id: 'DirEntry',
})
export const EditArgs: z.ZodObject<{
  get: typeof ProfileIdOpt
  create: z.ZodBoolean
}> = /* @__PURE__ */ object({ get: ProfileIdOpt, create: z.boolean() }).meta({
  id: 'EditArgs',
})

/**
 * `ethernet.set` request: the desired port layout plus a confirmation flag.
 * Reassigning a port to a different profile moves its devices to a new subnet,
 * which breaks any published ports forwarding to them. When the flag is false
 * and such ports exist, `set` applies nothing and returns them in
 * `EthernetSetResult` for a confirmation dialog; with the flag true it deletes
 * those published ports as part of the reassignment.
 */
export const EthernetSetRequest: z.ZodIntersection<
  z.ZodObject<{
    confirm_published_port_deletion: z.ZodCodec<
      z.ZodOptional<z.ZodBoolean>,
      z.ZodBoolean
    >
  }>,
  z.ZodObject<{
    wan_ipv6: z.ZodBoolean
    wan_port: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<z.ZodString>>,
      z.ZodNullable<z.ZodString>
    >
    ports: z.ZodRecord<
      z.ZodString,
      z.ZodObject<{
        profile: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<typeof ProfileIdOpt>>,
          z.ZodNullable<typeof ProfileIdOpt>
        >
      }>
    >
  }>
> = /* @__PURE__ */ z
  .intersection(
    object({
      confirm_published_port_deletion: wire(
        z.boolean().optional(),
        z.boolean(),
      ),
    }),
    object({
      wan_ipv6: z.boolean(),
      wan_port: wire(z.string().nullable().optional(), z.string().nullable()),
      ports: z.record(
        z.string(),
        object({
          profile: wire(
            ProfileIdOpt.nullable().optional(),
            ProfileIdOpt.nullable(),
          ),
        }),
      ),
    }),
  )
  .meta({
    id: 'EthernetSetRequest',
    description:
      '`ethernet.set` request: the desired port layout plus a confirmation flag.\nReassigning a port to a different profile moves its devices to a new subnet,\nwhich breaks any published ports forwarding to them. When the flag is false\nand such ports exist, `set` applies nothing and returns them in\n`EthernetSetResult` for a confirmation dialog; with the flag true it deletes\nthose published ports as part of the reassignment.',
  })
export const EthernetSetResult: z.ZodObject<{
  /**
   * Non-empty (and nothing applied) when published ports would be deleted and
   * the caller hasn't confirmed yet. Empty once the change is applied.
   */
  pending_published_port_deletions: z.ZodArray<typeof AffectedPublishedPort>
}> = /* @__PURE__ */ object({
  /**
   * Non-empty (and nothing applied) when published ports would be deleted and
   * the caller hasn't confirmed yet. Empty once the change is applied.
   */
  pending_published_port_deletions: z.array(AffectedPublishedPort).meta({
    description:
      "Non-empty (and nothing applied) when published ports would be deleted and\nthe caller hasn't confirmed yet. Empty once the change is applied.",
  }),
}).meta({
  id: 'EthernetSetResult',
})
export const FollowLogsRes: z.ZodObject<{
  /**
   * Streams each new [`LogEntry`] as JSON from `/ws/rpc/{guid}`.
   */
  guid: typeof Guid
}> = /* @__PURE__ */ object({
  /**
   * Streams each new [`LogEntry`] as JSON from `/ws/rpc/{guid}`.
   */
  guid: Guid.meta({
    description: 'Streams each new [`LogEntry`] as JSON from `/ws/rpc/{guid}`.',
  }),
}).meta({
  id: 'FollowLogsRes',
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
export const LogsResponse: z.ZodObject<{
  entries: z.ZodArray<typeof LogEntry>
}> = /* @__PURE__ */ object({ entries: z.array(LogEntry) }).meta({
  id: 'LogsResponse',
})
export const OutboundVpn: z.ZodObject<{
  id: z.ZodString
  label: z.ZodString
  target: z.ZodString
  enabled: z.ZodBoolean
  used_by: z.ZodArray<z.ZodString>
  /**
   * True when the VPN's WireGuard interface has at least one IPv6 address;
   * false means profiles routed through this VPN will have IPv6 disabled
   * (per `outbound_supports_ipv6` in profiles.rs).
   */
  supports_ipv6: z.ZodBoolean
  /**
   * Interface MTU, if explicitly set. `None` means the kernel default
   * (1420 on a clean path) is in effect. Editable via `update`.
   */
  mtu: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  /**
   * The server is named by hostname; only `Internet` is a valid target.
   */
  hostname_endpoint: z.ZodBoolean
  /**
   * Resolvers reached through the tunnel. Empty leaves profiles routed through
   * this VPN on the system DNS servers, or on the ISP's outside the tunnel.
   */
  dns: z.ZodArray<z.ZodString>
  status: typeof OutboundVpnStatus
  /**
   * Seconds since the tunnel's latest handshake, measured on the router.
   * `None` when it has never completed one.
   */
  last_handshake_secs: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
}> = /* @__PURE__ */ object({
  id: z.string(),
  label: z.string(),
  target: z.string(),
  enabled: z.boolean(),
  used_by: z.array(z.string()),
  /**
   * True when the VPN's WireGuard interface has at least one IPv6 address;
   * false means profiles routed through this VPN will have IPv6 disabled
   * (per `outbound_supports_ipv6` in profiles.rs).
   */
  supports_ipv6: z.boolean().meta({
    description:
      "True when the VPN's WireGuard interface has at least one IPv6 address;\nfalse means profiles routed through this VPN will have IPv6 disabled\n(per `outbound_supports_ipv6` in profiles.rs).",
  }),
  /**
   * Interface MTU, if explicitly set. `None` means the kernel default
   * (1420 on a clean path) is in effect. Editable via `update`.
   */
  mtu: wire(z.int().nullable().optional(), z.int().nullable()).meta({
    description:
      'Interface MTU, if explicitly set. `None` means the kernel default\n(1420 on a clean path) is in effect. Editable via `update`.',
  }),
  /**
   * The server is named by hostname; only `Internet` is a valid target.
   */
  hostname_endpoint: z.boolean().meta({
    description:
      'The server is named by hostname; only `Internet` is a valid target.',
  }),
  /**
   * Resolvers reached through the tunnel. Empty leaves profiles routed through
   * this VPN on the system DNS servers, or on the ISP's outside the tunnel.
   */
  dns: z.array(z.string()).meta({
    description:
      "Resolvers reached through the tunnel. Empty leaves profiles routed through\nthis VPN on the system DNS servers, or on the ISP's outside the tunnel.",
  }),
  status: OutboundVpnStatus,
  /**
   * Seconds since the tunnel's latest handshake, measured on the router.
   * `None` when it has never completed one.
   */
  last_handshake_secs: wire(
    z.int().nullable().optional(),
    z.int().nullable(),
  ).meta({
    description:
      "Seconds since the tunnel's latest handshake, measured on the router.\n`None` when it has never completed one.",
  }),
}).meta({
  id: 'OutboundVpn',
})
export const PeerAddArgs: z.ZodObject<{
  /**
   * Profile interface name (e.g., "lan", "guest")
   */
  profile: z.ZodString
  /**
   * Peer configuration
   */
  peer: typeof VpnServerPeer
}> = /* @__PURE__ */ object({
  /**
   * Profile interface name (e.g., "lan", "guest")
   */
  profile: z.string().meta({
    description: 'Profile interface name (e.g., "lan", "guest")',
  }),
  /**
   * Peer configuration
   */
  peer: VpnServerPeer.meta({
    description: 'Peer configuration',
  }),
}).meta({
  id: 'PeerAddArgs',
})
export const ProfileSetRequest: z.ZodIntersection<
  z.ZodObject<{ force: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean> }>,
  z.ZodIntersection<
    z.ZodObject<{
      gateway_ip: z.ZodString
      outbound: z.ZodString
      lan_access: z.ZodUnion<
        readonly [
          z.ZodLiteral<'ALL'>,
          z.ZodLiteral<'SAME_PROFILE'>,
          z.ZodObject<{ other_profiles: z.ZodArray<typeof ProfileIdOpt> }>,
        ]
      >
      wan_access: typeof WanAccess
      dns_override: z.ZodCodec<
        z.ZodOptional<z.ZodArray<typeof DnsServer>>,
        z.ZodArray<typeof DnsServer>
      >
      dns_source: z.ZodCodec<z.ZodOptional<z.ZodString>, z.ZodString>
      access_to_new_profiles: z.ZodBoolean
      owns_lan: z.ZodBoolean
    }>,
    typeof ProfileIdOpt
  >
> = /* @__PURE__ */ z
  .intersection(
    object({ force: wire(z.boolean().optional(), z.boolean()) }),
    z.intersection(
      object({
        gateway_ip: z.string(),
        outbound: z.string(),
        lan_access: z.union([
          z.literal('ALL'),
          z.literal('SAME_PROFILE'),
          object({ other_profiles: z.array(ProfileIdOpt) }),
        ]),
        wan_access: WanAccess,
        dns_override: wire(z.array(DnsServer).optional(), z.array(DnsServer)),
        dns_source: wire(z.string().optional(), z.string()),
        access_to_new_profiles: z.boolean(),
        owns_lan: z.boolean(),
      }),
      ProfileIdOpt,
    ),
  )
  .meta({
    id: 'ProfileSetRequest',
  })
export const PublishedPort: z.ZodObject<{
  id: z.ZodString
  enabled: z.ZodBoolean
  label: z.ZodString
  device_mac: z.ZodString
  ports: z.ZodString
  protocol: typeof Protocol
  ipv4: z.ZodBoolean
  ipv6: z.ZodBoolean
  ipv4_public_port: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  source: z.ZodString
  /**
   * Whether WAN collision confirmation remains active.
   */
  override_wan_ports: z.ZodBoolean
  status: typeof PublishedPortStatus
  status_reason: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  device_name: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  device_ipv4: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  device_ipv6: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  id: z.string(),
  enabled: z.boolean(),
  label: z.string(),
  device_mac: z.string(),
  ports: z.string(),
  protocol: Protocol,
  ipv4: z.boolean(),
  ipv6: z.boolean(),
  ipv4_public_port: wire(
    z.string().nullable().optional(),
    z.string().nullable(),
  ),
  source: z.string(),
  /**
   * Whether WAN collision confirmation remains active.
   */
  override_wan_ports: z.boolean().meta({
    description: 'Whether WAN collision confirmation remains active.',
  }),
  status: PublishedPortStatus,
  status_reason: wire(z.string().nullable().optional(), z.string().nullable()),
  device_name: wire(z.string().nullable().optional(), z.string().nullable()),
  device_ipv4: wire(z.string().nullable().optional(), z.string().nullable()),
  device_ipv6: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'PublishedPort',
})
export const PublishedPortInput: z.ZodObject<{
  id: z.ZodString
  enabled: z.ZodBoolean
  label: z.ZodString
  device_mac: z.ZodString
  ports: z.ZodString
  protocol: typeof Protocol
  ipv4: z.ZodBoolean
  ipv6: z.ZodBoolean
  ipv4_public_port: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  source: z.ZodString
  /**
   * Confirms an enabled IPv4 WAN collision.
   */
  override_wan_ports: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  id: z.string(),
  enabled: z.boolean(),
  label: z.string(),
  device_mac: z.string(),
  ports: z.string(),
  protocol: Protocol,
  ipv4: z.boolean(),
  ipv6: z.boolean(),
  ipv4_public_port: wire(
    z.string().nullable().optional(),
    z.string().nullable(),
  ),
  source: z.string(),
  /**
   * Confirms an enabled IPv4 WAN collision.
   */
  override_wan_ports: wire(z.boolean().optional(), z.boolean()).meta({
    description: 'Confirms an enabled IPv4 WAN collision.',
  }),
}).meta({
  id: 'PublishedPortInput',
})
export const ScheduleWindows: z.ZodObject<{
  interface: z.ZodString
  windows: z.ZodArray<typeof ScheduleWindow>
}> = /* @__PURE__ */ object({
  interface: z.string(),
  windows: z.array(ScheduleWindow),
}).meta({
  id: 'ScheduleWindows',
})
export const SetArgs: z.ZodObject<{
  /**
   * Profile interface name (e.g., "lan", "guest")
   */
  profile: z.ZodString
  /**
   * VPN server configuration
   */
  config: typeof VpnServerConfig
}> = /* @__PURE__ */ object({
  /**
   * Profile interface name (e.g., "lan", "guest")
   */
  profile: z.string().meta({
    description: 'Profile interface name (e.g., "lan", "guest")',
  }),
  /**
   * VPN server configuration
   */
  config: VpnServerConfig.meta({
    description: 'VPN server configuration',
  }),
}).meta({
  id: 'SetArgs',
})
export const SetupStatusRes: z.ZodObject<{
  setupMode: z.ZodBoolean
  disk: typeof DiskState
}> = /* @__PURE__ */ object({ setupMode: z.boolean(), disk: DiskState }).meta({
  id: 'SetupStatusRes',
})
export const UciFile: z.ZodObject<{
  sections: z.ZodArray<typeof Section>
  modified: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  sections: z.array(Section),
  modified: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'UciFile',
})

/**
 * VPN server configuration returned by list (excludes sensitive data)
 */
export const VpnServer: z.ZodObject<{
  /**
   * Profile interface name this VPN server is associated with (e.g., "lan", "guest")
   */
  profile: z.ZodString
  /**
   * Human-readable label for the VPN server
   */
  label: z.ZodString
  /**
   * Whether the VPN server is enabled (listening)
   */
  enabled: z.ZodBoolean
  /**
   * Listen port for WireGuard connections
   */
  listen_port: z.ZodInt
  /**
   * Public endpoint address for inbound client connections (hostname or IP)
   */
  endpoint: z.ZodString
  /**
   * Server's public key
   */
  public_key: z.ZodString
  /**
   * Server's VPN address (derived from profile's gateway)
   */
  server_address: z.ZodString
  /**
   * Connected peers
   */
  peers: z.ZodArray<typeof VpnServerPeer>
}> = /* @__PURE__ */ object({
  /**
   * Profile interface name this VPN server is associated with (e.g., "lan", "guest")
   */
  profile: z.string().meta({
    description:
      'Profile interface name this VPN server is associated with (e.g., "lan", "guest")',
  }),
  /**
   * Human-readable label for the VPN server
   */
  label: z.string().meta({
    description: 'Human-readable label for the VPN server',
  }),
  /**
   * Whether the VPN server is enabled (listening)
   */
  enabled: z.boolean().meta({
    description: 'Whether the VPN server is enabled (listening)',
  }),
  /**
   * Listen port for WireGuard connections
   */
  listen_port: z.int().meta({
    description: 'Listen port for WireGuard connections',
  }),
  /**
   * Public endpoint address for inbound client connections (hostname or IP)
   */
  endpoint: z.string().meta({
    description:
      'Public endpoint address for inbound client connections (hostname or IP)',
  }),
  /**
   * Server's public key
   */
  public_key: z.string().meta({
    description: "Server's public key",
  }),
  /**
   * Server's VPN address (derived from profile's gateway)
   */
  server_address: z.string().meta({
    description: "Server's VPN address (derived from profile's gateway)",
  }),
  /**
   * Connected peers
   */
  peers: z.array(VpnServerPeer).meta({
    description: 'Connected peers',
  }),
}).meta({
  id: 'VpnServer',
  description:
    'VPN server configuration returned by list (excludes sensitive data)',
})
export const WanDdnsResponse: z.ZodObject<{
  enabled: z.ZodBoolean
  provider: typeof DdnsProvider
  hostname: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  username: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  password: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  token: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  zone: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  enabled: z.boolean(),
  provider: DdnsProvider,
  hostname: wire(z.string().nullable().optional(), z.string().nullable()),
  username: wire(z.string().nullable().optional(), z.string().nullable()),
  password: wire(z.string().nullable().optional(), z.string().nullable()),
  token: wire(z.string().nullable().optional(), z.string().nullable()),
  zone: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'WanDdnsResponse',
})
export const WanDdnsSetRequest: z.ZodObject<{
  enabled: z.ZodBoolean
  provider: typeof DdnsProvider
  hostname: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  username: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  password: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  token: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  zone: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  enabled: z.boolean(),
  provider: DdnsProvider,
  hostname: wire(z.string().nullable().optional(), z.string().nullable()),
  username: wire(z.string().nullable().optional(), z.string().nullable()),
  password: wire(z.string().nullable().optional(), z.string().nullable()),
  token: wire(z.string().nullable().optional(), z.string().nullable()),
  zone: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'WanDdnsSetRequest',
})
export const WanDnsResponse: z.ZodObject<{
  mode: typeof DnsMode
  servers: z.ZodArray<typeof DnsServer>
}> = /* @__PURE__ */ object({
  mode: DnsMode,
  servers: z.array(DnsServer),
}).meta({
  id: 'WanDnsResponse',
})
export const WanDnsSetRequest: z.ZodObject<{
  mode: typeof DnsMode
  servers: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<typeof DnsServer>>>,
    z.ZodNullable<z.ZodArray<typeof DnsServer>>
  >
}> = /* @__PURE__ */ object({
  mode: DnsMode,
  servers: wire(
    z.array(DnsServer).nullable().optional(),
    z.array(DnsServer).nullable(),
  ),
}).meta({
  id: 'WanDnsSetRequest',
})
export const WanIpv4Response: z.ZodObject<{
  mode: typeof WanIpv4Mode
  assigned_ip: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Whether the WAN port has link. A failed read reports link.
   */
  carrier: z.ZodBoolean
  address: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  netmask: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  gateway: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  username: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  password: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  device: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  mode: WanIpv4Mode,
  assigned_ip: wire(z.string().nullable().optional(), z.string().nullable()),
  /**
   * Whether the WAN port has link. A failed read reports link.
   */
  carrier: z.boolean().meta({
    description: 'Whether the WAN port has link. A failed read reports link.',
  }),
  address: wire(z.string().nullable().optional(), z.string().nullable()),
  netmask: wire(z.string().nullable().optional(), z.string().nullable()),
  gateway: wire(z.string().nullable().optional(), z.string().nullable()),
  username: wire(z.string().nullable().optional(), z.string().nullable()),
  password: wire(z.string().nullable().optional(), z.string().nullable()),
  device: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'WanIpv4Response',
})
export const WanIpv4SetRequest: z.ZodObject<{
  mode: typeof WanIpv4Mode
  address: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  netmask: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  gateway: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  username: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  password: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  device: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  mode: WanIpv4Mode,
  address: wire(z.string().nullable().optional(), z.string().nullable()),
  netmask: wire(z.string().nullable().optional(), z.string().nullable()),
  gateway: wire(z.string().nullable().optional(), z.string().nullable()),
  username: wire(z.string().nullable().optional(), z.string().nullable()),
  password: wire(z.string().nullable().optional(), z.string().nullable()),
  device: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'WanIpv4SetRequest',
})
export const WanIpv6Response: z.ZodObject<{
  mode: typeof WanIpv6Mode
  address: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  prefix: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  gateway: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  ip6prefix: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  ip6prefixlen: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  ip4prefixlen: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  border_relay: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  assigned_ipv6: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Static mode: LAN prefix pool for sub-delegation, e.g. "2001:db8:abcd::/48"
   */
  lan_prefix: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  mode: WanIpv6Mode,
  address: wire(z.string().nullable().optional(), z.string().nullable()),
  prefix: wire(z.string().nullable().optional(), z.string().nullable()),
  gateway: wire(z.string().nullable().optional(), z.string().nullable()),
  ip6prefix: wire(z.string().nullable().optional(), z.string().nullable()),
  ip6prefixlen: wire(z.string().nullable().optional(), z.string().nullable()),
  ip4prefixlen: wire(z.string().nullable().optional(), z.string().nullable()),
  border_relay: wire(z.string().nullable().optional(), z.string().nullable()),
  assigned_ipv6: wire(z.string().nullable().optional(), z.string().nullable()),
  /**
   * Static mode: LAN prefix pool for sub-delegation, e.g. "2001:db8:abcd::/48"
   */
  lan_prefix: wire(
    z.string().nullable().optional(),
    z.string().nullable(),
  ).meta({
    description:
      'Static mode: LAN prefix pool for sub-delegation, e.g. "2001:db8:abcd::/48"',
  }),
}).meta({
  id: 'WanIpv6Response',
})
export const WanIpv6SetRequest: z.ZodObject<{
  mode: typeof WanIpv6Mode
  address: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  prefix: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  gateway: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  ip6prefix: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  ip6prefixlen: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  ip4prefixlen: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  border_relay: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Static mode: LAN prefix pool for sub-delegation, e.g. "2001:db8:abcd::/48"
   */
  lan_prefix: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  mode: WanIpv6Mode,
  address: wire(z.string().nullable().optional(), z.string().nullable()),
  prefix: wire(z.string().nullable().optional(), z.string().nullable()),
  gateway: wire(z.string().nullable().optional(), z.string().nullable()),
  ip6prefix: wire(z.string().nullable().optional(), z.string().nullable()),
  ip6prefixlen: wire(z.string().nullable().optional(), z.string().nullable()),
  ip4prefixlen: wire(z.string().nullable().optional(), z.string().nullable()),
  border_relay: wire(z.string().nullable().optional(), z.string().nullable()),
  /**
   * Static mode: LAN prefix pool for sub-delegation, e.g. "2001:db8:abcd::/48"
   */
  lan_prefix: wire(
    z.string().nullable().optional(),
    z.string().nullable(),
  ).meta({
    description:
      'Static mode: LAN prefix pool for sub-delegation, e.g. "2001:db8:abcd::/48"',
  }),
}).meta({
  id: 'WanIpv6SetRequest',
})
export const WanMacResponse: z.ZodObject<{
  strategy: typeof MacStrategy
  mac: z.ZodString
  default_mac: z.ZodString
}> = /* @__PURE__ */ object({
  strategy: MacStrategy,
  mac: z.string(),
  default_mac: z.string(),
}).meta({
  id: 'WanMacResponse',
})
export const WanMacSetRequest: z.ZodObject<{
  strategy: typeof MacStrategy
  mac: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  strategy: MacStrategy,
  mac: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'WanMacSetRequest',
})

/**
 * An enabled IPv4 forward overlapping a router- or SNI-owned WAN port.
 */
export const WanPortCollision: z.ZodObject<{
  id: z.ZodString
  label: z.ZodString
  router_service_ports: z.ZodArray<z.ZodString>
  hostname_route_ports: z.ZodArray<typeof SniPortUse>
}> = /* @__PURE__ */ object({
  id: z.string(),
  label: z.string(),
  router_service_ports: z.array(z.string()),
  hostname_route_ports: z.array(SniPortUse),
}).meta({
  id: 'WanPortCollision',
  description:
    'An enabled IPv4 forward overlapping a router- or SNI-owned WAN port.',
})

/**
 * `wifi.set` request: the desired WiFi config plus a confirmation flag.
 * When a profile loses its last WiFi password its devices can no longer reach
 * that subnet — disconnected if the password was removed, or moved to another
 * profile if it was reassigned — which breaks any published ports forwarding to
 * them. When the flag is false and such ports exist, `set` applies nothing and
 * returns them in `WifiSetResult` for a confirmation dialog; with the flag true
 * it deletes those published ports as part of the change.
 */
export const WifiSetRequest: z.ZodIntersection<
  z.ZodObject<{
    confirmPublishedPortDeletion: z.ZodCodec<
      z.ZodOptional<z.ZodBoolean>,
      z.ZodBoolean
    >
  }>,
  z.ZodObject<{
    ssid: z.ZodString
    broadcastSeparately: z.ZodBoolean
    /**
     * Unset runs the world regulatory domain.
     */
    country: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<z.ZodString>>,
      z.ZodNullable<z.ZodString>
    >
    radios: z.ZodRecord<z.ZodString, typeof WifiRadio>
    passwords: z.ZodArray<
      z.ZodObject<{
        label: z.ZodString
        profile: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<typeof ProfileIdOpt>>,
          z.ZodNullable<typeof ProfileIdOpt>
        >
        password: z.ZodString
      }>
    >
  }>
> = /* @__PURE__ */ z
  .intersection(
    object({
      confirmPublishedPortDeletion: wire(z.boolean().optional(), z.boolean()),
    }),
    object({
      ssid: z.string(),
      broadcastSeparately: z.boolean(),
      /**
       * Unset runs the world regulatory domain.
       */
      country: wire(
        z.string().nullable().optional(),
        z.string().nullable(),
      ).meta({
        description: 'Unset runs the world regulatory domain.',
      }),
      radios: z.record(z.string(), WifiRadio),
      passwords: z.array(
        object({
          label: z.string(),
          profile: wire(
            ProfileIdOpt.nullable().optional(),
            ProfileIdOpt.nullable(),
          ),
          password: z.string(),
        }),
      ),
    }),
  )
  .meta({
    id: 'WifiSetRequest',
    description:
      '`wifi.set` request: the desired WiFi config plus a confirmation flag.\nWhen a profile loses its last WiFi password its devices can no longer reach\nthat subnet — disconnected if the password was removed, or moved to another\nprofile if it was reassigned — which breaks any published ports forwarding to\nthem. When the flag is false and such ports exist, `set` applies nothing and\nreturns them in `WifiSetResult` for a confirmation dialog; with the flag true\nit deletes those published ports as part of the change.',
  })
export const WifiSetResult: z.ZodObject<{
  /**
   * Non-empty (and nothing applied) when published ports would be deleted and
   * the caller hasn't confirmed yet. Empty once the change is applied.
   */
  pendingPublishedPortDeletions: z.ZodArray<typeof AffectedPublishedPort>
}> = /* @__PURE__ */ object({
  /**
   * Non-empty (and nothing applied) when published ports would be deleted and
   * the caller hasn't confirmed yet. Empty once the change is applied.
   */
  pendingPublishedPortDeletions: z.array(AffectedPublishedPort).meta({
    description:
      "Non-empty (and nothing applied) when published ports would be deleted and\nthe caller hasn't confirmed yet. Empty once the change is applied.",
  }),
}).meta({
  id: 'WifiSetResult',
})

/**
 * `wifi.blackout-set` request: the blackout windows plus a confirmation flag.
 * A window covering the router's current time turns Wi-Fi off as soon as it is
 * saved, for every device on it. When the flag is false, `blackout-set` applies
 * nothing and reports that in `BlackoutSetResult` for a confirmation dialog;
 * with the flag true it saves the windows and applies them at once.
 */
export const BlackoutSetRequest: z.ZodIntersection<
  z.ZodObject<{
    confirmImmediateBlackout: z.ZodCodec<
      z.ZodOptional<z.ZodBoolean>,
      z.ZodBoolean
    >
  }>,
  typeof BlackoutWindows
> = /* @__PURE__ */ z
  .intersection(
    object({
      confirmImmediateBlackout: wire(z.boolean().optional(), z.boolean()),
    }),
    BlackoutWindows,
  )
  .meta({
    id: 'BlackoutSetRequest',
    description:
      "`wifi.blackout-set` request: the blackout windows plus a confirmation flag.\nA window covering the router's current time turns Wi-Fi off as soon as it is\nsaved, for every device on it. When the flag is false, `blackout-set` applies\nnothing and reports that in `BlackoutSetResult` for a confirmation dialog;\nwith the flag true it saves the windows and applies them at once.",
  })
export const PublishedPortsSetRequest: z.ZodObject<{
  ports: z.ZodArray<typeof PublishedPortInput>
}> = /* @__PURE__ */ object({ ports: z.array(PublishedPortInput) }).meta({
  id: 'PublishedPortsSetRequest',
})

/**
 * [`set`] response. A non-empty collision list means nothing was applied —
 * the caller confirms and re-saves; empty means the request was applied.
 */
export const PublishedPortsSetResult: z.ZodObject<{
  pending_wan_port_collisions: z.ZodArray<typeof WanPortCollision>
}> = /* @__PURE__ */ object({
  pending_wan_port_collisions: z.array(WanPortCollision),
}).meta({
  id: 'PublishedPortsSetResult',
  description:
    '[`set`] response. A non-empty collision list means nothing was applied —\nthe caller confirms and re-saves; empty means the request was applied.',
})
export const SessionList: z.ZodObject<{
  current: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  sessions: typeof AuthKeys
}> = /* @__PURE__ */ object({
  current: wire(z.string().nullable().optional(), z.string().nullable()),
  sessions: AuthKeys,
}).meta({
  id: 'SessionList',
})

/**
 * Response containing all VPN servers
 */
export const VpnServers: z.ZodObject<{
  servers: z.ZodArray<typeof VpnServer>
}> = /* @__PURE__ */ object({ servers: z.array(VpnServer) }).meta({
  id: 'VpnServers',
  description: 'Response containing all VPN servers',
})
export const WifiRegulatory: z.ZodObject<{
  /**
   * ISO 3166-1 alpha-2 codes the regulatory database defines.
   */
  countries: z.ZodArray<z.ZodString>
  /**
   * What an access point may use under the current country, by band.
   */
  bands: z.ZodRecord<z.ZodString, typeof BandChannels>
}> = /* @__PURE__ */ object({
  /**
   * ISO 3166-1 alpha-2 codes the regulatory database defines.
   */
  countries: z.array(z.string()).meta({
    description: 'ISO 3166-1 alpha-2 codes the regulatory database defines.',
  }),
  /**
   * What an access point may use under the current country, by band.
   */
  bands: z.record(z.string(), BandChannels).meta({
    description:
      'What an access point may use under the current country, by band.',
  }),
}).meta({
  id: 'WifiRegulatory',
})
export const Api = {
  _PARAMS: /* @__PURE__ */ empty,
  _CHILDREN: {
    activity: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        clear: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        delete: {
          _PARAMS: /* @__PURE__ */ ActivityDeleteParams,
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
            ActivityListParams,
          ),
          _RETURN: /* @__PURE__ */ ActivityListResponse,
        },
      },
    },
    auth: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        'check-initialized': {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ CheckInitializedRes,
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
        'set-initial-password': {
          _PARAMS: /* @__PURE__ */ SetInitialPasswordParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-password': {
          _PARAMS: /* @__PURE__ */ ResetPasswordParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'verify-password': {
          _PARAMS: /* @__PURE__ */ VerifyPasswordParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
    backup: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        create: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ BackupCreateRes,
        },
        restore: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ BackupRestoreRes,
        },
      },
    },
    devices: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        'data-usage': {
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
            DataUsageReq.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ z.array(DataUsagePoint),
        },
        forget: {
          _PARAMS: /* @__PURE__ */ DeviceMacReq.meta({
            description:
              'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
          }),
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
          _RETURN: /* @__PURE__ */ z.array(Device),
        },
        'set-auto-forward': {
          _PARAMS: /* @__PURE__ */ SetAutoForwardRequest.meta({
            description:
              'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
          }),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-dns-injection': {
          _PARAMS: /* @__PURE__ */ SetDnsInjectionReq.meta({
            description:
              'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
          }),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-router-advertisements': {
          _PARAMS: /* @__PURE__ */ SetRouterAdvertisementsReq.meta({
            description:
              'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
          }),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        update: {
          _PARAMS: /* @__PURE__ */ DeviceUpdateReq.meta({
            description:
              'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
          }),
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
    diagnostics: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        create: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ DiagnosticsCreateRes,
        },
      },
    },
    dir: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        get: {
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
            DirGetArgs,
          ),
          _RETURN: /* @__PURE__ */ z.array(DirEntry),
        },
      },
    },
    dns: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        'injected-list': {
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
          _RETURN: /* @__PURE__ */ z.array(InjectedDnsRecord),
        },
      },
    },
    ethernet: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        edit: {
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
          _RETURN: /* @__PURE__ */ z.null(),
        },
        get: {
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
          _RETURN: /* @__PURE__ */ object({
            wan_ipv6: z.boolean(),
            wan_port: wire(
              z.string().nullable().optional(),
              z.string().nullable(),
            ),
            ports: z.record(
              z.string(),
              object({
                profile: wire(
                  ProfileId.nullable().optional(),
                  ProfileId.nullable(),
                ),
              }),
            ),
          }),
        },
        set: {
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
            EthernetSetRequest.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ EthernetSetResult,
        },
      },
    },
    exec: {
      _PARAMS: /* @__PURE__ */ z.intersection(
        z.intersection(
          object({
            format: wire(IoFormat.nullable().optional(), IoFormat.nullable()),
          }),
          empty,
        ),
        ExecReq,
      ),
      _RETURN: /* @__PURE__ */ ExecRes,
    },
    file: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        get: {
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
            GetFileArgs,
          ),
          _RETURN: /* @__PURE__ */ FileContents,
        },
        set: {
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
            SetFileArgs,
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
    lan: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        'ipv4-get': {
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
          _RETURN: /* @__PURE__ */ LanIpv4Response,
        },
        'ipv4-set': {
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
            LanIpv4SetRequest.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'ipv6-get': {
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
          _RETURN: /* @__PURE__ */ LanIpv6Response,
        },
        'ipv6-set': {
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
            LanIpv6SetRequest.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
    profiles: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        create: {
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
            z
              .intersection(
                object({
                  gateway_ip: z.string(),
                  outbound: z.string(),
                  lan_access: z.union([
                    z.literal('ALL'),
                    z.literal('SAME_PROFILE'),
                    object({ other_profiles: z.array(ProfileIdOpt) }),
                  ]),
                  wan_access: WanAccess,
                  dns_override: wire(
                    z.array(DnsServer).optional(),
                    z.array(DnsServer),
                  ),
                  dns_source: wire(z.string().optional(), z.string()),
                  access_to_new_profiles: z.boolean(),
                  owns_lan: z.boolean(),
                }),
                ProfileIdOpt,
              )
              .meta({
                description:
                  'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
              }),
          ),
          _RETURN: /* @__PURE__ */ ProfileId,
        },
        delete: {
          _PARAMS: /* @__PURE__ */ ProfileIdOpt,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        edit: {
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
            EditArgs,
          ),
          _RETURN: /* @__PURE__ */ ProfileId,
        },
        get: {
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
            ProfileIdOpt,
          ),
          _RETURN: /* @__PURE__ */ z.intersection(
            object({
              gateway_ip: z.string(),
              outbound: z.string(),
              lan_access: z.union([
                z.literal('ALL'),
                z.literal('SAME_PROFILE'),
                object({ other_profiles: z.array(ProfileId) }),
              ]),
              wan_access: WanAccess,
              dns_override: wire(
                z.array(DnsServer).optional(),
                z.array(DnsServer),
              ),
              dns_source: wire(z.string().optional(), z.string()),
              access_to_new_profiles: z.boolean(),
              owns_lan: z.boolean(),
            }),
            ProfileId,
          ),
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
          _RETURN: /* @__PURE__ */ z.array(ProfileId),
        },
        'schedule-get': {
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
            ScheduleGetParams.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ z.array(ScheduleWindow),
        },
        'schedule-set': {
          _PARAMS: /* @__PURE__ */ ScheduleWindows.meta({
            description:
              'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
          }),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        set: {
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
            ProfileSetRequest.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ ProfileId,
        },
      },
    },
    'published-ports': {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        'auto-list': {
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
          _RETURN: /* @__PURE__ */ z.array(AutomaticPortUse),
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
          _RETURN: /* @__PURE__ */ z.array(PublishedPort),
        },
        reconcile: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.unknown(),
        },
        set: {
          _PARAMS: /* @__PURE__ */ PublishedPortsSetRequest.meta({
            description:
              'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
          }),
          _RETURN: /* @__PURE__ */ PublishedPortsSetResult,
        },
        'sync-hairpin': {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.unknown(),
        },
        'wan-changed': {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.unknown(),
        },
      },
    },
    setup: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        status: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ SetupStatusRes,
        },
      },
    },
    'ssh-keys': {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        add: {
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
            SshKeyAddParams,
          ),
          _RETURN: /* @__PURE__ */ SshKeyResponse,
        },
        delete: {
          _PARAMS: /* @__PURE__ */ SshKeyDeleteParams,
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
          _RETURN: /* @__PURE__ */ z.array(SshKeyResponse),
        },
      },
    },
    system: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        'apply-remote-access': {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.unknown(),
        },
        'factory-reset': {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'get-timezones': {
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
        info: {
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
          _RETURN: /* @__PURE__ */ SystemInfoResponse,
        },
        logs: {
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
          _RETURN: /* @__PURE__ */ LogsResponse,
        },
        'logs-follow': {
          _PARAMS: /* @__PURE__ */ FollowLogsParams,
          _RETURN: /* @__PURE__ */ FollowLogsRes,
        },
        'newer-versions': {
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
          _RETURN: /* @__PURE__ */ z.array(VersionInfo),
        },
        restart: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-preferences': {
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
            SetPreferencesReq.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ z.unknown(),
        },
        'set-timezone': {
          _PARAMS: /* @__PURE__ */ SetTimezoneParams.meta({
            description:
              'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
          }),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        update: {
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
            UpdateSystemParams,
          ),
          _RETURN: /* @__PURE__ */ UpdateSystemRes,
        },
      },
    },
    uci: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        edit: {
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
            GetArgs,
          ),
          _RETURN: /* @__PURE__ */ z.record(z.string(), z.string()),
        },
        get: {
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
            GetArgs,
          ),
          _RETURN: /* @__PURE__ */ z.record(z.string(), UciFile),
        },
        set: {
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
            z.record(z.string(), UciFile).meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ z.record(z.string(), z.string()),
        },
      },
    },
    'vpn-client': {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        create: {
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
            OutboundVpnCreateRequest.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ OutboundVpnCreateResponse,
        },
        delete: {
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
            OutboundVpnDeleteRequest,
          ),
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
          _RETURN: /* @__PURE__ */ z.array(OutboundVpn),
        },
        'set-enabled': {
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
            OutboundVpnSetEnabledRequest.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        update: {
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
            OutboundVpnUpdateRequest.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
    'vpn-server': {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        delete: {
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
            DeleteArgs,
          ),
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
          _RETURN: /* @__PURE__ */ VpnServers,
        },
        'peer-add': {
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
            PeerAddArgs.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ PeerAddResponse,
        },
        'peer-delete': {
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
            PeerDeleteArgs,
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'peer-rename': {
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
            PeerRenameArgs,
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'peer-set-dns-injection': {
          _PARAMS: /* @__PURE__ */ PeerSetDnsInjectionArgs,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        set: {
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
            SetArgs.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
    wan: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        'ddns-get': {
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
          _RETURN: /* @__PURE__ */ WanDdnsResponse,
        },
        'ddns-set': {
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
            WanDdnsSetRequest.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'dns-get': {
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
          _RETURN: /* @__PURE__ */ WanDnsResponse,
        },
        'dns-set': {
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
            WanDnsSetRequest.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'ipv4-get': {
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
          _RETURN: /* @__PURE__ */ WanIpv4Response,
        },
        'ipv4-set': {
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
            WanIpv4SetRequest.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'ipv6-get': {
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
          _RETURN: /* @__PURE__ */ WanIpv6Response,
        },
        'ipv6-set': {
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
            WanIpv6SetRequest.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'mac-get': {
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
          _RETURN: /* @__PURE__ */ WanMacResponse,
        },
        'mac-set': {
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
            WanMacSetRequest.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
    wifi: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        'blackout-apply': {
          _PARAMS: /* @__PURE__ */ z.unknown(),
          _RETURN: /* @__PURE__ */ z.unknown(),
          _OPAQUE: true as const,
        },
        'blackout-get': {
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
          _RETURN: /* @__PURE__ */ z.array(BlackoutWindow),
        },
        'blackout-set': {
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
            BlackoutSetRequest.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ BlackoutSetResult,
        },
        edit: {
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
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'generate-password': {
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
          _RETURN: /* @__PURE__ */ z.string(),
        },
        get: {
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
          _RETURN: /* @__PURE__ */ object({
            ssid: z.string(),
            broadcastSeparately: z.boolean(),
            /**
             * Unset runs the world regulatory domain.
             */
            country: wire(
              z.string().nullable().optional(),
              z.string().nullable(),
            ).meta({
              description: 'Unset runs the world regulatory domain.',
            }),
            radios: z.record(z.string(), WifiRadio),
            passwords: z.array(
              object({
                label: z.string(),
                profile: wire(
                  ProfileId.nullable().optional(),
                  ProfileId.nullable(),
                ),
                password: z.string(),
              }),
            ),
          }),
        },
        regulatory: {
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
          _RETURN: /* @__PURE__ */ WifiRegulatory,
        },
        set: {
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
            WifiSetRequest.meta({
              description:
                'A clap-compatible wrapper that reads the inner value as JSON from stdin\ninstead of from CLI args. Used for commands that take complex structured\ninput (entire profile objects, etc.).\n\nThis is a thin wrapper around `startos::util::serde::StdinDeserializable`\nthat adds a `CommandFactory` impl so it can be used directly as a handler\n`Params` type (rather than flattened inside a `#[derive(Parser)]` struct,\nwhich is how startos uses its version).',
            }),
          ),
          _RETURN: /* @__PURE__ */ WifiSetResult,
        },
      },
    },
  },
}
