import { z } from 'zod'
const wire = <A extends z.ZodType, B extends z.ZodType>(input: A, output: B) =>
  z.codec(input, output, { decode: v => v as never, encode: v => v as never })
const object = <Shape extends z.core.$ZodLooseShape>(shape: Shape) =>
  z.object(shape).meta({ additionalProperties: true })
const empty: z.ZodType<{}, {}> = /* @__PURE__ */ object({})
const literal_0 = /* @__PURE__ */ z.string()
const literal_1 = /* @__PURE__ */ z.record(z.string(), z.unknown()).nullable()
const literal_2 = /* @__PURE__ */ z.record(z.string(), z.unknown())
const literal_3 = /* @__PURE__ */ z.union([z.string(), z.array(z.number())])
const literal_4 = /* @__PURE__ */ z.union([
  z.string(),
  z.record(z.string(), z.string()),
])
const literal_5 = /* @__PURE__ */ z.enum(['processor', 'display'])
const literal_6 = /* @__PURE__ */ z.object({
  value: z.string(),
  unit: z.string(),
})
export const AcmeProvider: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'AcmeProvider',
})
export const ActionId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'ActionId',
})
export const ActionResultV0: z.ZodObject<{
  message: z.ZodString
  value: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  copyable: z.ZodBoolean
  qr: z.ZodBoolean
}> = /* @__PURE__ */ object({
  message: z.string(),
  value: wire(z.string().nullable().optional(), z.string().nullable()),
  copyable: z.boolean(),
  qr: z.boolean(),
}).meta({
  id: 'ActionResultV0',
})

/**
 * JSON Patch 'add' operation representation
 */
export const AddOperation: z.ZodObject<{
  /**
   * JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
   * within the target document where the operation is performed.
   */
  path: z.ZodString
  /**
   * Value to add to the target location.
   */
  value: z.ZodUnknown
}> = /* @__PURE__ */ object({
  /**
   * JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
   * within the target document where the operation is performed.
   */
  path: z.string().meta({
    description:
      'JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location\nwithin the target document where the operation is performed.',
  }),
  /**
   * Value to add to the target location.
   */
  value: z.unknown().meta({
    description: 'Value to add to the target location.',
  }),
}).meta({
  id: 'AddOperation',
  description: "JSON Patch 'add' operation representation",
})

/**
 * Keys an import added, and keys it found already present.
 */
export const AddTrustedKeyRes: z.ZodObject<{
  /**
   * Trusted for packages.
   */
  added: z.ZodArray<z.ZodString>
  /**
   * GPG identities merged; explicit `trustFor` grants the requested permission.
   */
  existing: z.ZodArray<z.ZodString>
}> = /* @__PURE__ */ object({
  /**
   * Trusted for packages.
   */
  added: z.array(z.string()).meta({
    description: 'Trusted for packages.',
  }),
  /**
   * GPG identities merged; explicit `trustFor` grants the requested permission.
   */
  existing: z.array(z.string()).meta({
    description:
      'GPG identities merged; explicit `trustFor` grants the requested permission.',
  }),
}).meta({
  id: 'AddTrustedKeyRes',
  description: 'Keys an import added, and keys it found already present.',
})
export const AnySignature: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'AnySignature',
})
export const AnyVerifyingKey: typeof literal_0 = /* @__PURE__ */ literal_0.meta(
  {
    id: 'AnyVerifyingKey',
  },
)
export const ApiState: z.ZodEnum<{
  error: 'error'
  initializing: 'initializing'
  running: 'running'
}> = /* @__PURE__ */ z.enum(['error', 'initializing', 'running']).meta({
  id: 'ApiState',
})
export const ApplyParams: z.ZodObject<{ expr: z.ZodString }> =
  /* @__PURE__ */ object({ expr: z.string() }).meta({
    id: 'ApplyParams',
  })

/**
 * A registered-target identity; migrated CIFS targets retain their existing IDs.
 */
export const BackupTargetId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'BackupTargetId',
  description:
    'A registered-target identity; migrated CIFS targets retain their existing IDs.',
})
export const BasicCredential: z.ZodObject<{
  username: z.ZodString
  password: z.ZodString
}> = /* @__PURE__ */ object({
  username: z.string(),
  password: z.string(),
}).meta({
  id: 'BasicCredential',
})
export const Blake3Commitment: z.ZodIntersection<
  z.ZodObject<{
    type: z.ZodCodec<
      z.ZodOptional<z.ZodLiteral<'blake3'>>,
      z.ZodLiteral<'blake3'>
    >
  }>,
  z.ZodObject<{ hash: typeof literal_0; size: z.ZodInt }>
> = /* @__PURE__ */ z
  .intersection(
    object({ type: wire(z.literal('blake3').optional(), z.literal('blake3')) }),
    object({ hash: literal_0, size: z.int() }),
  )
  .meta({
    id: 'Blake3Commitment',
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
export const Celsius: typeof literal_6 = /* @__PURE__ */ literal_6.meta({
  id: 'Celsius',
})
export const CheckPortRes: z.ZodObject<{
  ip: z.ZodString
  port: z.ZodInt
  openExternally: z.ZodBoolean
  openInternally: z.ZodBoolean
  hairpinning: z.ZodBoolean
}> = /* @__PURE__ */ object({
  ip: z.string(),
  port: z.int(),
  openExternally: z.boolean(),
  openInternally: z.boolean(),
  hairpinning: z.boolean(),
}).meta({
  id: 'CheckPortRes',
})

/**
 * v6 reachability of the box's GUA at a port. v6 is NAT-free (the GUA is the
 * box's own address), so there is no hairpinning. Queried separately from
 * [`CheckPortRes`] so the IPv4 and IPv6 checks can run independently.
 */
export const CheckPortV6Res: z.ZodObject<{
  ip: z.ZodString
  openExternally: z.ZodBoolean
  openInternally: z.ZodBoolean
}> = /* @__PURE__ */ object({
  ip: z.string(),
  openExternally: z.boolean(),
  openInternally: z.boolean(),
}).meta({
  id: 'CheckPortV6Res',
  description:
    "v6 reachability of the box's GUA at a port. v6 is NAT-free (the GUA is the\nbox's own address), so there is no hairpinning. Queried separately from\n[`CheckPortRes`] so the IPv4 and IPv6 checks can run independently.",
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
export const CifsAddParams: z.ZodObject<{
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
  id: 'CifsAddParams',
})
export const ConnectRegistryParams: z.ZodObject<{
  url: z.ZodString
  name: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  url: z.string(),
  name: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'ConnectRegistryParams',
})
export const ContactInfo: z.ZodUnion<
  readonly [
    z.ZodObject<{ email: z.ZodString }>,
    z.ZodObject<{ matrix: z.ZodString }>,
    z.ZodObject<{ website: typeof literal_0 }>,
  ]
> = /* @__PURE__ */ z
  .union([
    object({ email: z.string() }),
    object({ matrix: z.string() }),
    object({ website: literal_0 }),
  ])
  .meta({
    id: 'ContactInfo',
  })
export const ContainerId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'ContainerId',
})

/**
 * JSON Patch 'copy' operation representation
 */
export const CopyOperation: z.ZodObject<{
  /**
   * JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
   * to copy value from.
   */
  from: z.ZodString
  /**
   * JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
   * within the target document where the operation is performed.
   */
  path: z.ZodString
}> = /* @__PURE__ */ object({
  /**
   * JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
   * to copy value from.
   */
  from: z.string().meta({
    description:
      'JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location\nto copy value from.',
  }),
  /**
   * JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
   * within the target document where the operation is performed.
   */
  path: z.string().meta({
    description:
      'JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location\nwithin the target document where the operation is performed.',
  }),
}).meta({
  id: 'CopyOperation',
  description: "JSON Patch 'copy' operation representation",
})
export const CountEntry: z.ZodObject<{ label: z.ZodString; count: z.ZodInt }> =
  /* @__PURE__ */ object({ label: z.string(), count: z.int() }).meta({
    id: 'CountEntry',
  })
export const CreateTokenParams: z.ZodObject<{ name: z.ZodString }> =
  /* @__PURE__ */ object({ name: z.string() }).meta({
    id: 'CreateTokenParams',
  })
export const CreatedToken: z.ZodObject<{
  /**
   * The session id the token authenticates as, for `auth.session.kill`.
   */
  id: z.ZodString
  /**
   * The bearer secret. It is not stored and cannot be retrieved again.
   */
  token: z.ZodString
}> = /* @__PURE__ */ object({
  /**
   * The session id the token authenticates as, for `auth.session.kill`.
   */
  id: z.string().meta({
    description:
      'The session id the token authenticates as, for `auth.session.kill`.',
  }),
  /**
   * The bearer secret. It is not stored and cannot be retrieved again.
   */
  token: z.string().meta({
    description:
      'The bearer secret. It is not stored and cannot be retrieved again.',
  }),
}).meta({
  id: 'CreatedToken',
})
export const DataUrl: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'DataUrl',
})
export const DeviceFilter: z.ZodObject<{
  description: z.ZodString
  class: typeof literal_5
  product: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  vendor: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  capabilities: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<z.ZodString>>>,
    z.ZodNullable<z.ZodArray<z.ZodString>>
  >
  driver: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  description: z.string(),
  class: literal_5,
  product: wire(z.string().nullable().optional(), z.string().nullable()),
  vendor: wire(z.string().nullable().optional(), z.string().nullable()),
  capabilities: wire(
    z.array(z.string()).nullable().optional(),
    z.array(z.string()).nullable(),
  ),
  driver: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'DeviceFilter',
})
export const DnsSource: z.ZodEnum<{
  gateway: 'gateway'
  static: 'static'
  'public-fallback': 'public-fallback'
}> = /* @__PURE__ */ z.enum(['gateway', 'static', 'public-fallback']).meta({
  id: 'DnsSource',
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
export const EchoParams: z.ZodObject<{ message: z.ZodString }> =
  /* @__PURE__ */ object({ message: z.string() }).meta({
    id: 'EchoParams',
  })
export const EncryptedWire: z.ZodObject<{ encrypted: z.ZodUnknown }> =
  /* @__PURE__ */ object({ encrypted: z.unknown() }).meta({
    id: 'EncryptedWire',
  })
export const EndpointParams: z.ZodObject<{ endpoint: z.ZodString }> =
  /* @__PURE__ */ object({ endpoint: z.string() }).meta({
    id: 'EndpointParams',
  })

/**
 * Selects how aggressively an EPP-capable CPU pursues performance.
 */
export const Epp: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'Epp',
  description:
    'Selects how aggressively an EPP-capable CPU pursues performance.',
})
export const ExecResult: z.ZodObject<{
  stdout: z.ZodString
  stderr: z.ZodString
  exitCode: z.ZodInt
  timedOut: z.ZodBoolean
  /**
   * Whether either output kept only its final 64 KiB.
   */
  truncated: z.ZodBoolean
}> = /* @__PURE__ */ object({
  stdout: z.string(),
  stderr: z.string(),
  exitCode: z.int(),
  timedOut: z.boolean(),
  /**
   * Whether either output kept only its final 64 KiB.
   */
  truncated: z.boolean().meta({
    description: 'Whether either output kept only its final 64 KiB.',
  }),
}).meta({
  id: 'ExecResult',
})
export const FormatDiskParams: z.ZodObject<{
  disk: z.ZodString
  label: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  disk: z.string(),
  label: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'FormatDiskParams',
})

/**
 * Unauthenticated discovery hints; opening the store validates its actual state.
 */
export const FormatStatus: z.ZodEnum<{
  legacy: 'legacy'
  current: 'current'
  interrupted: 'interrupted'
  unknown: 'unknown'
}> = /* @__PURE__ */ z
  .enum(['legacy', 'current', 'interrupted', 'unknown'])
  .meta({
    id: 'FormatStatus',
    description:
      'Unauthenticated discovery hints; opening the store validates its actual state.',
  })
export const ForwardTarget: z.ZodObject<{
  target: z.ZodString
  target_prefix: z.ZodInt
  reqs: z.ZodString
}> = /* @__PURE__ */ object({
  target: z.string(),
  target_prefix: z.int(),
  reqs: z.string(),
}).meta({
  id: 'ForwardTarget',
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
export const GenerateCertificateParams: z.ZodObject<{
  hostnames: z.ZodArray<z.ZodString>
  ed25519: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  hostnames: z.array(z.string()),
  ed25519: wire(z.boolean().optional(), z.boolean()),
}).meta({
  id: 'GenerateCertificateParams',
})
export const GenerateCertificateResponse: z.ZodObject<{
  key: z.ZodString
  fullchain: z.ZodString
}> = /* @__PURE__ */ object({ key: z.string(), fullchain: z.string() }).meta({
  id: 'GenerateCertificateResponse',
})
export const GetDownloadsParams: z.ZodObject<{
  /**
   * Filter by package ID
   */
  pkgId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Filter by version
   */
  version: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Start of time range (RFC 3339)
   */
  after: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * End of time range (RFC 3339)
   */
  before: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  /**
   * Filter by package ID
   */
  pkgId: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description: 'Filter by package ID',
  }),
  /**
   * Filter by version
   */
  version: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description: 'Filter by version',
  }),
  /**
   * Start of time range (RFC 3339)
   */
  after: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description: 'Start of time range (RFC 3339)',
  }),
  /**
   * End of time range (RFC 3339)
   */
  before: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description: 'End of time range (RFC 3339)',
  }),
}).meta({
  id: 'GetDownloadsParams',
})
export const GetOsAssetParams: z.ZodObject<{
  version: z.ZodString
  platform: z.ZodString
}> = /* @__PURE__ */ object({ version: z.string(), platform: z.string() }).meta(
  {
    id: 'GetOsAssetParams',
  },
)
export const GetOsVersionParams: z.ZodObject<{
  sourceVersion: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  targetVersion: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  serverId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  platform: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  sourceVersion: wire(z.string().nullable().optional(), z.string().nullable()),
  targetVersion: wire(z.string().nullable().optional(), z.string().nullable()),
  serverId: wire(z.string().nullable().optional(), z.string().nullable()),
  platform: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'GetOsVersionParams',
})
export const GetUsersParams: z.ZodObject<{
  /**
   * Start of time range (RFC 3339)
   */
  after: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * End of time range (RFC 3339)
   */
  before: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  /**
   * Start of time range (RFC 3339)
   */
  after: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description: 'Start of time range (RFC 3339)',
  }),
  /**
   * End of time range (RFC 3339)
   */
  before: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description: 'End of time range (RFC 3339)',
  }),
}).meta({
  id: 'GetUsersParams',
})
export const GigaBytes: typeof literal_6 = /* @__PURE__ */ literal_6.meta({
  id: 'GigaBytes',
})
export const GitHash: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'GitHash',
})
export const Governor: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'Governor',
})

/**
 * An original public-key export and its self-certified identity claims.
 */
export const GpgPublicKeyExport: z.ZodObject<{
  /**
   * Original armor, preserving all certificate packets and headers.
   */
  armored: z.ZodString
  /**
   * Primary-key fingerprints of the certificates in this export.
   */
  fingerprints: z.ZodArray<z.ZodString>
  /**
   * Self-certified user IDs containing names, comments, and email addresses.
   */
  userIds: z.ZodArray<z.ZodString>
  /**
   * Self-certified claims indexed by the signing key's canonical DER encoding.
   */
  identities: z.ZodCodec<
    z.ZodOptional<z.ZodRecord<z.ZodString, z.ZodArray<z.ZodString>>>,
    z.ZodRecord<z.ZodString, z.ZodArray<z.ZodString>>
  >
  /**
   * OpenPGP fingerprints indexed by the signing key's canonical DER encoding.
   */
  keyFingerprints: z.ZodCodec<
    z.ZodOptional<z.ZodRecord<z.ZodString, z.ZodArray<z.ZodString>>>,
    z.ZodRecord<z.ZodString, z.ZodArray<z.ZodString>>
  >
}> = /* @__PURE__ */ object({
  /**
   * Original armor, preserving all certificate packets and headers.
   */
  armored: z.string().meta({
    description:
      'Original armor, preserving all certificate packets and headers.',
  }),
  /**
   * Primary-key fingerprints of the certificates in this export.
   */
  fingerprints: z.array(z.string()).meta({
    description: 'Primary-key fingerprints of the certificates in this export.',
  }),
  /**
   * Self-certified user IDs containing names, comments, and email addresses.
   */
  userIds: z.array(z.string()).meta({
    description:
      'Self-certified user IDs containing names, comments, and email addresses.',
  }),
  /**
   * Self-certified claims indexed by the signing key's canonical DER encoding.
   */
  identities: wire(
    z.record(z.string(), z.array(z.string())).optional(),
    z.record(z.string(), z.array(z.string())),
  ).meta({
    description:
      "Self-certified claims indexed by the signing key's canonical DER encoding.",
  }),
  /**
   * OpenPGP fingerprints indexed by the signing key's canonical DER encoding.
   */
  keyFingerprints: wire(
    z.record(z.string(), z.array(z.string())).optional(),
    z.record(z.string(), z.array(z.string())),
  ).meta({
    description:
      "OpenPGP fingerprints indexed by the signing key's canonical DER encoding.",
  }),
}).meta({
  id: 'GpgPublicKeyExport',
  description:
    'An original public-key export and its self-certified identity claims.',
})
export const Guid: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'Guid',
})

/**
 * Proof that a set of auth keys was unenrolled — removed from the persisted
 * and ephemeral signer stores with any continuations they opened killed.
 * Obtained via [`SignatureAuthorizationContext::unenroll`], or [`Self::unenroll`]
 * from inside a db transaction.
 */
export const HasUnenrolledKeys: z.ZodNull = /* @__PURE__ */ z.null().meta({
  id: 'HasUnenrolledKeys',
  description:
    'Proof that a set of auth keys was unenrolled — removed from the persisted\nand ephemeral signer stores with any continuations they opened killed.\nObtained via [`SignatureAuthorizationContext::unenroll`], or [`Self::unenroll`]\nfrom inside a db transaction.',
})
export const HealthCheckId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'HealthCheckId',
})
export const HistoryParams: z.ZodObject<{ offset: z.ZodInt; limit: z.ZodInt }> =
  /* @__PURE__ */ object({ offset: z.int(), limit: z.int() }).meta({
    id: 'HistoryParams',
  })
export const HostId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'HostId',
})
export const ImageId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'ImageId',
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
export const ListNotificationParams: z.ZodObject<{
  before: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  limit: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
}> = /* @__PURE__ */ object({
  before: wire(z.int().nullable().optional(), z.int().nullable()),
  limit: wire(z.int().nullable().optional(), z.int().nullable()),
}).meta({
  id: 'ListNotificationParams',
})

/**
 * Address of the NUT server to list UPS devices from.
 */
export const ListNutUpsParams: z.ZodObject<{
  host: z.ZodString
  port: z.ZodCodec<z.ZodOptional<z.ZodInt>, z.ZodInt>
}> = /* @__PURE__ */ object({
  host: z.string(),
  port: wire(z.int().optional(), z.int()),
}).meta({
  id: 'ListNutUpsParams',
  description: 'Address of the NUT server to list UPS devices from.',
})
export const ListParams: typeof empty = /* @__PURE__ */ empty.meta({
  id: 'ListParams',
})
export const ListVersionSignersParams: z.ZodObject<{ version: z.ZodString }> =
  /* @__PURE__ */ object({ version: z.string() }).meta({
    id: 'ListVersionSignersParams',
  })
export const LocaleString: typeof literal_4 = /* @__PURE__ */ literal_4.meta({
  id: 'LocaleString',
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

/**
 * Verbosity accepted by the server's runtime logging control.
 */
export const LogLevel: z.ZodEnum<{
  error: 'error'
  warn: 'warn'
  info: 'info'
  debug: 'debug'
  trace: 'trace'
}> = /* @__PURE__ */ z.enum(['error', 'warn', 'info', 'debug', 'trace']).meta({
  id: 'LogLevel',
  description: "Verbosity accepted by the server's runtime logging control.",
})
export const LogoutParams: typeof empty = /* @__PURE__ */ empty.meta({
  id: 'LogoutParams',
})
export const LshwDisplay: z.ZodObject<{
  product: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  vendor: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  capabilities: z.ZodArray<z.ZodString>
  driver: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  product: wire(z.string().nullable().optional(), z.string().nullable()),
  vendor: wire(z.string().nullable().optional(), z.string().nullable()),
  capabilities: z.array(z.string()),
  driver: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'LshwDisplay',
})
export const LshwProcessor: z.ZodObject<{
  product: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  vendor: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  capabilities: z.ZodArray<z.ZodString>
}> = /* @__PURE__ */ object({
  product: wire(z.string().nullable().optional(), z.string().nullable()),
  vendor: wire(z.string().nullable().optional(), z.string().nullable()),
  capabilities: z.array(z.string()),
}).meta({
  id: 'LshwProcessor',
})
export const MaybeUtf8String: typeof literal_3 = /* @__PURE__ */ literal_3.meta(
  {
    id: 'MaybeUtf8String',
  },
)
export const MebiBytes: typeof literal_6 = /* @__PURE__ */ literal_6.meta({
  id: 'MebiBytes',
})
export const MerkleArchiveCommitment: z.ZodIntersection<
  z.ZodObject<{
    type: z.ZodCodec<
      z.ZodOptional<z.ZodLiteral<'merkle-archive'>>,
      z.ZodLiteral<'merkle-archive'>
    >
  }>,
  z.ZodObject<{ rootSighash: typeof literal_0; rootMaxsize: z.ZodInt }>
> = /* @__PURE__ */ z
  .intersection(
    object({
      type: wire(
        z.literal('merkle-archive').optional(),
        z.literal('merkle-archive'),
      ),
    }),
    object({ rootSighash: literal_0, rootMaxsize: z.int() }),
  )
  .meta({
    id: 'MerkleArchiveCommitment',
  })
export const MetricsFollowParams: typeof empty = /* @__PURE__ */ empty.meta({
  id: 'MetricsFollowParams',
})
export const MiB: z.ZodInt = /* @__PURE__ */ z.int().meta({
  id: 'MiB',
})
export const ModifyNotificationBeforeParams: z.ZodObject<{ before: z.ZodInt }> =
  /* @__PURE__ */ object({ before: z.int() }).meta({
    id: 'ModifyNotificationBeforeParams',
  })
export const ModifyNotificationParams: z.ZodObject<{
  ids: z.ZodArray<z.ZodInt>
}> = /* @__PURE__ */ object({ ids: z.array(z.int()) }).meta({
  id: 'ModifyNotificationParams',
})

/**
 * JSON Patch 'move' operation representation
 */
export const MoveOperation: z.ZodObject<{
  /**
   * JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
   * to move value from.
   */
  from: z.ZodString
  /**
   * JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
   * within the target document where the operation is performed.
   */
  path: z.ZodString
}> = /* @__PURE__ */ object({
  /**
   * JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
   * to move value from.
   */
  from: z.string().meta({
    description:
      'JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location\nto move value from.',
  }),
  /**
   * JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
   * within the target document where the operation is performed.
   */
  path: z.string().meta({
    description:
      'JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location\nwithin the target document where the operation is performed.',
  }),
}).meta({
  id: 'MoveOperation',
  description: "JSON Patch 'move' operation representation",
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
export const NetInfo: z.ZodObject<{
  assignedPort: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  assignedSslPort: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
}> = /* @__PURE__ */ object({
  assignedPort: wire(z.int().nullable().optional(), z.int().nullable()),
  assignedSslPort: wire(z.int().nullable().optional(), z.int().nullable()),
}).meta({
  id: 'NetInfo',
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
export const NotificationLevel: z.ZodEnum<{
  success: 'success'
  info: 'info'
  warning: 'warning'
  error: 'error'
}> = /* @__PURE__ */ z.enum(['success', 'info', 'warning', 'error']).meta({
  id: 'NotificationLevel',
})

/**
 * A UPS detected on USB, with the driver settings that address it.
 */
export const NutDevice: z.ZodObject<{
  driver: z.ZodString
  port: z.ZodString
  vendorId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  productId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  serial: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  bus: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Pins devices without a serial number to the selected USB port.
   */
  busPort: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  vendor: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  product: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  driver: z.string(),
  port: z.string(),
  vendorId: wire(z.string().nullable().optional(), z.string().nullable()),
  productId: wire(z.string().nullable().optional(), z.string().nullable()),
  serial: wire(z.string().nullable().optional(), z.string().nullable()),
  bus: wire(z.string().nullable().optional(), z.string().nullable()),
  /**
   * Pins devices without a serial number to the selected USB port.
   */
  busPort: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      'Pins devices without a serial number to the selected USB port.',
  }),
  vendor: wire(z.string().nullable().optional(), z.string().nullable()),
  product: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'NutDevice',
  description:
    'A UPS detected on USB, with the driver settings that address it.',
})

/**
 * A UPS served by a NUT server, as `LIST UPS` reports it.
 */
export const NutRemoteUps: z.ZodObject<{
  name: z.ZodString
  description: z.ZodString
}> = /* @__PURE__ */ object({ name: z.string(), description: z.string() }).meta(
  {
    id: 'NutRemoteUps',
    description: 'A UPS served by a NUT server, as `LIST UPS` reports it.',
  },
)

/**
 * Current variables reported by the configured UPS.
 */
export const NutStatus: z.ZodObject<{
  target: z.ZodString
  variables: z.ZodRecord<z.ZodString, z.ZodString>
  monitoringError: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  target: z.string(),
  variables: z.record(z.string(), z.string()),
  monitoringError: wire(
    z.string().nullable().optional(),
    z.string().nullable(),
  ),
}).meta({
  id: 'NutStatus',
  description: 'Current variables reported by the configured UPS.',
})

/**
 * An absent update preserves DNS intent; null servers select gateway DNS.
 */
export const OutboundDnsUpdate: z.ZodObject<{
  servers: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<z.ZodString>>>,
    z.ZodNullable<z.ZodArray<z.ZodString>>
  >
}> = /* @__PURE__ */ object({
  servers: wire(
    z.array(z.string()).nullable().optional(),
    z.array(z.string()).nullable(),
  ),
}).meta({
  id: 'OutboundDnsUpdate',
  description:
    'An absent update preserves DNS intent; null servers select gateway DNS.',
})
export const PackageBackupReport: z.ZodObject<{
  error: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  duration_ms: z.ZodInt
}> = /* @__PURE__ */ object({
  error: wire(z.string().nullable().optional(), z.string().nullable()),
  duration_ms: z.int(),
}).meta({
  id: 'PackageBackupReport',
})
export const PackageDetailLevel: z.ZodEnum<{
  none: 'none'
  short: 'short'
  full: 'full'
}> = /* @__PURE__ */ z.enum(['none', 'short', 'full']).meta({
  id: 'PackageDetailLevel',
})
export const PackageId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'PackageId',
})
export const PackageVersionCount: z.ZodObject<{
  pkgId: z.ZodString
  version: z.ZodString
  count: z.ZodInt
}> = /* @__PURE__ */ object({
  pkgId: z.string(),
  version: z.string(),
  count: z.int(),
}).meta({
  id: 'PackageVersionCount',
})
export const Percentage: typeof literal_6 = /* @__PURE__ */ literal_6.meta({
  id: 'Percentage',
})
export const PluginId: z.ZodEnum<{ 'url-v0': 'url-v0' }> = /* @__PURE__ */ z
  .enum(['url-v0'])
  .meta({
    id: 'PluginId',
  })
export const PreDownloadAlertWhen: z.ZodObject<{ sourceVersion: z.ZodString }> =
  /* @__PURE__ */ object({ sourceVersion: z.string() }).meta({
    id: 'PreDownloadAlertWhen',
  })
export const ProgressUnits: z.ZodEnum<{ bytes: 'bytes'; steps: 'steps' }> =
  /* @__PURE__ */ z.enum(['bytes', 'steps']).meta({
    id: 'ProgressUnits',
  })
export const PushKeys: z.ZodObject<{
  /**
   * Base64url-encoded uncompressed P-256 public key.
   */
  p256dh: z.ZodString
  /**
   * Base64url-encoded 16-byte authentication secret.
   */
  auth: z.ZodString
}> = /* @__PURE__ */ object({
  /**
   * Base64url-encoded uncompressed P-256 public key.
   */
  p256dh: z.string().meta({
    description: 'Base64url-encoded uncompressed P-256 public key.',
  }),
  /**
   * Base64url-encoded 16-byte authentication secret.
   */
  auth: z.string().meta({
    description: 'Base64url-encoded 16-byte authentication secret.',
  }),
}).meta({
  id: 'PushKeys',
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
export const ReaderParams: typeof empty = /* @__PURE__ */ empty.meta({
  id: 'ReaderParams',
})
export const ReassignPortParams: z.ZodObject<{
  internalPort: z.ZodInt
  externalPort: z.ZodInt
  ssl: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  internalPort: z.int(),
  externalPort: z.int(),
  ssl: wire(z.boolean().optional(), z.boolean()),
}).meta({
  id: 'ReassignPortParams',
})

/**
 * A registry asset with a caller-selected commitment representation.
 */
export const RegistryAsset = <Commitment extends z.ZodType>(
  Commitment: Commitment,
) => {
  const self = object({
    publishedAt: z.string(),
    urls: z.array(z.string()),
    commitment: Commitment,
    signatures: z.record(z.string(), AnySignature),
  })
  return self
}
export const RegistryUrlParams: z.ZodObject<{ registry: z.ZodString }> =
  /* @__PURE__ */ object({ registry: z.string() }).meta({
    id: 'RegistryUrlParams',
  })
export const RemoveAssetParams: z.ZodObject<{
  version: z.ZodString
  platform: z.ZodString
}> = /* @__PURE__ */ object({ version: z.string(), platform: z.string() }).meta(
  {
    id: 'RemoveAssetParams',
  },
)
export const RemoveCategoryParams: z.ZodObject<{ id: z.ZodString }> =
  /* @__PURE__ */ object({ id: z.string() }).meta({
    id: 'RemoveCategoryParams',
  })
export const RemoveDomainParams: z.ZodObject<{ fqdn: z.ZodString }> =
  /* @__PURE__ */ object({ fqdn: z.string() }).meta({
    id: 'RemoveDomainParams',
  })

/**
 * JSON Patch 'remove' operation representation
 */
export const RemoveOperation: z.ZodObject<{
  /**
   * JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
   * within the target document where the operation is performed.
   */
  path: z.ZodString
}> = /* @__PURE__ */ object({
  /**
   * JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
   * within the target document where the operation is performed.
   */
  path: z.string().meta({
    description:
      'JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location\nwithin the target document where the operation is performed.',
  }),
}).meta({
  id: 'RemoveOperation',
  description: "JSON Patch 'remove' operation representation",
})
export const RemovePassthroughParams: z.ZodObject<{
  hostname: z.ZodString
  'listen-port': z.ZodInt
}> = /* @__PURE__ */ object({
  hostname: z.string(),
  'listen-port': z.int(),
}).meta({
  id: 'RemovePassthroughParams',
})
export const RemoveTrustedKeyParams: z.ZodObject<{ id: z.ZodString }> =
  /* @__PURE__ */ object({ id: z.string() }).meta({
    id: 'RemoveTrustedKeyParams',
  })
export const RemoveTrustedKeysParams: z.ZodObject<{
  ids: z.ZodArray<z.ZodString>
}> = /* @__PURE__ */ object({ ids: z.array(z.string()) }).meta({
  id: 'RemoveTrustedKeysParams',
})
export const RemoveVersionParams: z.ZodObject<{ version: z.ZodString }> =
  /* @__PURE__ */ object({ version: z.string() }).meta({
    id: 'RemoveVersionParams',
  })

/**
 * JSON Patch 'replace' operation representation
 */
export const ReplaceOperation: z.ZodObject<{
  /**
   * JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
   * within the target document where the operation is performed.
   */
  path: z.ZodString
  /**
   * Value to replace with.
   */
  value: z.ZodUnknown
}> = /* @__PURE__ */ object({
  /**
   * JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
   * within the target document where the operation is performed.
   */
  path: z.string().meta({
    description:
      'JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location\nwithin the target document where the operation is performed.',
  }),
  /**
   * Value to replace with.
   */
  value: z.unknown().meta({
    description: 'Value to replace with.',
  }),
}).meta({
  id: 'ReplaceOperation',
  description: "JSON Patch 'replace' operation representation",
})
export const ReplayId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'ReplayId',
})
export const RequiresReboot: z.ZodBoolean = /* @__PURE__ */ z.boolean().meta({
  id: 'RequiresReboot',
})
export const Security: z.ZodObject<{ ssl: z.ZodBoolean }> =
  /* @__PURE__ */ object({ ssl: z.boolean() }).meta({
    id: 'Security',
  })
export const ServerBackupReport: z.ZodObject<{
  attempted: z.ZodBoolean
  error: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  attempted: z.boolean(),
  error: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'ServerBackupReport',
})
export const ServerHostname: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'ServerHostname',
})
export const ServiceInterfaceId: typeof literal_0 =
  /* @__PURE__ */ literal_0.meta({
    id: 'ServiceInterfaceId',
  })
export const ServiceInterfaceType: z.ZodEnum<{
  ui: 'ui'
  p2p: 'p2p'
  api: 'api'
}> = /* @__PURE__ */ z.enum(['ui', 'p2p', 'api']).meta({
  id: 'ServiceInterfaceType',
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
export const SetCountryParams: z.ZodObject<{ country: z.ZodString }> =
  /* @__PURE__ */ object({ country: z.string() }).meta({
    id: 'SetCountryParams',
  })
export const SetEchoipUrlsParams: z.ZodObject<{
  urls: z.ZodArray<z.ZodString>
}> = /* @__PURE__ */ object({ urls: z.array(z.string()) }).meta({
  id: 'SetEchoipUrlsParams',
})
export const SetLanguageParams: z.ZodObject<{ language: z.ZodString }> =
  /* @__PURE__ */ object({ language: z.string() }).meta({
    id: 'SetLanguageParams',
  })
export const SetNameParams: z.ZodObject<{ name: z.ZodString }> =
  /* @__PURE__ */ object({ name: z.string() }).meta({
    id: 'SetNameParams',
  })
export const SetServerHostnameParams: z.ZodObject<{
  /**
   * The server's `.local` hostname: up to 32 lowercase letters, numbers, and
   * hyphens, not starting or ending with a hyphen
   */
  hostname: z.ZodString
}> = /* @__PURE__ */ object({
  /**
   * The server's `.local` hostname: up to 32 lowercase letters, numbers, and
   * hyphens, not starting or ending with a hyphen
   */
  hostname: z.string().meta({
    description:
      "The server's `.local` hostname: up to 32 lowercase letters, numbers, and\nhyphens, not starting or ending with a hyphen",
  }),
}).meta({
  id: 'SetServerHostnameParams',
})
export const SetWifiEnabledParams: z.ZodObject<{ enabled: z.ZodBoolean }> =
  /* @__PURE__ */ object({ enabled: z.boolean() }).meta({
    id: 'SetWifiEnabledParams',
  })
export const SharedVolumeAvailability: z.ZodEnum<{
  unknown: 'unknown'
  available: 'available'
  unavailable: 'unavailable'
}> = /* @__PURE__ */ z.enum(['unknown', 'available', 'unavailable']).meta({
  id: 'SharedVolumeAvailability',
})
export const SharedVolumeBackupInfo: z.ZodObject<{
  name: z.ZodString
  timestamp: z.ZodString
  /**
   * Identifies the completed copy beneath the volume's encrypted `data` directory.
   */
  generation: z.ZodString
}> = /* @__PURE__ */ object({
  name: z.string(),
  timestamp: z.string(),
  /**
   * Identifies the completed copy beneath the volume's encrypted `data` directory.
   */
  generation: z.string().meta({
    description:
      "Identifies the completed copy beneath the volume's encrypted `data` directory.",
  }),
}).meta({
  id: 'SharedVolumeBackupInfo',
})
export const SharedVolumeDrive: z.ZodObject<{
  vendor: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  model: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  label: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  capacity: z.ZodInt
}> = /* @__PURE__ */ object({
  vendor: wire(z.string().nullable().optional(), z.string().nullable()),
  model: wire(z.string().nullable().optional(), z.string().nullable()),
  label: wire(z.string().nullable().optional(), z.string().nullable()),
  capacity: z.int(),
}).meta({
  id: 'SharedVolumeDrive',
})
export const SharedVolumeGrant: z.ZodObject<{ readonly: z.ZodBoolean }> =
  /* @__PURE__ */ object({ readonly: z.boolean() }).meta({
    id: 'SharedVolumeGrant',
  })

/**
 * A 32-character uppercase base32 volume identity.
 */
export const SharedVolumeId: z.ZodString = /* @__PURE__ */ z.string().meta({
  id: 'SharedVolumeId',
  description: 'A 32-character uppercase base32 volume identity.',
})
export const ShutdownParams: z.ZodObject<{
  /**
   * Block until graceful teardown completes (default over the CLI; the
   * frontend omits this and gets an immediate reply). Cleared with
   * `--nowait`. The wait can't outlive the webserver teardown that follows
   * container shutdown, so the connection drops once services are stopped.
   */
  wait: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  /**
   * Block until graceful teardown completes (default over the CLI; the
   * frontend omits this and gets an immediate reply). Cleared with
   * `--nowait`. The wait can't outlive the webserver teardown that follows
   * container shutdown, so the connection drops once services are stopped.
   */
  wait: wire(z.boolean().optional(), z.boolean()).meta({
    description:
      "Block until graceful teardown completes (default over the CLI; the\nfrontend omits this and gets an immediate reply). Cleared with\n`--nowait`. The wait can't outlive the webserver teardown that follows\ncontainer shutdown, so the connection drops once services are stopped.",
  }),
}).meta({
  id: 'ShutdownParams',
})

/**
 * So a signal strength is a number between 0-100, I want the null option to be 0 since there is no signal
 */
export const SignalStrength: z.ZodInt = /* @__PURE__ */ z.int().meta({
  id: 'SignalStrength',
  description:
    'So a signal strength is a number between 0-100, I want the null option to be 0 since there is no signal',
})
export const SmtpSecurity: z.ZodEnum<{ starttls: 'starttls'; tls: 'tls' }> =
  /* @__PURE__ */ z.enum(['starttls', 'tls']).meta({
    id: 'SmtpSecurity',
  })
export const SshDeleteParams: z.ZodObject<{ fingerprint: z.ZodString }> =
  /* @__PURE__ */ object({ fingerprint: z.string() }).meta({
    id: 'SshDeleteParams',
  })
export const SshKeyResponse: z.ZodObject<{
  alg: z.ZodString
  fingerprint: z.ZodString
  hostname: z.ZodString
  createdAt: z.ZodString
}> = /* @__PURE__ */ object({
  alg: z.string(),
  fingerprint: z.string(),
  hostname: z.string(),
  createdAt: z.string(),
}).meta({
  id: 'SshKeyResponse',
})
export const SshPubKey: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'SshPubKey',
})

/**
 * Ssid are the names of the wifis, usually human readable.
 */
export const Ssid: z.ZodString = /* @__PURE__ */ z.string().meta({
  id: 'Ssid',
  description: 'Ssid are the names of the wifis, usually human readable.',
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
export const TermSize: z.ZodObject<{
  rows: z.ZodInt
  cols: z.ZodInt
  pixels: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodTuple<[z.ZodInt, z.ZodInt], null>>>,
    z.ZodNullable<z.ZodTuple<[z.ZodInt, z.ZodInt], null>>
  >
}> = /* @__PURE__ */ object({
  rows: z.int(),
  cols: z.int(),
  pixels: wire(
    z.tuple([z.int(), z.int()]).nullable().optional(),
    z.tuple([z.int(), z.int()]).nullable(),
  ),
}).meta({
  id: 'TermSize',
})

/**
 * JSON Patch 'test' operation representation
 */
export const TestOperation: z.ZodObject<{
  /**
   * JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
   * within the target document where the operation is performed.
   */
  path: z.ZodString
  /**
   * Value to test against.
   */
  value: z.ZodUnknown
}> = /* @__PURE__ */ object({
  /**
   * JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
   * within the target document where the operation is performed.
   */
  path: z.string().meta({
    description:
      'JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location\nwithin the target document where the operation is performed.',
  }),
  /**
   * Value to test against.
   */
  value: z.unknown().meta({
    description: 'Value to test against.',
  }),
}).meta({
  id: 'TestOperation',
  description: "JSON Patch 'test' operation representation",
})
export const TimeInfo: z.ZodObject<{ now: z.ZodString; uptime: z.ZodInt }> =
  /* @__PURE__ */ object({ now: z.string(), uptime: z.int() }).meta({
    id: 'TimeInfo',
  })
export const TrustCaParams: z.ZodObject<{ cert: typeof literal_0 }> =
  /* @__PURE__ */ object({ cert: literal_0 }).meta({
    id: 'TrustCaParams',
  })
export const TrustContext: z.ZodEnum<{ marketplace: 'marketplace'; os: 'os' }> =
  /* @__PURE__ */ z.enum(['marketplace', 'os']).meta({
    id: 'TrustContext',
  })
export const TrustedCa: z.ZodObject<{
  subject: z.ZodString
  fingerprint: z.ZodString
}> = /* @__PURE__ */ object({
  subject: z.string(),
  fingerprint: z.string(),
}).meta({
  id: 'TrustedCa',
})
export const TrustedRegistryParams: z.ZodObject<{ url: z.ZodString }> =
  /* @__PURE__ */ object({ url: z.string() }).meta({
    id: 'TrustedRegistryParams',
  })
export const UiParams: z.ZodObject<{
  pointer: z.ZodString
  value: z.ZodUnknown
}> = /* @__PURE__ */ object({ pointer: z.string(), value: z.unknown() }).meta({
  id: 'UiParams',
})

/**
 * Changes a key's name and both trust selections atomically.
 */
export const UpdateTrustedKeyParams: z.ZodObject<{
  id: z.ZodString
  name: z.ZodString
  marketplace: z.ZodBoolean
  os: z.ZodBoolean
}> = /* @__PURE__ */ object({
  id: z.string(),
  name: z.string(),
  marketplace: z.boolean(),
  os: z.boolean(),
}).meta({
  id: 'UpdateTrustedKeyParams',
  description: "Changes a key's name and both trust selections atomically.",
})

/**
 * How the OS reverse proxy validates the container's TLS certificate when it
 * rewraps SSL (`add_ssl` set AND `secure.ssl == true`, so the OS terminates
 * the client's TLS and initiates a fresh TLS connection to the container).
 * Absent (`None`) means validate against the StartOS root CA — the default.
 */
export const UpstreamCertValidation: z.ZodUnion<
  readonly [
    z.ZodLiteral<'disable'>,
    z.ZodObject<{
      /**
       * Validate against this PEM-encoded certificate (or chain) instead of the
       * StartOS root CA.
       */
      certificate: z.ZodString
    }>,
  ]
> = /* @__PURE__ */ z
  .union([
    z.literal('disable').meta({
      description:
        "Do not validate the container's certificate at all. Use when the\ncontainer serves a self-signed cert on the trusted internal bridge.",
    }),
    object({
      /**
       * Validate against this PEM-encoded certificate (or chain) instead of the
       * StartOS root CA.
       */
      certificate: z.string().meta({
        description:
          'Validate against this PEM-encoded certificate (or chain) instead of the\nStartOS root CA.',
      }),
    }).meta({
      description:
        'Validate against this PEM-encoded certificate (or chain) instead of the\nStartOS root CA.',
    }),
  ])
  .meta({
    id: 'UpstreamCertValidation',
    description:
      "How the OS reverse proxy validates the container's TLS certificate when it\nrewraps SSL (`add_ssl` set AND `secure.ssl == true`, so the OS terminates\nthe client's TLS and initiates a fresh TLS connection to the container).\nAbsent (`None`) means validate against the StartOS root CA — the default.",
  })
export const UsersResponse: z.ZodObject<{
  uniqueServers: z.ZodInt
  totalCheckins: z.ZodInt
}> = /* @__PURE__ */ object({
  uniqueServers: z.int(),
  totalCheckins: z.int(),
}).meta({
  id: 'UsersResponse',
})
export const Version: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'Version',
})
export const WifiAddParams: z.ZodObject<{
  ssid: z.ZodString
  password: z.ZodString
}> = /* @__PURE__ */ object({ ssid: z.string(), password: z.string() }).meta({
  id: 'WifiAddParams',
})
export const WifiSsidParams: z.ZodObject<{ ssid: z.ZodString }> =
  /* @__PURE__ */ object({ ssid: z.string() }).meta({
    id: 'WifiSsidParams',
  })
export const ZramParams: z.ZodObject<{ enable: z.ZodBoolean }> =
  /* @__PURE__ */ object({ enable: z.boolean() }).meta({
    id: 'ZramParams',
  })
export const AcceptSigners: z.ZodUnion<
  readonly [
    z.ZodObject<{ signer: typeof AnyVerifyingKey }>,
    z.ZodObject<{ any: z.ZodArray<typeof AcceptSigners> }>,
    z.ZodObject<{ all: z.ZodArray<typeof AcceptSigners> }>,
  ]
> = /* @__PURE__ */ z
  .union([
    object({ signer: AnyVerifyingKey }),
    object({
      get any(): z.ZodArray<typeof AcceptSigners> {
        return z.array(AcceptSigners)
      },
    }),
    object({
      get all(): z.ZodArray<typeof AcceptSigners> {
        return z.array(AcceptSigners)
      },
    }),
  ])
  .meta({
    id: 'AcceptSigners',
  })
export const ActionInput: z.ZodObject<{
  eventId: z.ZodCodec<z.ZodOptional<typeof Guid>, typeof Guid>
  spec: typeof literal_2
  value: z.ZodCodec<z.ZodOptional<typeof literal_1>, typeof literal_1>
}> = /* @__PURE__ */ object({
  eventId: wire(Guid.optional(), Guid),
  spec: literal_2,
  value: wire(literal_1.optional(), literal_1),
}).meta({
  id: 'ActionInput',
})
export const ActionResultValue: z.ZodUnion<
  readonly [
    z.ZodIntersection<
      z.ZodObject<{ type: z.ZodLiteral<'file'> }>,
      z.ZodObject<{
        /**
         * The capability prepared by this action's prepare-download effect.
         */
        value: typeof Guid
        /**
         * Removes the exported file after completion, failure, cancellation, or expiry.
         */
        delete: z.ZodBoolean
      }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ type: z.ZodLiteral<'single'> }>,
      z.ZodObject<{
        /**
         * The actual string value to display. The UI renders it as a single-line field —
         * multi-line text belongs in a `multiline` value.
         */
        value: z.ZodString
        /**
         * (optional) Whether or not to include a copy to clipboard icon to copy the value
         */
        copyable: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodBoolean>>,
          z.ZodNullable<z.ZodBoolean>
        >
        /**
         * (optional) Whether or not to also display the value as a QR code
         */
        qr: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodBoolean>>,
          z.ZodNullable<z.ZodBoolean>
        >
        /**
         * (optional) Whether or not to mask the value using ●●●●●●●, which is useful for password or other sensitive information
         */
        masked: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodBoolean>>,
          z.ZodNullable<z.ZodBoolean>
        >
        /**
         * (optional) Whether or not to include an open in new tab icon to launch the value, which must be an http(s) URL
         */
        launchable: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodBoolean>>,
          z.ZodNullable<z.ZodBoolean>
        >
      }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ type: z.ZodLiteral<'multiline'> }>,
      z.ZodObject<{
        /**
         * The actual string value to display. The UI renders it verbatim in a read-only monospace field that keeps its line breaks
         */
        value: z.ZodString
        /**
         * (optional) Whether or not to include a copy to clipboard icon to copy the value
         */
        copyable: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodBoolean>>,
          z.ZodNullable<z.ZodBoolean>
        >
        /**
         * (optional) Whether or not to also display the value as a QR code
         */
        qr: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodBoolean>>,
          z.ZodNullable<z.ZodBoolean>
        >
        /**
         * (optional) Whether or not to blur the value until the user reveals it, which is useful for a private key or other sensitive information
         */
        masked: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodBoolean>>,
          z.ZodNullable<z.ZodBoolean>
        >
        /**
         * (optional) Also offer the value as a download under this file name, such as "diagnostics.txt"
         */
        filename: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodString>>,
          z.ZodNullable<z.ZodString>
        >
      }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ type: z.ZodLiteral<'group'> }>,
      z.ZodObject<{
        /**
         * An new group of nested values, experienced by the user as an accordion dropdown
         */
        value: z.ZodArray<typeof ActionResultMember>
      }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(
      object({ type: z.literal('file') }),
      object({
        /**
         * The capability prepared by this action's prepare-download effect.
         */
        value: Guid.meta({
          description:
            "The capability prepared by this action's prepare-download effect.",
        }),
        /**
         * Removes the exported file after completion, failure, cancellation, or expiry.
         */
        delete: z.boolean().meta({
          description:
            'Removes the exported file after completion, failure, cancellation, or expiry.',
        }),
      }),
    ),
    z.intersection(
      object({ type: z.literal('single') }),
      object({
        /**
         * The actual string value to display. The UI renders it as a single-line field —
         * multi-line text belongs in a `multiline` value.
         */
        value: z.string().meta({
          description:
            'The actual string value to display. The UI renders it as a single-line field —\nmulti-line text belongs in a `multiline` value.',
        }),
        /**
         * (optional) Whether or not to include a copy to clipboard icon to copy the value
         */
        copyable: wire(
          z.boolean().nullable().optional(),
          z.boolean().nullable(),
        ).meta({
          description:
            '(optional) Whether or not to include a copy to clipboard icon to copy the value',
        }),
        /**
         * (optional) Whether or not to also display the value as a QR code
         */
        qr: wire(
          z.boolean().nullable().optional(),
          z.boolean().nullable(),
        ).meta({
          description:
            '(optional) Whether or not to also display the value as a QR code',
        }),
        /**
         * (optional) Whether or not to mask the value using ●●●●●●●, which is useful for password or other sensitive information
         */
        masked: wire(
          z.boolean().nullable().optional(),
          z.boolean().nullable(),
        ).meta({
          description:
            '(optional) Whether or not to mask the value using ●●●●●●●, which is useful for password or other sensitive information',
        }),
        /**
         * (optional) Whether or not to include an open in new tab icon to launch the value, which must be an http(s) URL
         */
        launchable: wire(
          z.boolean().nullable().optional(),
          z.boolean().nullable(),
        ).meta({
          description:
            '(optional) Whether or not to include an open in new tab icon to launch the value, which must be an http(s) URL',
        }),
      }),
    ),
    z.intersection(
      object({ type: z.literal('multiline') }),
      object({
        /**
         * The actual string value to display. The UI renders it verbatim in a read-only monospace field that keeps its line breaks
         */
        value: z.string().meta({
          description:
            'The actual string value to display. The UI renders it verbatim in a read-only monospace field that keeps its line breaks',
        }),
        /**
         * (optional) Whether or not to include a copy to clipboard icon to copy the value
         */
        copyable: wire(
          z.boolean().nullable().optional(),
          z.boolean().nullable(),
        ).meta({
          description:
            '(optional) Whether or not to include a copy to clipboard icon to copy the value',
        }),
        /**
         * (optional) Whether or not to also display the value as a QR code
         */
        qr: wire(
          z.boolean().nullable().optional(),
          z.boolean().nullable(),
        ).meta({
          description:
            '(optional) Whether or not to also display the value as a QR code',
        }),
        /**
         * (optional) Whether or not to blur the value until the user reveals it, which is useful for a private key or other sensitive information
         */
        masked: wire(
          z.boolean().nullable().optional(),
          z.boolean().nullable(),
        ).meta({
          description:
            '(optional) Whether or not to blur the value until the user reveals it, which is useful for a private key or other sensitive information',
        }),
        /**
         * (optional) Also offer the value as a download under this file name, such as "diagnostics.txt"
         */
        filename: wire(
          z.string().nullable().optional(),
          z.string().nullable(),
        ).meta({
          description:
            '(optional) Also offer the value as a download under this file name, such as "diagnostics.txt"',
        }),
      }),
    ),
    z.intersection(
      object({ type: z.literal('group') }),
      object({
        /**
         * An new group of nested values, experienced by the user as an accordion dropdown
         */
        get value(): z.ZodArray<typeof ActionResultMember> {
          return z.array(ActionResultMember).meta({
            description:
              'An new group of nested values, experienced by the user as an accordion dropdown',
          })
        },
      }),
    ),
  ])
  .meta({
    id: 'ActionResultValue',
  })
export const AddAdminParams: z.ZodObject<{ signer: typeof Guid }> =
  /* @__PURE__ */ object({ signer: Guid }).meta({
    id: 'AddAdminParams',
  })
export const AddAssetParams: z.ZodObject<{
  version: z.ZodString
  platform: z.ZodString
  url: z.ZodString
  signature: typeof AnySignature
  signatures: z.ZodCodec<
    z.ZodOptional<z.ZodRecord<z.ZodString, typeof AnySignature>>,
    z.ZodRecord<z.ZodString, typeof AnySignature>
  >
  commitment: typeof Blake3Commitment
}> = /* @__PURE__ */ object({
  version: z.string(),
  platform: z.string(),
  url: z.string(),
  signature: AnySignature,
  signatures: wire(
    z.record(z.string(), AnySignature).optional(),
    z.record(z.string(), AnySignature),
  ),
  commitment: Blake3Commitment,
}).meta({
  id: 'AddAssetParams',
})
export const AddCategoryParams: z.ZodObject<{
  id: z.ZodString
  name: typeof LocaleString
}> = /* @__PURE__ */ object({ id: z.string(), name: LocaleString }).meta({
  id: 'AddCategoryParams',
})
export const AddMirrorParams: z.ZodObject<{
  url: z.ZodString
  commitment: typeof MerkleArchiveCommitment
  signature: typeof AnySignature
  signatures: z.ZodCodec<
    z.ZodOptional<z.ZodRecord<z.ZodString, typeof AnySignature>>,
    z.ZodRecord<z.ZodString, typeof AnySignature>
  >
}> = /* @__PURE__ */ object({
  url: z.string(),
  commitment: MerkleArchiveCommitment,
  signature: AnySignature,
  signatures: wire(
    z.record(z.string(), AnySignature).optional(),
    z.record(z.string(), AnySignature),
  ),
}).meta({
  id: 'AddMirrorParams',
})
export const AddPackageParams: z.ZodObject<{
  urls: z.ZodArray<z.ZodString>
  commitment: typeof MerkleArchiveCommitment
  signature: typeof AnySignature
  signatures: z.ZodCodec<
    z.ZodOptional<z.ZodRecord<z.ZodString, typeof AnySignature>>,
    z.ZodRecord<z.ZodString, typeof AnySignature>
  >
}> = /* @__PURE__ */ object({
  urls: z.array(z.string()),
  commitment: MerkleArchiveCommitment,
  signature: AnySignature,
  signatures: wire(
    z.record(z.string(), AnySignature).optional(),
    z.record(z.string(), AnySignature),
  ),
}).meta({
  id: 'AddPackageParams',
})
export const AddPackageSignerParams: z.ZodObject<{
  id: typeof PackageId
  signer: typeof Guid
  versions: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  merge: z.ZodBoolean
}> = /* @__PURE__ */ object({
  id: PackageId,
  signer: Guid,
  versions: wire(z.string().nullable().optional(), z.string().nullable()),
  merge: z.boolean(),
}).meta({
  id: 'AddPackageSignerParams',
})
export const AddPackageToCategoryParams: z.ZodObject<{
  id: z.ZodString
  package: typeof PackageId
}> = /* @__PURE__ */ object({ id: z.string(), package: PackageId }).meta({
  id: 'AddPackageToCategoryParams',
})
export const AddPassthroughParams: z.ZodObject<{
  hostname: z.ZodString
  'listen-port': z.ZodInt
  backend: z.ZodString
  'public-gateway': z.ZodArray<typeof GatewayId>
  'private-ip': z.ZodArray<z.ZodString>
}> = /* @__PURE__ */ object({
  hostname: z.string(),
  'listen-port': z.int(),
  backend: z.string(),
  'public-gateway': z.array(GatewayId),
  'private-ip': z.array(z.string()),
}).meta({
  id: 'AddPassthroughParams',
})
export const AddPrivateDomainParams: z.ZodObject<{
  fqdn: z.ZodString
  gateway: typeof GatewayId
}> = /* @__PURE__ */ object({ fqdn: z.string(), gateway: GatewayId }).meta({
  id: 'AddPrivateDomainParams',
})
export const AddPublicDomainParams: z.ZodObject<{
  fqdn: z.ZodString
  acme: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof AcmeProvider>>,
    z.ZodNullable<typeof AcmeProvider>
  >
  gateway: typeof GatewayId
  internalPort: z.ZodInt
}> = /* @__PURE__ */ object({
  fqdn: z.string(),
  acme: wire(AcmeProvider.nullable().optional(), AcmeProvider.nullable()),
  gateway: GatewayId,
  internalPort: z.int(),
}).meta({
  id: 'AddPublicDomainParams',
})
export const AddSignerParams: z.ZodObject<{
  name: z.ZodString
  contact: z.ZodArray<typeof ContactInfo>
  /**
   * Ed25519 public keys in PEM, canonical DER Base64, or armored OpenPGP format.
   */
  keys: z.ZodArray<z.ZodString>
  /**
   * Retained certificates; `keys` determines the selected signing keys.
   */
  gpgExports: z.ZodCodec<
    z.ZodOptional<z.ZodArray<typeof GpgPublicKeyExport>>,
    z.ZodArray<typeof GpgPublicKeyExport>
  >
}> = /* @__PURE__ */ object({
  name: z.string(),
  contact: z.array(ContactInfo),
  /**
   * Ed25519 public keys in PEM, canonical DER Base64, or armored OpenPGP format.
   */
  keys: z.array(z.string()).meta({
    description:
      'Ed25519 public keys in PEM, canonical DER Base64, or armored OpenPGP format.',
  }),
  /**
   * Retained certificates; `keys` determines the selected signing keys.
   */
  gpgExports: wire(
    z.array(GpgPublicKeyExport).optional(),
    z.array(GpgPublicKeyExport),
  ).meta({
    description:
      'Retained certificates; `keys` determines the selected signing keys.',
  }),
}).meta({
  id: 'AddSignerParams',
})
export const AddTrustedKeyParams: z.ZodObject<{
  /**
   * Blank names each GPG key after its identity.
   */
  name: z.ZodString
  /**
   * Ed25519 public keys in PEM or armored GPG format.
   */
  publicKeys: z.ZodArray<z.ZodString>
  /**
   * Grants this permission to every imported key, including existing keys.
   */
  trustFor: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof TrustContext>>,
    z.ZodNullable<typeof TrustContext>
  >
}> = /* @__PURE__ */ object({
  /**
   * Blank names each GPG key after its identity.
   */
  name: z.string().meta({
    description: 'Blank names each GPG key after its identity.',
  }),
  /**
   * Ed25519 public keys in PEM or armored GPG format.
   */
  publicKeys: z.array(z.string()).meta({
    description: 'Ed25519 public keys in PEM or armored GPG format.',
  }),
  /**
   * Grants this permission to every imported key, including existing keys.
   */
  trustFor: wire(
    TrustContext.nullable().optional(),
    TrustContext.nullable(),
  ).meta({
    description:
      'Grants this permission to every imported key, including existing keys.',
  }),
}).meta({
  id: 'AddTrustedKeyParams',
})
export const AddTunnelParams: z.ZodObject<{
  name: z.ZodString
  config: z.ZodString
  type: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof GatewayType>>,
    z.ZodNullable<typeof GatewayType>
  >
  setAsDefaultOutbound: z.ZodBoolean
}> = /* @__PURE__ */ object({
  name: z.string(),
  config: z.string(),
  type: wire(GatewayType.nullable().optional(), GatewayType.nullable()),
  setAsDefaultOutbound: z.boolean(),
}).meta({
  id: 'AddTunnelParams',
})
export const AddVersionParams: z.ZodObject<{
  version: z.ZodString
  headline: z.ZodString
  releaseNotes: typeof LocaleString
  sourceVersion: z.ZodString
  gitHash: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof GitHash>>,
    z.ZodNullable<typeof GitHash>
  >
  expectedGitHash: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof GitHash>>,
    z.ZodNullable<typeof GitHash>
  >
  security: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  version: z.string(),
  headline: z.string(),
  releaseNotes: LocaleString,
  sourceVersion: z.string(),
  gitHash: wire(GitHash.nullable().optional(), GitHash.nullable()),
  expectedGitHash: wire(GitHash.nullable().optional(), GitHash.nullable()),
  security: wire(z.boolean().optional(), z.boolean()),
}).meta({
  id: 'AddVersionParams',
})
export const AddressInfo: z.ZodObject<{
  username: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  hostId: typeof HostId
  internalPort: z.ZodInt
  scheme: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  sslScheme: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  suffix: z.ZodString
}> = /* @__PURE__ */ object({
  username: wire(z.string().nullable().optional(), z.string().nullable()),
  hostId: HostId,
  internalPort: z.int(),
  scheme: wire(z.string().nullable().optional(), z.string().nullable()),
  sslScheme: wire(z.string().nullable().optional(), z.string().nullable()),
  suffix: z.string(),
}).meta({
  id: 'AddressInfo',
})

/**
 * The protocols a binding answers with, carried on the wire as the list itself.
 */
export const AlpnInfo: z.ZodArray<typeof MaybeUtf8String> = /* @__PURE__ */ z
  .array(MaybeUtf8String)
  .meta({
    id: 'AlpnInfo',
    description:
      'The protocols a binding answers with, carried on the wire as the list itself.',
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
export const AttachParams: z.ZodObject<{
  id: typeof PackageId
  /**
   * Accepts strings and legacy platform arguments without losing bytes.
   */
  command: z.ZodArray<z.ZodString>
  tty: z.ZodBoolean
  stderrTty: z.ZodBoolean
  ptySize: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof TermSize>>,
    z.ZodNullable<typeof TermSize>
  >
  subcontainer: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Guid>>,
    z.ZodNullable<typeof Guid>
  >
  name: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  imageId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof ImageId>>,
    z.ZodNullable<typeof ImageId>
  >
  user: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  id: PackageId,
  /**
   * Accepts strings and legacy platform arguments without losing bytes.
   */
  command: z.array(z.string()).meta({
    description:
      'Accepts strings and legacy platform arguments without losing bytes.',
  }),
  tty: z.boolean(),
  stderrTty: z.boolean(),
  ptySize: wire(TermSize.nullable().optional(), TermSize.nullable()),
  subcontainer: wire(Guid.nullable().optional(), Guid.nullable()),
  name: wire(z.string().nullable().optional(), z.string().nullable()),
  imageId: wire(ImageId.nullable().optional(), ImageId.nullable()),
  user: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'AttachParams',
})

/**
 * Enrolled sign-ins indexed by canonical DER64 SPKI.
 */
export const AuthKeys: z.ZodRecord<z.ZodString, typeof Session> =
  /* @__PURE__ */ z.record(z.string(), Session).meta({
    id: 'AuthKeys',
    description: 'Enrolled sign-ins indexed by canonical DER64 SPKI.',
  })
export const CancelInstallParams: z.ZodObject<{ id: typeof PackageId }> =
  /* @__PURE__ */ object({ id: PackageId }).meta({
    id: 'CancelInstallParams',
  })
export const Category: z.ZodObject<{ name: typeof LocaleString }> =
  /* @__PURE__ */ object({ name: LocaleString }).meta({
    id: 'Category',
  })
export const CheckChallengeParams: z.ZodObject<{
  fqdn: z.ZodString
  gateway: typeof GatewayId
  acme: typeof AcmeProvider
}> = /* @__PURE__ */ object({
  fqdn: z.string(),
  gateway: GatewayId,
  acme: AcmeProvider,
}).meta({
  id: 'CheckChallengeParams',
})

/**
 * Reachability of the port a certificate authority validates on. A null leg
 * went unprobed.
 */
export const CheckChallengeRes: z.ZodObject<{
  port: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof CheckPortRes>>,
    z.ZodNullable<typeof CheckPortRes>
  >
  portV6: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof CheckPortV6Res>>,
    z.ZodNullable<typeof CheckPortV6Res>
  >
}> = /* @__PURE__ */ object({
  port: wire(CheckPortRes.nullable().optional(), CheckPortRes.nullable()),
  portV6: wire(CheckPortV6Res.nullable().optional(), CheckPortV6Res.nullable()),
}).meta({
  id: 'CheckChallengeRes',
  description:
    'Reachability of the port a certificate authority validates on. A null leg\nwent unprobed.',
})
export const CheckDnsParams: z.ZodObject<{
  gateway: typeof GatewayId
  fqdn: z.ZodString
}> = /* @__PURE__ */ object({ gateway: GatewayId, fqdn: z.string() }).meta({
  id: 'CheckDnsParams',
})
export const CheckPortParams: z.ZodObject<{
  port: z.ZodInt
  gateway: typeof GatewayId
}> = /* @__PURE__ */ object({ port: z.int(), gateway: GatewayId }).meta({
  id: 'CheckPortParams',
})
export const CifsRemoveParams: z.ZodObject<{ id: typeof BackupTargetId }> =
  /* @__PURE__ */ object({ id: BackupTargetId }).meta({
    id: 'CifsRemoveParams',
  })
export const CifsUpdateParams: z.ZodObject<{
  id: typeof BackupTargetId
  hostname: z.ZodString
  path: z.ZodString
  username: z.ZodString
  password: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  id: BackupTargetId,
  hostname: z.string(),
  path: z.string(),
  username: z.string(),
  password: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'CifsUpdateParams',
})
export const ClearTaskParams: z.ZodObject<{
  packageId: typeof PackageId
  replayId: typeof ReplayId
  force: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  packageId: PackageId,
  replayId: ReplayId,
  force: wire(z.boolean().optional(), z.boolean()),
}).meta({
  id: 'ClearTaskParams',
})
export const ControlParams: z.ZodObject<{ id: typeof PackageId }> =
  /* @__PURE__ */ object({ id: PackageId }).meta({
    id: 'ControlParams',
  })
export const CreateParams: z.ZodObject<{
  package: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof PackageId>>,
    z.ZodNullable<typeof PackageId>
  >
  level: typeof NotificationLevel
  title: z.ZodString
  message: z.ZodString
}> = /* @__PURE__ */ object({
  package: wire(PackageId.nullable().optional(), PackageId.nullable()),
  level: NotificationLevel,
  title: z.string(),
  message: z.string(),
}).meta({
  id: 'CreateParams',
})
export const CreateSharedVolumeSource: z.ZodUnion<
  readonly [
    z.ZodObject<{ kind: z.ZodLiteral<'local'> }>,
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'disk'> }>,
      z.ZodObject<{
        logicalname: z.ZodString
        path: z.ZodCodec<z.ZodOptional<z.ZodString>, z.ZodString>
      }>
    >,
    z.ZodIntersection<z.ZodObject<{ kind: z.ZodLiteral<'cifs'> }>, typeof Cifs>,
  ]
> = /* @__PURE__ */ z
  .union([
    object({ kind: z.literal('local') }),
    z.intersection(
      object({ kind: z.literal('disk') }),
      object({
        logicalname: z.string(),
        path: wire(z.string().optional(), z.string()),
      }),
    ),
    z.intersection(object({ kind: z.literal('cifs') }), Cifs),
  ])
  .meta({
    id: 'CreateSharedVolumeSource',
  })
export const CurrentDependencyKind: z.ZodUnion<
  readonly [
    z.ZodObject<{ kind: z.ZodLiteral<'exists'> }>,
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'running'> }>,
      z.ZodObject<{
        healthChecks: z.ZodCodec<
          z.ZodOptional<z.ZodArray<typeof HealthCheckId>>,
          z.ZodArray<typeof HealthCheckId>
        >
      }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    object({ kind: z.literal('exists') }),
    z.intersection(
      object({ kind: z.literal('running') }),
      object({
        healthChecks: wire(
          z.array(HealthCheckId).optional(),
          z.array(HealthCheckId),
        ),
      }),
    ),
  ])
  .meta({
    id: 'CurrentDependencyKind',
  })
export const DeleteLegacyParams: z.ZodObject<{
  targetId: typeof BackupTargetId
}> = /* @__PURE__ */ object({ targetId: BackupTargetId }).meta({
  id: 'DeleteLegacyParams',
})
export const Description: z.ZodObject<{
  short: typeof LocaleString
  long: typeof LocaleString
}> = /* @__PURE__ */ object({ short: LocaleString, long: LocaleString }).meta({
  id: 'Description',
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
export const DownloadsResponse: z.ZodObject<{
  totalRequests: z.ZodInt
  byPackage: z.ZodArray<typeof CountEntry>
  byPackageVersion: z.ZodArray<typeof PackageVersionCount>
}> = /* @__PURE__ */ object({
  totalRequests: z.int(),
  byPackage: z.array(CountEntry),
  byPackageVersion: z.array(PackageVersionCount),
}).meta({
  id: 'DownloadsResponse',
})
export const EditGatewayParams: z.ZodObject<{
  id: typeof GatewayId
  name: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  dns: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof OutboundDnsUpdate>>,
    z.ZodNullable<typeof OutboundDnsUpdate>
  >
}> = /* @__PURE__ */ object({
  id: GatewayId,
  name: wire(z.string().nullable().optional(), z.string().nullable()),
  dns: wire(
    OutboundDnsUpdate.nullable().optional(),
    OutboundDnsUpdate.nullable(),
  ),
}).meta({
  id: 'EditGatewayParams',
})
export const EditSignerParams: z.ZodObject<{
  id: typeof Guid
  setName: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  addContact: z.ZodArray<typeof ContactInfo>
  addKey: z.ZodArray<z.ZodString>
  removeContact: z.ZodArray<typeof ContactInfo>
  removeKey: z.ZodArray<typeof AnyVerifyingKey>
  /**
   * Removes whole exports containing a matching primary-key fingerprint, preserving signing keys.
   */
  removeGpgExport: z.ZodCodec<
    z.ZodOptional<z.ZodArray<z.ZodString>>,
    z.ZodArray<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  id: Guid,
  setName: wire(z.string().nullable().optional(), z.string().nullable()),
  addContact: z.array(ContactInfo),
  addKey: z.array(z.string()),
  removeContact: z.array(ContactInfo),
  removeKey: z.array(AnyVerifyingKey),
  /**
   * Removes whole exports containing a matching primary-key fingerprint, preserving signing keys.
   */
  removeGpgExport: wire(
    z.array(z.string()).optional(),
    z.array(z.string()),
  ).meta({
    description:
      'Removes whole exports containing a matching primary-key fingerprint, preserving signing keys.',
  }),
}).meta({
  id: 'EditSignerParams',
})
export const EppInfo: z.ZodObject<{
  current: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Epp>>,
    z.ZodNullable<typeof Epp>
  >
  available: z.ZodArray<typeof Epp>
}> = /* @__PURE__ */ object({
  current: wire(Epp.nullable().optional(), Epp.nullable()),
  available: z.array(Epp),
}).meta({
  id: 'EppInfo',
})
export const EppParams: z.ZodObject<{
  set: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Epp>>,
    z.ZodNullable<typeof Epp>
  >
}> = /* @__PURE__ */ object({
  set: wire(Epp.nullable().optional(), Epp.nullable()),
}).meta({
  id: 'EppParams',
})
export const ExecParams: z.ZodObject<{
  id: typeof PackageId
  /**
   * Program and arguments, run without a shell. Empty runs the user's login shell.
   */
  command: z.ZodArray<z.ZodString>
  /**
   * Written to the command's standard input, which is otherwise empty.
   */
  stdin: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Seconds before the command is terminated. Defaults to 120.
   */
  timeoutSecs: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  /**
   * Part of the subcontainer's ID. Overrides `name` and `imageId`.
   */
  subcontainer: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Guid>>,
    z.ZodNullable<typeof Guid>
  >
  /**
   * Part of the subcontainer's name, case-insensitive. Overrides `imageId`.
   */
  name: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Part of the subcontainer's image ID, case-insensitive.
   */
  imageId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof ImageId>>,
    z.ZodNullable<typeof ImageId>
  >
  /**
   * Defaults to the image's user, else root.
   */
  user: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  id: PackageId,
  /**
   * Program and arguments, run without a shell. Empty runs the user's login shell.
   */
  command: z.array(z.string()).meta({
    description:
      "Program and arguments, run without a shell. Empty runs the user's login shell.",
  }),
  /**
   * Written to the command's standard input, which is otherwise empty.
   */
  stdin: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      "Written to the command's standard input, which is otherwise empty.",
  }),
  /**
   * Seconds before the command is terminated. Defaults to 120.
   */
  timeoutSecs: wire(z.int().nullable().optional(), z.int().nullable()).meta({
    description: 'Seconds before the command is terminated. Defaults to 120.',
  }),
  /**
   * Part of the subcontainer's ID. Overrides `name` and `imageId`.
   */
  subcontainer: wire(Guid.nullable().optional(), Guid.nullable()).meta({
    description:
      "Part of the subcontainer's ID. Overrides `name` and `imageId`.",
  }),
  /**
   * Part of the subcontainer's name, case-insensitive. Overrides `imageId`.
   */
  name: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      "Part of the subcontainer's name, case-insensitive. Overrides `imageId`.",
  }),
  /**
   * Part of the subcontainer's image ID, case-insensitive.
   */
  imageId: wire(ImageId.nullable().optional(), ImageId.nullable()).meta({
    description: "Part of the subcontainer's image ID, case-insensitive.",
  }),
  /**
   * Defaults to the image's user, else root.
   */
  user: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description: "Defaults to the image's user, else root.",
  }),
}).meta({
  id: 'ExecParams',
})
export const ForgetGatewayParams: z.ZodObject<{ gateway: typeof GatewayId }> =
  /* @__PURE__ */ object({ gateway: GatewayId }).meta({
    id: 'ForgetGatewayParams',
  })
export const ForwardTable: z.ZodRecord<z.ZodString, typeof ForwardTarget> =
  /* @__PURE__ */ z.record(z.string(), ForwardTarget).meta({
    id: 'ForwardTable',
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
export const GetActionInputParams: z.ZodObject<{
  packageId: typeof PackageId
  actionId: typeof ActionId
  prefill: z.ZodCodec<z.ZodOptional<typeof literal_1>, typeof literal_1>
}> = /* @__PURE__ */ object({
  packageId: PackageId,
  actionId: ActionId,
  prefill: wire(literal_1.optional(), literal_1),
}).meta({
  id: 'GetActionInputParams',
})
export const GetPackageAccessParams: z.ZodObject<{ id: typeof PackageId }> =
  /* @__PURE__ */ object({ id: PackageId }).meta({
    id: 'GetPackageAccessParams',
  })
export const GetPackageParams: z.ZodIntersection<
  z.ZodObject<{
    id: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<typeof PackageId>>,
      z.ZodNullable<typeof PackageId>
    >
    targetVersion: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<z.ZodString>>,
      z.ZodNullable<z.ZodString>
    >
    sourceVersion: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<typeof Version>>,
      z.ZodNullable<typeof Version>
    >
    otherVersions: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<typeof PackageDetailLevel>>,
      z.ZodNullable<typeof PackageDetailLevel>
    >
    allRevisions: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  }>,
  typeof ReaderParams
> = /* @__PURE__ */ z
  .intersection(
    object({
      id: wire(PackageId.nullable().optional(), PackageId.nullable()),
      targetVersion: wire(
        z.string().nullable().optional(),
        z.string().nullable(),
      ),
      sourceVersion: wire(Version.nullable().optional(), Version.nullable()),
      otherVersions: wire(
        PackageDetailLevel.nullable().optional(),
        PackageDetailLevel.nullable(),
      ),
      allRevisions: wire(z.boolean().optional(), z.boolean()),
    }),
    ReaderParams,
  )
  .meta({
    id: 'GetPackageParams',
  })
export const GovernorInfo: z.ZodObject<{
  current: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Governor>>,
    z.ZodNullable<typeof Governor>
  >
  available: z.ZodArray<typeof Governor>
}> = /* @__PURE__ */ object({
  current: wire(Governor.nullable().optional(), Governor.nullable()),
  available: z.array(Governor),
}).meta({
  id: 'GovernorInfo',
})
export const GovernorParams: z.ZodObject<{
  set: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Governor>>,
    z.ZodNullable<typeof Governor>
  >
}> = /* @__PURE__ */ object({
  set: wire(Governor.nullable().optional(), Governor.nullable()),
}).meta({
  id: 'GovernorParams',
})
export const HardwareRequirements: z.ZodObject<{
  device: z.ZodCodec<
    z.ZodOptional<z.ZodArray<typeof DeviceFilter>>,
    z.ZodArray<typeof DeviceFilter>
  >
  ram: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  arch: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<z.ZodString>>>,
    z.ZodNullable<z.ZodArray<z.ZodString>>
  >
}> = /* @__PURE__ */ object({
  device: wire(z.array(DeviceFilter).optional(), z.array(DeviceFilter)),
  ram: wire(z.int().nullable().optional(), z.int().nullable()),
  arch: wire(
    z.array(z.string()).nullable().optional(),
    z.array(z.string()).nullable(),
  ),
}).meta({
  id: 'HardwareRequirements',
})
export const HostnameMetadata: z.ZodUnion<
  readonly [
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'ipv4'> }>,
      z.ZodObject<{ gateway: typeof GatewayId }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'ipv6'> }>,
      z.ZodObject<{ gateway: typeof GatewayId; scopeId: z.ZodInt }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'mdns'> }>,
      z.ZodObject<{ gateways: z.ZodArray<typeof GatewayId> }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'private-domain'> }>,
      z.ZodObject<{ gateways: z.ZodArray<typeof GatewayId> }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'public-domain'> }>,
      z.ZodObject<{ gateway: typeof GatewayId }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'plugin'> }>,
      z.ZodObject<{
        packageId: typeof PackageId
        removeAction: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<typeof ActionId>>,
          z.ZodNullable<typeof ActionId>
        >
        overflowActions: z.ZodArray<typeof ActionId>
        info: z.ZodCodec<z.ZodOptional<z.ZodUnknown>, z.ZodUnknown>
      }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(
      object({ kind: z.literal('ipv4') }),
      object({ gateway: GatewayId }),
    ),
    z.intersection(
      object({ kind: z.literal('ipv6') }),
      object({ gateway: GatewayId, scopeId: z.int() }),
    ),
    z.intersection(
      object({ kind: z.literal('mdns') }),
      object({ gateways: z.array(GatewayId) }),
    ),
    z.intersection(
      object({ kind: z.literal('private-domain') }),
      object({ gateways: z.array(GatewayId) }),
    ),
    z.intersection(
      object({ kind: z.literal('public-domain') }),
      object({ gateway: GatewayId }),
    ),
    z.intersection(
      object({ kind: z.literal('plugin') }),
      object({
        packageId: PackageId,
        removeAction: wire(ActionId.nullable().optional(), ActionId.nullable()),
        overflowActions: z.array(ActionId),
        info: wire(z.unknown().optional(), z.unknown()),
      }),
    ),
  ])
  .meta({
    id: 'HostnameMetadata',
  })
export const InfoParams: z.ZodObject<{
  targetId: typeof BackupTargetId
  serverId: z.ZodString
  password: z.ZodString
}> = /* @__PURE__ */ object({
  targetId: BackupTargetId,
  serverId: z.string(),
  password: z.string(),
}).meta({
  id: 'InfoParams',
})
export const InitAcmeParams: z.ZodObject<{
  provider: typeof AcmeProvider
  contact: z.ZodArray<z.ZodString>
}> = /* @__PURE__ */ object({
  provider: AcmeProvider,
  contact: z.array(z.string()),
}).meta({
  id: 'InitAcmeParams',
})
export const InstalledVersionParams: z.ZodObject<{ id: typeof PackageId }> =
  /* @__PURE__ */ object({ id: PackageId }).meta({
    id: 'InstalledVersionParams',
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
export const LegacyAlpnInfo: z.ZodUnion<
  readonly [
    z.ZodLiteral<'reflect'>,
    z.ZodObject<{ specified: z.ZodArray<typeof MaybeUtf8String> }>,
  ]
> = /* @__PURE__ */ z
  .union([
    z.literal('reflect'),
    object({ specified: z.array(MaybeUtf8String) }),
  ])
  .meta({
    id: 'LegacyAlpnInfo',
  })
export const ListPackageSignersParams: z.ZodIntersection<
  z.ZodObject<{ id: typeof PackageId }>,
  typeof ReaderParams
> = /* @__PURE__ */ z
  .intersection(object({ id: PackageId }), ReaderParams)
  .meta({
    id: 'ListPackageSignersParams',
  })
export const LogFollowResponse: z.ZodObject<{
  startCursor: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Present for ordinary replies; typed observations use same-ID events.
   */
  guid: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Guid>>,
    z.ZodNullable<typeof Guid>
  >
}> = /* @__PURE__ */ object({
  startCursor: wire(z.string().nullable().optional(), z.string().nullable()),
  /**
   * Present for ordinary replies; typed observations use same-ID events.
   */
  guid: wire(Guid.nullable().optional(), Guid.nullable()).meta({
    description:
      'Present for ordinary replies; typed observations use same-ID events.',
  }),
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
export const LshwDevice: z.ZodUnion<
  readonly [
    z.ZodIntersection<
      z.ZodObject<{ class: z.ZodLiteral<'processor'> }>,
      typeof LshwProcessor
    >,
    z.ZodIntersection<
      z.ZodObject<{ class: z.ZodLiteral<'display'> }>,
      typeof LshwDisplay
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(object({ class: z.literal('processor') }), LshwProcessor),
    z.intersection(object({ class: z.literal('display') }), LshwDisplay),
  ])
  .meta({
    id: 'LshwDevice',
  })
export const MetricsCpu: z.ZodObject<{
  percentageUsed: typeof Percentage
  idle: typeof Percentage
  userSpace: typeof Percentage
  kernelSpace: typeof Percentage
  wait: typeof Percentage
}> = /* @__PURE__ */ object({
  percentageUsed: Percentage,
  idle: Percentage,
  userSpace: Percentage,
  kernelSpace: Percentage,
  wait: Percentage,
}).meta({
  id: 'MetricsCpu',
})
export const MetricsDisk: z.ZodObject<{
  percentageUsed: typeof Percentage
  used: typeof GigaBytes
  available: typeof GigaBytes
  capacity: typeof GigaBytes
}> = /* @__PURE__ */ object({
  percentageUsed: Percentage,
  used: GigaBytes,
  available: GigaBytes,
  capacity: GigaBytes,
}).meta({
  id: 'MetricsDisk',
})
export const MetricsGeneral: z.ZodObject<{
  temperature: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Celsius>>,
    z.ZodNullable<typeof Celsius>
  >
}> = /* @__PURE__ */ object({
  temperature: wire(Celsius.nullable().optional(), Celsius.nullable()),
}).meta({
  id: 'MetricsGeneral',
})
export const MetricsMemory: z.ZodObject<{
  percentageUsed: typeof Percentage
  total: typeof MebiBytes
  available: typeof MebiBytes
  used: typeof MebiBytes
  zramTotal: typeof MebiBytes
  zramAvailable: typeof MebiBytes
  zramUsed: typeof MebiBytes
}> = /* @__PURE__ */ object({
  percentageUsed: Percentage,
  total: MebiBytes,
  available: MebiBytes,
  used: MebiBytes,
  zramTotal: MebiBytes,
  zramAvailable: MebiBytes,
  zramUsed: MebiBytes,
}).meta({
  id: 'MetricsMemory',
})
export const MetricsSummary: z.ZodObject<{
  totalCheckins: z.ZodInt
  uniqueServers: z.ZodInt
  totalPackageRequests: z.ZodInt
  byArch: z.ZodArray<typeof CountEntry>
  byOsVersion: z.ZodArray<typeof CountEntry>
}> = /* @__PURE__ */ object({
  totalCheckins: z.int(),
  uniqueServers: z.int(),
  totalPackageRequests: z.int(),
  byArch: z.array(CountEntry),
  byOsVersion: z.array(CountEntry),
}).meta({
  id: 'MetricsSummary',
})
export const MountParams: z.ZodObject<{
  targetId: typeof BackupTargetId
  serverId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  password: z.ZodString
  allowPartial: z.ZodBoolean
}> = /* @__PURE__ */ object({
  targetId: BackupTargetId,
  serverId: wire(z.string().nullable().optional(), z.string().nullable()),
  password: z.string(),
  allowPartial: z.boolean(),
}).meta({
  id: 'MountParams',
})
export const Notification: z.ZodObject<{
  packageId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof PackageId>>,
    z.ZodNullable<typeof PackageId>
  >
  createdAt: z.ZodString
  code: z.ZodInt
  level: typeof NotificationLevel
  title: z.ZodString
  message: z.ZodString
  data: z.ZodUnknown
  seen: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  packageId: wire(PackageId.nullable().optional(), PackageId.nullable()),
  createdAt: z.string(),
  code: z.int(),
  level: NotificationLevel,
  title: z.string(),
  message: z.string(),
  data: z.unknown(),
  seen: wire(z.boolean().optional(), z.boolean()),
}).meta({
  id: 'Notification',
})

/**
 * Directly connected UPS server settings or remote NUT client settings.
 */
export const NutSettings: z.ZodUnion<
  readonly [
    z.ZodIntersection<
      z.ZodObject<{
        /**
         * Serve and monitor a UPS connected directly to this StartOS host.
         */
        mode: z.ZodLiteral<'server'>
      }>,
      z.ZodObject<{
        device: typeof NutDevice
        listenAll: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
        remoteUsername: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodString>>,
          z.ZodNullable<z.ZodString>
        >
        remotePassword: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodString>>,
          z.ZodNullable<z.ZodString>
        >
      }>
    >,
    z.ZodIntersection<
      z.ZodObject<{
        /**
         * Monitor a UPS exposed by another NUT server.
         */
        mode: z.ZodLiteral<'client'>
      }>,
      z.ZodObject<{
        upsName: z.ZodString
        host: z.ZodString
        port: z.ZodCodec<z.ZodOptional<z.ZodInt>, z.ZodInt>
        monitorUsername: z.ZodString
        monitorPassword: z.ZodString
      }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(
      object({
        /**
         * Serve and monitor a UPS connected directly to this StartOS host.
         */
        mode: z.literal('server').meta({
          description:
            'Serve and monitor a UPS connected directly to this StartOS host.',
        }),
      }),
      object({
        device: NutDevice,
        listenAll: wire(z.boolean().optional(), z.boolean()),
        remoteUsername: wire(
          z.string().nullable().optional(),
          z.string().nullable(),
        ),
        remotePassword: wire(
          z.string().nullable().optional(),
          z.string().nullable(),
        ),
      }),
    ),
    z.intersection(
      object({
        /**
         * Monitor a UPS exposed by another NUT server.
         */
        mode: z.literal('client').meta({
          description: 'Monitor a UPS exposed by another NUT server.',
        }),
      }),
      object({
        upsName: z.string(),
        host: z.string(),
        port: wire(z.int().optional(), z.int()),
        monitorUsername: z.string(),
        monitorPassword: z.string(),
      }),
    ),
  ])
  .meta({
    id: 'NutSettings',
    description:
      'Directly connected UPS server settings or remote NUT client settings.',
  })
export const OsInfo: z.ZodObject<{
  version: typeof Version
  compat: z.ZodString
  platform: z.ZodString
  language: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  version: Version,
  compat: z.string(),
  platform: z.string(),
  language: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'OsInfo',
})
export const OsVersionInfo: z.ZodObject<{
  headline: z.ZodString
  releaseNotes: typeof LocaleString
  sourceVersion: z.ZodString
  /**
   * Commit that produced the OS image, if recorded at publication.
   */
  gitHash: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof GitHash>>,
    z.ZodNullable<typeof GitHash>
  >
  /**
   * Whether the release includes security fixes.
   */
  security: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  authorized: z.ZodArray<typeof Guid>
  iso: z.ZodRecord<
    z.ZodString,
    ReturnType<typeof RegistryAsset<typeof Blake3Commitment>>
  >
  squashfs: z.ZodRecord<
    z.ZodString,
    ReturnType<typeof RegistryAsset<typeof Blake3Commitment>>
  >
  img: z.ZodRecord<
    z.ZodString,
    ReturnType<typeof RegistryAsset<typeof Blake3Commitment>>
  >
}> = /* @__PURE__ */ object({
  headline: z.string(),
  releaseNotes: LocaleString,
  sourceVersion: z.string(),
  /**
   * Commit that produced the OS image, if recorded at publication.
   */
  gitHash: wire(GitHash.nullable().optional(), GitHash.nullable()).meta({
    description:
      'Commit that produced the OS image, if recorded at publication.',
  }),
  /**
   * Whether the release includes security fixes.
   */
  security: wire(z.boolean().optional(), z.boolean()).meta({
    description: 'Whether the release includes security fixes.',
  }),
  authorized: z.array(Guid),
  iso: z.record(z.string(), RegistryAsset(Blake3Commitment)),
  squashfs: z.record(z.string(), RegistryAsset(Blake3Commitment)),
  img: z.record(z.string(), RegistryAsset(Blake3Commitment)),
}).meta({
  id: 'OsVersionInfo',
})
export const PackageAccessControl: z.ZodObject<{
  private: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  readers: z.ZodCodec<
    z.ZodOptional<z.ZodArray<typeof Guid>>,
    z.ZodArray<typeof Guid>
  >
}> = /* @__PURE__ */ object({
  private: wire(z.boolean().optional(), z.boolean()),
  readers: wire(z.array(Guid).optional(), z.array(Guid)),
}).meta({
  id: 'PackageAccessControl',
})
export const PackageBackupInfo: z.ZodObject<{
  title: z.ZodString
  version: typeof Version
  osVersion: z.ZodString
  timestamp: z.ZodString
}> = /* @__PURE__ */ object({
  title: z.string(),
  version: Version,
  osVersion: z.string(),
  timestamp: z.string(),
}).meta({
  id: 'PackageBackupInfo',
})

/**
 * Inline for StartOS before 0.4.0.3 and from registries before 1.2.0.
 */
export const PackageIcon: z.ZodUnion<readonly [typeof DataUrl, z.ZodString]> =
  /* @__PURE__ */ z
    .union([
      DataUrl,
      z.string().meta({
        description:
          "A path under the registry's `/icons/`, led by the scheme that names the file.",
      }),
    ])
    .meta({
      id: 'PackageIcon',
      description:
        'Inline for StartOS before 0.4.0.3 and from registries before 1.2.0.',
    })
export const PackageIdParams: z.ZodObject<{ id: typeof PackageId }> =
  /* @__PURE__ */ object({ id: PackageId }).meta({
    id: 'PackageIdParams',
  })
export const PackageInfoShort: z.ZodObject<{
  releaseNotes: typeof LocaleString
}> = /* @__PURE__ */ object({ releaseNotes: LocaleString }).meta({
  id: 'PackageInfoShort',
})
export const PackageReaderParams: z.ZodObject<{
  id: typeof PackageId
  signer: typeof Guid
}> = /* @__PURE__ */ object({ id: PackageId, signer: Guid }).meta({
  id: 'PackageReaderParams',
})
export const PassthroughInfo: z.ZodObject<{
  hostname: z.ZodString
  listenPort: z.ZodInt
  backend: z.ZodString
  publicGateways: z.ZodArray<typeof GatewayId>
  privateIps: z.ZodArray<z.ZodString>
}> = /* @__PURE__ */ object({
  hostname: z.string(),
  listenPort: z.int(),
  backend: z.string(),
  publicGateways: z.array(GatewayId),
  privateIps: z.array(z.string()),
}).meta({
  id: 'PassthroughInfo',
})
export const PasswordType: z.ZodUnion<
  readonly [typeof EncryptedWire, z.ZodString]
> = /* @__PURE__ */ z.union([EncryptedWire, z.string()]).meta({
  id: 'PasswordType',
})

/**
 * JSON Patch single patch operation
 */
export const PatchOperation: z.ZodUnion<
  readonly [
    z.ZodIntersection<
      z.ZodObject<{
        /**
         * 'add' operation
         */
        op: z.ZodLiteral<'add'>
      }>,
      typeof AddOperation
    >,
    z.ZodIntersection<
      z.ZodObject<{
        /**
         * 'remove' operation
         */
        op: z.ZodLiteral<'remove'>
      }>,
      typeof RemoveOperation
    >,
    z.ZodIntersection<
      z.ZodObject<{
        /**
         * 'replace' operation
         */
        op: z.ZodLiteral<'replace'>
      }>,
      typeof ReplaceOperation
    >,
    z.ZodIntersection<
      z.ZodObject<{
        /**
         * 'move' operation
         */
        op: z.ZodLiteral<'move'>
      }>,
      typeof MoveOperation
    >,
    z.ZodIntersection<
      z.ZodObject<{
        /**
         * 'copy' operation
         */
        op: z.ZodLiteral<'copy'>
      }>,
      typeof CopyOperation
    >,
    z.ZodIntersection<
      z.ZodObject<{
        /**
         * 'test' operation
         */
        op: z.ZodLiteral<'test'>
      }>,
      typeof TestOperation
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(
      object({
        /**
         * 'add' operation
         */
        op: z.literal('add').meta({
          description: "'add' operation",
        }),
      }),
      AddOperation,
    ),
    z.intersection(
      object({
        /**
         * 'remove' operation
         */
        op: z.literal('remove').meta({
          description: "'remove' operation",
        }),
      }),
      RemoveOperation,
    ),
    z.intersection(
      object({
        /**
         * 'replace' operation
         */
        op: z.literal('replace').meta({
          description: "'replace' operation",
        }),
      }),
      ReplaceOperation,
    ),
    z.intersection(
      object({
        /**
         * 'move' operation
         */
        op: z.literal('move').meta({
          description: "'move' operation",
        }),
      }),
      MoveOperation,
    ),
    z.intersection(
      object({
        /**
         * 'copy' operation
         */
        op: z.literal('copy').meta({
          description: "'copy' operation",
        }),
      }),
      CopyOperation,
    ),
    z.intersection(
      object({
        /**
         * 'test' operation
         */
        op: z.literal('test').meta({
          description: "'test' operation",
        }),
      }),
      TestOperation,
    ),
  ])
  .meta({
    id: 'PatchOperation',
    description: 'JSON Patch single patch operation',
  })
export const PreDownloadAlert: z.ZodObject<{
  message: typeof LocaleString
  when: typeof PreDownloadAlertWhen
}> = /* @__PURE__ */ object({
  message: LocaleString,
  when: PreDownloadAlertWhen,
}).meta({
  id: 'PreDownloadAlert',
})

/**
 * Auth gate enforced by the OS reverse proxy on incoming requests.
 *
 * - `Bearer { tokens }`: any of `tokens` is accepted as `Authorization: Bearer <token>`.
 * - `Basic  { credentials }`: any `(username, password)` pair in `credentials` is
 *   accepted as `Authorization: Basic <base64(username:password)>`. The matched
 *   `username` is forwarded upstream as `X-Forwarded-User`.
 *
 * `realm` is the authentication realm advertised in the
 * `WWW-Authenticate` challenge sent on 401 responses (RFC 7235
 * §2.2). Defaults to `"StartOS"` when unset. Packages that share
 * credentials across multiple bindings should pick a stable realm
 * so that browsers reuse cached credentials across them.
 */
export const ProxyAuth: z.ZodUnion<
  readonly [
    z.ZodIntersection<
      z.ZodObject<{ type: z.ZodLiteral<'bearer'> }>,
      z.ZodObject<{
        tokens: z.ZodArray<z.ZodString>
        realm: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodString>>,
          z.ZodNullable<z.ZodString>
        >
      }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ type: z.ZodLiteral<'basic'> }>,
      z.ZodObject<{
        credentials: z.ZodArray<typeof BasicCredential>
        realm: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodString>>,
          z.ZodNullable<z.ZodString>
        >
      }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(
      object({ type: z.literal('bearer') }),
      object({
        tokens: z.array(z.string()),
        realm: wire(z.string().nullable().optional(), z.string().nullable()),
      }),
    ),
    z.intersection(
      object({ type: z.literal('basic') }),
      object({
        credentials: z.array(BasicCredential),
        realm: wire(z.string().nullable().optional(), z.string().nullable()),
      }),
    ),
  ])
  .meta({
    id: 'ProxyAuth',
    description:
      'Auth gate enforced by the OS reverse proxy on incoming requests.\n\n- `Bearer { tokens }`: any of `tokens` is accepted as `Authorization: Bearer <token>`.\n- `Basic  { credentials }`: any `(username, password)` pair in `credentials` is\n  accepted as `Authorization: Basic <base64(username:password)>`. The matched\n  `username` is forwarded upstream as `X-Forwarded-User`.\n\n`realm` is the authentication realm advertised in the\n`WWW-Authenticate` challenge sent on 401 responses (RFC 7235\n§2.2). Defaults to `"StartOS"` when unset. Packages that share\ncredentials across multiple bindings should pick a stable realm\nso that browsers reuse cached credentials across them.',
  })
export const PublicDomainConfig: z.ZodObject<{
  gateway: typeof GatewayId
  acme: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof AcmeProvider>>,
    z.ZodNullable<typeof AcmeProvider>
  >
}> = /* @__PURE__ */ object({
  gateway: GatewayId,
  acme: wire(AcmeProvider.nullable().optional(), AcmeProvider.nullable()),
}).meta({
  id: 'PublicDomainConfig',
})
export const PushSubscription: z.ZodObject<{
  /**
   * HTTP(S) push resource selected by the browser, including LAN endpoints.
   */
  endpoint: z.ZodString
  keys: typeof PushKeys
}> = /* @__PURE__ */ object({
  /**
   * HTTP(S) push resource selected by the browser, including LAN endpoints.
   */
  endpoint: z.string().meta({
    description:
      'HTTP(S) push resource selected by the browser, including LAN endpoints.',
  }),
  keys: PushKeys,
}).meta({
  id: 'PushSubscription',
})
export const RebuildParams: z.ZodObject<{ id: typeof PackageId }> =
  /* @__PURE__ */ object({ id: PackageId }).meta({
    id: 'RebuildParams',
  })
export const RegistryIdentity: z.ZodObject<{
  name: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  icon: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof DataUrl>>,
    z.ZodNullable<typeof DataUrl>
  >
}> = /* @__PURE__ */ object({
  name: wire(z.string().nullable().optional(), z.string().nullable()),
  icon: wire(DataUrl.nullable().optional(), DataUrl.nullable()),
}).meta({
  id: 'RegistryIdentity',
})
export const RemoveAcmeParams: z.ZodObject<{ provider: typeof AcmeProvider }> =
  /* @__PURE__ */ object({ provider: AcmeProvider }).meta({
    id: 'RemoveAcmeParams',
  })
export const RemoveAdminParams: z.ZodObject<{ signer: typeof Guid }> =
  /* @__PURE__ */ object({ signer: Guid }).meta({
    id: 'RemoveAdminParams',
  })
export const RemoveMirrorParams: z.ZodObject<{
  id: typeof PackageId
  version: typeof Version
  url: z.ZodString
}> = /* @__PURE__ */ object({
  id: PackageId,
  version: Version,
  url: z.string(),
}).meta({
  id: 'RemoveMirrorParams',
})
export const RemovePackageFromCategoryParams: z.ZodObject<{
  id: z.ZodString
  package: typeof PackageId
}> = /* @__PURE__ */ object({ id: z.string(), package: PackageId }).meta({
  id: 'RemovePackageFromCategoryParams',
})
export const RemovePackageParams: z.ZodObject<{
  id: typeof PackageId
  version: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Version>>,
    z.ZodNullable<typeof Version>
  >
  sighash: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof literal_0>>,
    z.ZodNullable<typeof literal_0>
  >
  force: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  id: PackageId,
  version: wire(Version.nullable().optional(), Version.nullable()),
  sighash: wire(literal_0.nullable().optional(), literal_0.nullable()),
  force: wire(z.boolean().optional(), z.boolean()),
}).meta({
  id: 'RemovePackageParams',
})
export const RemovePackageSignerParams: z.ZodObject<{
  id: typeof PackageId
  signer: typeof Guid
}> = /* @__PURE__ */ object({ id: PackageId, signer: Guid }).meta({
  id: 'RemovePackageSignerParams',
})
export const RemoveSignerParams: z.ZodObject<{ id: typeof Guid }> =
  /* @__PURE__ */ object({ id: Guid }).meta({
    id: 'RemoveSignerParams',
  })
export const RemoveTunnelParams: z.ZodObject<{ id: typeof GatewayId }> =
  /* @__PURE__ */ object({ id: GatewayId }).meta({
    id: 'RemoveTunnelParams',
  })
export const RenameGatewayParams: z.ZodObject<{
  id: typeof GatewayId
  name: z.ZodString
}> = /* @__PURE__ */ object({ id: GatewayId, name: z.string() }).meta({
  id: 'RenameGatewayParams',
})
export const RenameSharedVolumeParams: z.ZodObject<{
  id: typeof SharedVolumeId
  name: z.ZodString
}> = /* @__PURE__ */ object({ id: SharedVolumeId, name: z.string() }).meta({
  id: 'RenameSharedVolumeParams',
})
export const RenameTargetParams: z.ZodObject<{
  id: typeof BackupTargetId
  name: z.ZodString
}> = /* @__PURE__ */ object({ id: BackupTargetId, name: z.string() }).meta({
  id: 'RenameTargetParams',
})
export const RequiresHostId: z.ZodObject<{ host: typeof HostId }> =
  /* @__PURE__ */ object({ host: HostId }).meta({
    id: 'RequiresHostId',
  })
export const RequiresPackageId: z.ZodObject<{ package: typeof PackageId }> =
  /* @__PURE__ */ object({ package: PackageId }).meta({
    id: 'RequiresPackageId',
  })
export const RestorePackageParams: z.ZodObject<{
  targetId: typeof BackupTargetId
  password: z.ZodString
  ids: z.ZodArray<typeof PackageId>
  serverId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  targetId: BackupTargetId,
  password: z.string(),
  ids: z.array(PackageId),
  serverId: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'RestorePackageParams',
})
export const RestoreVolumeParams: z.ZodObject<{
  targetId: typeof BackupTargetId
  password: z.ZodString
  ids: z.ZodArray<typeof SharedVolumeId>
  serverId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  targetId: BackupTargetId,
  password: z.string(),
  ids: z.array(SharedVolumeId),
  serverId: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'RestoreVolumeParams',
})
export const RestoreVolumeResult: z.ZodObject<{
  restored: z.ZodRecord<z.ZodString, typeof SharedVolumeId>
  errors: z.ZodRecord<z.ZodString, z.ZodString>
}> = /* @__PURE__ */ object({
  restored: z.record(z.string(), SharedVolumeId),
  errors: z.record(z.string(), z.string()),
}).meta({
  id: 'RestoreVolumeResult',
})
export const RunActionParams: z.ZodObject<{
  packageId: typeof PackageId
  eventId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Guid>>,
    z.ZodNullable<typeof Guid>
  >
  actionId: typeof ActionId
  input: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodUnknown>>,
    z.ZodNullable<z.ZodUnknown>
  >
}> = /* @__PURE__ */ object({
  packageId: PackageId,
  eventId: wire(Guid.nullable().optional(), Guid.nullable()),
  actionId: ActionId,
  input: wire(z.unknown().nullable().optional(), z.unknown().nullable()),
}).meta({
  id: 'RunActionParams',
})
export const ServiceStats: z.ZodObject<{
  container_id: typeof ContainerId
  memory_usage: typeof MiB
  memory_limit: typeof MiB
}> = /* @__PURE__ */ object({
  container_id: ContainerId,
  memory_usage: MiB,
  memory_limit: MiB,
}).meta({
  id: 'ServiceStats',
})
export const SetDefaultOutboundParams: z.ZodObject<{
  gateway: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof GatewayId>>,
    z.ZodNullable<typeof GatewayId>
  >
  dns: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof OutboundDnsUpdate>>,
    z.ZodNullable<typeof OutboundDnsUpdate>
  >
}> = /* @__PURE__ */ object({
  gateway: wire(GatewayId.nullable().optional(), GatewayId.nullable()),
  dns: wire(
    OutboundDnsUpdate.nullable().optional(),
    OutboundDnsUpdate.nullable(),
  ),
}).meta({
  id: 'SetDefaultOutboundParams',
})
export const SetDescriptionParams: z.ZodObject<{
  description: z.ZodNullable<typeof LocaleString>
}> = /* @__PURE__ */ object({ description: LocaleString.nullable() }).meta({
  id: 'SetDescriptionParams',
})
export const SetGatewaySecureParams: z.ZodObject<{
  gateway: typeof GatewayId
  secure: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodBoolean>>,
    z.ZodNullable<z.ZodBoolean>
  >
}> = /* @__PURE__ */ object({
  gateway: GatewayId,
  secure: wire(z.boolean().nullable().optional(), z.boolean().nullable()),
}).meta({
  id: 'SetGatewaySecureParams',
})
export const SetIconParams: z.ZodObject<{
  icon: z.ZodNullable<typeof DataUrl>
}> = /* @__PURE__ */ object({ icon: DataUrl.nullable() }).meta({
  id: 'SetIconParams',
})

/**
 * Sets verbosity for StartOS backend code in the running daemon.
 */
export const SetLogLevelParams: z.ZodObject<{
  /**
   * Applies to start-core/start-os until the daemon restarts.
   */
  level: typeof LogLevel
}> = /* @__PURE__ */ object({
  /**
   * Applies to start-core/start-os until the daemon restarts.
   */
  level: LogLevel.meta({
    description: 'Applies to start-core/start-os until the daemon restarts.',
  }),
}).meta({
  id: 'SetLogLevelParams',
  description: 'Sets verbosity for StartOS backend code in the running daemon.',
})
export const SetOutboundGatewayParams: z.ZodObject<{
  package: typeof PackageId
  gateway: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof GatewayId>>,
    z.ZodNullable<typeof GatewayId>
  >
  dns: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof OutboundDnsUpdate>>,
    z.ZodNullable<typeof OutboundDnsUpdate>
  >
}> = /* @__PURE__ */ object({
  package: PackageId,
  gateway: wire(GatewayId.nullable().optional(), GatewayId.nullable()),
  dns: wire(
    OutboundDnsUpdate.nullable().optional(),
    OutboundDnsUpdate.nullable(),
  ),
}).meta({
  id: 'SetOutboundGatewayParams',
})
export const SetPackagePrivateParams: z.ZodObject<{
  id: typeof PackageId
  private: z.ZodBoolean
}> = /* @__PURE__ */ object({ id: PackageId, private: z.boolean() }).meta({
  id: 'SetPackagePrivateParams',
})
export const SetSharedVolumeGrantParams: z.ZodObject<{
  id: typeof SharedVolumeId
  packageId: typeof PackageId
  grant: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof SharedVolumeGrant>>,
    z.ZodNullable<typeof SharedVolumeGrant>
  >
}> = /* @__PURE__ */ object({
  id: SharedVolumeId,
  packageId: PackageId,
  grant: wire(
    SharedVolumeGrant.nullable().optional(),
    SharedVolumeGrant.nullable(),
  ),
}).meta({
  id: 'SetSharedVolumeGrantParams',
})
export const SetStaticDnsParams: z.ZodObject<{
  gateway: typeof GatewayId
  servers: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<z.ZodString>>>,
    z.ZodNullable<z.ZodArray<z.ZodString>>
  >
}> = /* @__PURE__ */ object({
  gateway: GatewayId,
  servers: wire(
    z.array(z.string()).nullable().optional(),
    z.array(z.string()).nullable(),
  ),
}).meta({
  id: 'SetStaticDnsParams',
})
export const SetTrustSignersParams: z.ZodObject<{
  signers: z.ZodArray<typeof Guid>
}> = /* @__PURE__ */ object({ signers: z.array(Guid) }).meta({
  id: 'SetTrustSignersParams',
})
export const SharedVolumeInspection: z.ZodObject<{
  availability: z.ZodCodec<
    z.ZodOptional<typeof SharedVolumeAvailability>,
    typeof SharedVolumeAvailability
  >
  available: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  error: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  checkedAt: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  availability: wire(
    SharedVolumeAvailability.optional(),
    SharedVolumeAvailability,
  ),
  available: wire(z.int().nullable().optional(), z.int().nullable()),
  error: wire(z.string().nullable().optional(), z.string().nullable()),
  checkedAt: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'SharedVolumeInspection',
})
export const SharedVolumeParams: z.ZodObject<{ id: typeof SharedVolumeId }> =
  /* @__PURE__ */ object({ id: SharedVolumeId }).meta({
    id: 'SharedVolumeParams',
  })
export const SharedVolumeSource: z.ZodUnion<
  readonly [
    z.ZodObject<{
      /**
       * Managed Btrfs subvolume on the encrypted data partition.
       */
      kind: z.ZodLiteral<'local'>
    }>,
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'disk'> }>,
      z.ZodObject<{
        logicalname: z.ZodString
        path: z.ZodCodec<z.ZodOptional<z.ZodString>, z.ZodString>
        /**
         * The drive as reported when the source was created, for display.
         */
        drive: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<typeof SharedVolumeDrive>>,
          z.ZodNullable<typeof SharedVolumeDrive>
        >
      }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'cifs'> }>,
      z.ZodObject<{
        hostname: z.ZodString
        path: z.ZodString
        username: z.ZodCodec<z.ZodOptional<z.ZodString>, z.ZodString>
      }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    object({
      /**
       * Managed Btrfs subvolume on the encrypted data partition.
       */
      kind: z.literal('local').meta({
        description: 'Managed Btrfs subvolume on the encrypted data partition.',
      }),
    }),
    z.intersection(
      object({ kind: z.literal('disk') }),
      object({
        logicalname: z.string(),
        path: wire(z.string().optional(), z.string()),
        /**
         * The drive as reported when the source was created, for display.
         */
        drive: wire(
          SharedVolumeDrive.nullable().optional(),
          SharedVolumeDrive.nullable(),
        ).meta({
          description:
            'The drive as reported when the source was created, for display.',
        }),
      }),
    ),
    z.intersection(
      object({ kind: z.literal('cifs') }),
      object({
        hostname: z.string(),
        path: z.string(),
        username: wire(z.string().optional(), z.string()),
      }),
    ),
  ])
  .meta({
    id: 'SharedVolumeSource',
  })
export const SideloadResponse: z.ZodObject<{
  upload: typeof Guid
  /**
   * Legacy progress continuation, absent for rpcstream observers.
   */
  progress: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Guid>>,
    z.ZodNullable<typeof Guid>
  >
}> = /* @__PURE__ */ object({
  upload: Guid,
  /**
   * Legacy progress continuation, absent for rpcstream observers.
   */
  progress: wire(Guid.nullable().optional(), Guid.nullable()).meta({
    description:
      'Legacy progress continuation, absent for rpcstream observers.',
  }),
}).meta({
  id: 'SideloadResponse',
})
export const SignAssetParams: z.ZodObject<{
  version: z.ZodString
  platform: z.ZodString
  signature: typeof AnySignature
}> = /* @__PURE__ */ object({
  version: z.string(),
  platform: z.string(),
  signature: AnySignature,
}).meta({
  id: 'SignAssetParams',
})
export const SignPackageParams: z.ZodObject<{
  id: typeof PackageId
  version: typeof Version
  commitment: typeof MerkleArchiveCommitment
  signature: typeof AnySignature
}> = /* @__PURE__ */ object({
  id: PackageId,
  version: Version,
  commitment: MerkleArchiveCommitment,
  signature: AnySignature,
}).meta({
  id: 'SignPackageParams',
})
export const SignerInfo: z.ZodObject<{
  name: z.ZodString
  contact: z.ZodArray<typeof ContactInfo>
  keys: z.ZodArray<typeof AnyVerifyingKey>
  gpgExports: z.ZodCodec<
    z.ZodOptional<z.ZodArray<typeof GpgPublicKeyExport>>,
    z.ZodArray<typeof GpgPublicKeyExport>
  >
}> = /* @__PURE__ */ object({
  name: z.string(),
  contact: z.array(ContactInfo),
  keys: z.array(AnyVerifyingKey),
  gpgExports: wire(
    z.array(GpgPublicKeyExport).optional(),
    z.array(GpgPublicKeyExport),
  ),
}).meta({
  id: 'SignerInfo',
})
export const SmtpValue: z.ZodCodec<
  z.ZodIntersection<
    z.ZodIntersection<
      z.ZodObject<{
        host: z.ZodOptional<z.ZodString>
        server: z.ZodOptional<z.ZodString>
        port: z.ZodInt
        from: z.ZodString
        login: z.ZodOptional<z.ZodString>
        username: z.ZodOptional<z.ZodString>
        password: z.ZodOptional<z.ZodNullable<z.ZodString>>
        security: z.ZodOptional<typeof SmtpSecurity>
      }>,
      z.ZodUnion<
        readonly [
          z.ZodObject<{ host: z.ZodString }>,
          z.ZodObject<{ server: z.ZodString }>,
        ]
      >
    >,
    z.ZodUnion<
      readonly [
        z.ZodObject<{ login: z.ZodString }>,
        z.ZodObject<{ username: z.ZodString }>,
      ]
    >
  >,
  z.ZodObject<{
    host: z.ZodString
    port: z.ZodInt
    from: z.ZodString
    username: z.ZodString
    password: z.ZodNullable<z.ZodString>
    security: typeof SmtpSecurity
  }>
> = /* @__PURE__ */ wire(
  z.intersection(
    z.intersection(
      object({
        host: z.string().optional(),
        server: z.string().optional(),
        port: z.int(),
        from: z.string(),
        login: z.string().optional(),
        username: z.string().optional(),
        password: z.string().nullable().optional(),
        security: SmtpSecurity.optional(),
      }),
      z.union([object({ host: z.string() }), object({ server: z.string() })]),
    ),
    z.union([object({ login: z.string() }), object({ username: z.string() })]),
  ),
  object({
    host: z.string(),
    port: z.int(),
    from: z.string(),
    username: z.string(),
    password: z.string().nullable(),
    security: SmtpSecurity,
  }),
).meta({
  id: 'SmtpValue',
})
export const SshAddParams: z.ZodObject<{ key: typeof SshPubKey }> =
  /* @__PURE__ */ object({ key: SshPubKey }).meta({
    id: 'SshAddParams',
  })

/**
 * The public view of a backup found on a target: enough to identify it, and none of
 * [`BackupUnencryptedMetadata`]'s key material.
 */
export const StartOsRecoveryInfo: z.ZodObject<{
  hostname: typeof ServerHostname
  version: z.ZodString
  timestamp: z.ZodString
  formatStatus: z.ZodCodec<
    z.ZodOptional<typeof FormatStatus>,
    typeof FormatStatus
  >
}> = /* @__PURE__ */ object({
  hostname: ServerHostname,
  version: z.string(),
  timestamp: z.string(),
  formatStatus: wire(FormatStatus.optional(), FormatStatus),
}).meta({
  id: 'StartOsRecoveryInfo',
  description:
    "The public view of a backup found on a target: enough to identify it, and none of\n[`BackupUnencryptedMetadata`]'s key material.",
})
export const SubscribeIndexParams: z.ZodIntersection<
  z.ZodObject<{
    pointer: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<z.ZodString>>,
      z.ZodNullable<z.ZodString>
    >
    /**
     * Client StartOS version. Omission preserves the canonical index format.
     */
    osVersion: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<z.ZodString>>,
      z.ZodNullable<z.ZodString>
    >
  }>,
  typeof ReaderParams
> = /* @__PURE__ */ z
  .intersection(
    object({
      pointer: wire(z.string().nullable().optional(), z.string().nullable()),
      /**
       * Client StartOS version. Omission preserves the canonical index format.
       */
      osVersion: wire(
        z.string().nullable().optional(),
        z.string().nullable(),
      ).meta({
        description:
          'Client StartOS version. Omission preserves the canonical index format.',
      }),
    }),
    ReaderParams,
  )
  .meta({
    id: 'SubscribeIndexParams',
  })
export const SubscribeRes: z.ZodObject<{
  dump: typeof Dump
  guid: typeof Guid
}> = /* @__PURE__ */ object({ dump: Dump, guid: Guid }).meta({
  id: 'SubscribeRes',
})
export const TargetParams: z.ZodObject<{ id: typeof BackupTargetId }> =
  /* @__PURE__ */ object({ id: BackupTargetId }).meta({
    id: 'TargetParams',
  })
export const TestSmtpParams: z.ZodObject<{
  host: z.ZodString
  port: z.ZodInt
  from: z.ZodString
  to: z.ZodString
  username: z.ZodString
  password: z.ZodString
  security: z.ZodCodec<z.ZodOptional<typeof SmtpSecurity>, typeof SmtpSecurity>
}> = /* @__PURE__ */ object({
  host: z.string(),
  port: z.int(),
  from: z.string(),
  to: z.string(),
  username: z.string(),
  password: z.string(),
  security: wire(SmtpSecurity.optional(), SmtpSecurity),
}).meta({
  id: 'TestSmtpParams',
})
export const UmountParams: z.ZodObject<{
  targetId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof BackupTargetId>>,
    z.ZodNullable<typeof BackupTargetId>
  >
}> = /* @__PURE__ */ object({
  targetId: wire(
    BackupTargetId.nullable().optional(),
    BackupTargetId.nullable(),
  ),
}).meta({
  id: 'UmountParams',
})
export const UninstallParams: z.ZodObject<{
  id: typeof PackageId
  soft: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  force: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  id: PackageId,
  soft: wire(z.boolean().optional(), z.boolean()),
  force: wire(z.boolean().optional(), z.boolean()),
}).meta({
  id: 'UninstallParams',
})
export const UnsetGatewaySecureParams: z.ZodObject<{
  gateway: typeof GatewayId
}> = /* @__PURE__ */ object({ gateway: GatewayId }).meta({
  id: 'UnsetGatewaySecureParams',
})
export const UpdateCifsSharedVolumeParams: z.ZodObject<{
  id: typeof SharedVolumeId
  hostname: z.ZodString
  path: z.ZodString
  username: z.ZodString
  /**
   * Omitted keeps the stored password; empty clears it.
   */
  password: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  id: SharedVolumeId,
  hostname: z.string(),
  path: z.string(),
  username: z.string(),
  /**
   * Omitted keeps the stored password; empty clears it.
   */
  password: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description: 'Omitted keeps the stored password; empty clears it.',
  }),
}).meta({
  id: 'UpdateCifsSharedVolumeParams',
})
export const UpdateCifsTargetParams: z.ZodObject<{
  id: typeof BackupTargetId
  hostname: z.ZodString
  path: z.ZodString
  username: z.ZodString
  /**
   * Omitted keeps the stored password; empty clears it.
   */
  password: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  id: BackupTargetId,
  hostname: z.string(),
  path: z.string(),
  username: z.string(),
  /**
   * Omitted keeps the stored password; empty clears it.
   */
  password: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description: 'Omitted keeps the stored password; empty clears it.',
  }),
}).meta({
  id: 'UpdateCifsTargetParams',
})
export const UpdateSystemRes: z.ZodObject<{
  target: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Legacy progress continuation, absent for rpcstream observers.
   */
  progress: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Guid>>,
    z.ZodNullable<typeof Guid>
  >
}> = /* @__PURE__ */ object({
  target: wire(z.string().nullable().optional(), z.string().nullable()),
  /**
   * Legacy progress continuation, absent for rpcstream observers.
   */
  progress: wire(Guid.nullable().optional(), Guid.nullable()).meta({
    description:
      'Legacy progress continuation, absent for rpcstream observers.',
  }),
}).meta({
  id: 'UpdateSystemRes',
})
export const UpdateTunnelParams: z.ZodObject<{
  id: typeof GatewayId
  config: z.ZodString
}> = /* @__PURE__ */ object({ id: GatewayId, config: z.string() }).meta({
  id: 'UpdateTunnelParams',
})
export const VersionSignerParams: z.ZodObject<{
  version: z.ZodString
  signer: typeof Guid
}> = /* @__PURE__ */ object({ version: z.string(), signer: Guid }).meta({
  id: 'VersionSignerParams',
})
export const VolumeBackupReport: z.ZodIntersection<
  z.ZodObject<{ name: z.ZodString }>,
  typeof PackageBackupReport
> = /* @__PURE__ */ z
  .intersection(object({ name: z.string() }), PackageBackupReport)
  .meta({
    id: 'VolumeBackupReport',
  })
export const WifiListOut: z.ZodObject<{
  ssid: typeof Ssid
  strength: typeof SignalStrength
  security: z.ZodArray<z.ZodString>
}> = /* @__PURE__ */ object({
  ssid: Ssid,
  strength: SignalStrength,
  security: z.array(z.string()),
}).meta({
  id: 'WifiListOut',
})
export const ActionResultMember: z.ZodIntersection<
  z.ZodObject<{
    /**
     * A human-readable name or title of the value, such as "Last Active" or "Login Password"
     */
    name: z.ZodString
    /**
     * (optional) A description of the value, such as an explaining why it exists or how to use it
     */
    description: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<z.ZodString>>,
      z.ZodNullable<z.ZodString>
    >
  }>,
  typeof ActionResultValue
> = /* @__PURE__ */ z
  .intersection(
    object({
      /**
       * A human-readable name or title of the value, such as "Last Active" or "Login Password"
       */
      name: z.string().meta({
        description:
          'A human-readable name or title of the value, such as "Last Active" or "Login Password"',
      }),
      /**
       * (optional) A description of the value, such as an explaining why it exists or how to use it
       */
      description: wire(
        z.string().nullable().optional(),
        z.string().nullable(),
      ).meta({
        description:
          '(optional) A description of the value, such as an explaining why it exists or how to use it',
      }),
    }),
    ActionResultValue,
  )
  .meta({
    id: 'ActionResultMember',
  })
export const ActionResultV1: z.ZodObject<{
  /**
   * Primary text to display as the header of the response modal. e.g. "Success!", "Name Updated", or "Service Information", whatever makes sense
   */
  title: z.ZodString
  /**
   * (optional) A general message for the user, just under the title. Rendered as Markdown, with a single line break kept as a line break.
   */
  message: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * (optional) Structured data to present inside the modal
   */
  result: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof ActionResultValue>>,
    z.ZodNullable<typeof ActionResultValue>
  >
}> = /* @__PURE__ */ object({
  /**
   * Primary text to display as the header of the response modal. e.g. "Success!", "Name Updated", or "Service Information", whatever makes sense
   */
  title: z.string().meta({
    description:
      'Primary text to display as the header of the response modal. e.g. "Success!", "Name Updated", or "Service Information", whatever makes sense',
  }),
  /**
   * (optional) A general message for the user, just under the title. Rendered as Markdown, with a single line break kept as a line break.
   */
  message: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      '(optional) A general message for the user, just under the title. Rendered as Markdown, with a single line break kept as a line break.',
  }),
  /**
   * (optional) Structured data to present inside the modal
   */
  result: wire(
    ActionResultValue.nullable().optional(),
    ActionResultValue.nullable(),
  ).meta({
    description: '(optional) Structured data to present inside the modal',
  }),
}).meta({
  id: 'ActionResultV1',
})
export const AddPublicDomainRes: z.ZodObject<{
  dns: typeof QueryDnsRes
  port: typeof CheckPortRes
  portV6: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof CheckPortV6Res>>,
    z.ZodNullable<typeof CheckPortV6Res>
  >
  /**
   * The authority's own reachability requirement, where the domain has one.
   */
  challenge: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof CheckChallengeRes>>,
    z.ZodNullable<typeof CheckChallengeRes>
  >
}> = /* @__PURE__ */ object({
  dns: QueryDnsRes,
  port: CheckPortRes,
  portV6: wire(CheckPortV6Res.nullable().optional(), CheckPortV6Res.nullable()),
  /**
   * The authority's own reachability requirement, where the domain has one.
   */
  challenge: wire(
    CheckChallengeRes.nullable().optional(),
    CheckChallengeRes.nullable(),
  ).meta({
    description:
      "The authority's own reachability requirement, where the domain has one.",
  }),
}).meta({
  id: 'AddPublicDomainRes',
})
export const BackupInfo: z.ZodObject<{
  version: z.ZodString
  timestamp: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  packageBackups: z.ZodRecord<z.ZodString, typeof PackageBackupInfo>
  sharedVolumeBackups: z.ZodCodec<
    z.ZodOptional<z.ZodRecord<z.ZodString, typeof SharedVolumeBackupInfo>>,
    z.ZodRecord<z.ZodString, typeof SharedVolumeBackupInfo>
  >
}> = /* @__PURE__ */ object({
  version: z.string(),
  timestamp: wire(z.string().nullable().optional(), z.string().nullable()),
  packageBackups: z.record(z.string(), PackageBackupInfo),
  sharedVolumeBackups: wire(
    z.record(z.string(), SharedVolumeBackupInfo).optional(),
    z.record(z.string(), SharedVolumeBackupInfo),
  ),
}).meta({
  id: 'BackupInfo',
})
export const BackupParams: z.ZodObject<{
  targetId: typeof BackupTargetId
  oldPassword: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof PasswordType>>,
    z.ZodNullable<typeof PasswordType>
  >
  packageIds: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<typeof PackageId>>>,
    z.ZodNullable<z.ZodArray<typeof PackageId>>
  >
  sharedVolumeIds: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<typeof SharedVolumeId>>>,
    z.ZodNullable<z.ZodArray<typeof SharedVolumeId>>
  >
  password: typeof PasswordType
}> = /* @__PURE__ */ object({
  targetId: BackupTargetId,
  oldPassword: wire(
    PasswordType.nullable().optional(),
    PasswordType.nullable(),
  ),
  packageIds: wire(
    z.array(PackageId).nullable().optional(),
    z.array(PackageId).nullable(),
  ),
  sharedVolumeIds: wire(
    z.array(SharedVolumeId).nullable().optional(),
    z.array(SharedVolumeId).nullable(),
  ),
  password: PasswordType,
}).meta({
  id: 'BackupParams',
})
export const BackupReport: z.ZodObject<{
  server: typeof ServerBackupReport
  packages: z.ZodRecord<z.ZodString, typeof PackageBackupReport>
  sharedVolumes: z.ZodCodec<
    z.ZodOptional<z.ZodRecord<z.ZodString, typeof VolumeBackupReport>>,
    z.ZodRecord<z.ZodString, typeof VolumeBackupReport>
  >
}> = /* @__PURE__ */ object({
  server: ServerBackupReport,
  packages: z.record(z.string(), PackageBackupReport),
  sharedVolumes: wire(
    z.record(z.string(), VolumeBackupReport).optional(),
    z.record(z.string(), VolumeBackupReport),
  ),
}).meta({
  id: 'BackupReport',
})

/**
 * A registered backup destination and its last source observation.
 */
export const BackupTarget: z.ZodIntersection<
  z.ZodObject<{
    name: z.ZodString
    source: typeof SharedVolumeSource
    startOs: z.ZodCodec<
      z.ZodOptional<z.ZodRecord<z.ZodString, typeof StartOsRecoveryInfo>>,
      z.ZodRecord<z.ZodString, typeof StartOsRecoveryInfo>
    >
    legacyBackup: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  }>,
  typeof SharedVolumeInspection
> = /* @__PURE__ */ z
  .intersection(
    object({
      name: z.string(),
      source: SharedVolumeSource,
      startOs: wire(
        z.record(z.string(), StartOsRecoveryInfo).optional(),
        z.record(z.string(), StartOsRecoveryInfo),
      ),
      legacyBackup: wire(z.boolean().optional(), z.boolean()),
    }),
    SharedVolumeInspection,
  )
  .meta({
    id: 'BackupTarget',
    description:
      'A registered backup destination and its last source observation.',
  })
export const CompatibleAlpnInfo: z.ZodUnion<
  readonly [z.ZodArray<typeof MaybeUtf8String>, typeof LegacyAlpnInfo]
> = /* @__PURE__ */ z.union([z.array(MaybeUtf8String), LegacyAlpnInfo]).meta({
  id: 'CompatibleAlpnInfo',
})

/**
 * Selects an existing server backup and its unlocking credential.
 */
export const ConvertParams: z.ZodObject<{
  targetId: typeof BackupTargetId
  serverId: z.ZodString
  password: typeof PasswordType
}> = /* @__PURE__ */ object({
  targetId: BackupTargetId,
  serverId: z.string(),
  password: PasswordType,
}).meta({
  id: 'ConvertParams',
  description:
    'Selects an existing server backup and its unlocking credential.',
})
export const CreateSharedVolumeParams: z.ZodObject<{
  name: z.ZodString
  source: typeof CreateSharedVolumeSource
}> = /* @__PURE__ */ object({
  name: z.string(),
  source: CreateSharedVolumeSource,
}).meta({
  id: 'CreateSharedVolumeParams',
})
export const DependencyMetadata: z.ZodIntersection<
  z.ZodObject<{
    title: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<typeof LocaleString>>,
      z.ZodNullable<typeof LocaleString>
    >
    icon: z.ZodOptional<z.ZodNullable<typeof PackageIcon>>
    description: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<typeof LocaleString>>,
      z.ZodNullable<typeof LocaleString>
    >
    optional: z.ZodBoolean
    versionRange: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<z.ZodString>>,
      z.ZodNullable<z.ZodString>
    >
  }>,
  z.ZodUnion<
    readonly [z.ZodNullable<typeof CurrentDependencyKind>, typeof empty]
  >
> = /* @__PURE__ */ z
  .intersection(
    object({
      title: wire(LocaleString.nullable().optional(), LocaleString.nullable()),
      icon: PackageIcon.nullable().optional(),
      description: wire(
        LocaleString.nullable().optional(),
        LocaleString.nullable(),
      ),
      optional: z.boolean(),
      versionRange: wire(
        z.string().nullable().optional(),
        z.string().nullable(),
      ),
    }),
    z.union([CurrentDependencyKind.nullable(), empty]),
  )
  .meta({
    id: 'DependencyMetadata',
  })
export const HardwareInfo: z.ZodObject<{
  arch: z.ZodString
  ram: z.ZodInt
  devices: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<typeof LshwDevice>>>,
    z.ZodNullable<z.ZodArray<typeof LshwDevice>>
  >
}> = /* @__PURE__ */ object({
  arch: z.string(),
  ram: z.int(),
  devices: wire(
    z.array(LshwDevice).nullable().optional(),
    z.array(LshwDevice).nullable(),
  ),
}).meta({
  id: 'HardwareInfo',
})
export const HostAddress: z.ZodObject<{
  address: z.ZodString
  public: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof PublicDomainConfig>>,
    z.ZodNullable<typeof PublicDomainConfig>
  >
  private: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<typeof GatewayId>>>,
    z.ZodNullable<z.ZodArray<typeof GatewayId>>
  >
}> = /* @__PURE__ */ object({
  address: z.string(),
  public: wire(
    PublicDomainConfig.nullable().optional(),
    PublicDomainConfig.nullable(),
  ),
  private: wire(
    z.array(GatewayId).nullable().optional(),
    z.array(GatewayId).nullable(),
  ),
}).meta({
  id: 'HostAddress',
})
export const HostnameInfo: z.ZodObject<{
  ssl: z.ZodBoolean
  public: z.ZodBoolean
  hostname: z.ZodString
  port: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  metadata: typeof HostnameMetadata
}> = /* @__PURE__ */ object({
  ssl: z.boolean(),
  public: z.boolean(),
  hostname: z.string(),
  port: wire(z.int().nullable().optional(), z.int().nullable()),
  metadata: HostnameMetadata,
}).meta({
  id: 'HostnameInfo',
})
export const InstallParams: z.ZodObject<{
  acceptSigners: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof AcceptSigners>>,
    z.ZodNullable<typeof AcceptSigners>
  >
  registry: z.ZodString
  id: typeof PackageId
  version: typeof Version
  /**
   * Accepts an untrusted signer without skipping signature or integrity verification.
   */
  trust: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  acceptSigners: wire(
    AcceptSigners.nullable().optional(),
    AcceptSigners.nullable(),
  ),
  registry: z.string(),
  id: PackageId,
  version: Version,
  /**
   * Accepts an untrusted signer without skipping signature or integrity verification.
   */
  trust: wire(z.boolean().optional(), z.boolean()).meta({
    description:
      'Accepts an untrusted signer without skipping signature or integrity verification.',
  }),
}).meta({
  id: 'InstallParams',
})
export const Metrics: z.ZodObject<{
  general: typeof MetricsGeneral
  memory: typeof MetricsMemory
  cpu: typeof MetricsCpu
  disk: typeof MetricsDisk
}> = /* @__PURE__ */ object({
  general: MetricsGeneral,
  memory: MetricsMemory,
  cpu: MetricsCpu,
  disk: MetricsDisk,
}).meta({
  id: 'Metrics',
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
export const NotificationWithId: z.ZodIntersection<
  z.ZodObject<{ id: z.ZodInt }>,
  typeof Notification
> = /* @__PURE__ */ z.intersection(object({ id: z.int() }), Notification).meta({
  id: 'NotificationWithId',
})

/**
 * Stored NUT configuration and its independent enabled state.
 */
export const NutConfig: z.ZodObject<{
  /**
   * Whether StartOS should run NUT monitoring.
   */
  enabled: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  /**
   * Retained server or client settings, including while monitoring is disabled.
   */
  settings: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof NutSettings>>,
    z.ZodNullable<typeof NutSettings>
  >
}> = /* @__PURE__ */ object({
  /**
   * Whether StartOS should run NUT monitoring.
   */
  enabled: wire(z.boolean().optional(), z.boolean()).meta({
    description: 'Whether StartOS should run NUT monitoring.',
  }),
  /**
   * Retained server or client settings, including while monitoring is disabled.
   */
  settings: wire(
    NutSettings.nullable().optional(),
    NutSettings.nullable(),
  ).meta({
    description:
      'Retained server or client settings, including while monitoring is disabled.',
  }),
}).meta({
  id: 'NutConfig',
  description: 'Stored NUT configuration and its independent enabled state.',
})
export const OsVersionInfoMap: z.ZodRecord<z.ZodString, typeof OsVersionInfo> =
  /* @__PURE__ */ z.record(z.string(), OsVersionInfo).meta({
    id: 'OsVersionInfoMap',
  })
export const PackageAccess: z.ZodObject<{
  private: z.ZodBoolean
  readers: z.ZodRecord<z.ZodString, typeof SignerInfo>
}> = /* @__PURE__ */ object({
  private: z.boolean(),
  readers: z.record(z.string(), SignerInfo),
}).meta({
  id: 'PackageAccess',
})
export const PackageMetadata: z.ZodObject<{
  title: z.ZodString
  description: typeof Description
  releaseNotes: typeof LocaleString
  preDownloadAlert: z.ZodOptional<z.ZodNullable<typeof PreDownloadAlert>>
  gitHash: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof GitHash>>,
    z.ZodNullable<typeof GitHash>
  >
  license: z.ZodString
  packageRepo: z.ZodCodec<z.ZodOptional<z.ZodString>, z.ZodString>
  upstreamRepo: z.ZodString
  marketingUrl: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  donationUrl: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  osVersion: z.ZodCodec<z.ZodOptional<z.ZodString>, z.ZodString>
  sdkVersion: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  hardwareAcceleration: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  /**
   * Grants access to `/dev/fuse`.
   */
  userspaceFilesystems: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  /**
   * Grants access to `/dev/net/tun`.
   */
  virtualNetworking: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  /**
   * Grants /dev/kvm when present. The opening process must run as container root.
   */
  hardwareVirtualization: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  plugins: z.ZodCodec<
    z.ZodOptional<z.ZodArray<typeof PluginId>>,
    z.ZodArray<typeof PluginId>
  >
  satisfies: z.ZodCodec<
    z.ZodOptional<z.ZodArray<typeof Version>>,
    z.ZodArray<typeof Version>
  >
}> = /* @__PURE__ */ object({
  title: z.string(),
  description: Description,
  releaseNotes: LocaleString,
  preDownloadAlert: PreDownloadAlert.nullable().optional(),
  gitHash: wire(GitHash.nullable().optional(), GitHash.nullable()),
  license: z.string(),
  packageRepo: wire(z.string().optional(), z.string()),
  upstreamRepo: z.string(),
  marketingUrl: wire(z.string().nullable().optional(), z.string().nullable()),
  donationUrl: wire(z.string().nullable().optional(), z.string().nullable()),
  osVersion: wire(z.string().optional(), z.string()),
  sdkVersion: wire(z.string().nullable().optional(), z.string().nullable()),
  hardwareAcceleration: wire(z.boolean().optional(), z.boolean()),
  /**
   * Grants access to `/dev/fuse`.
   */
  userspaceFilesystems: wire(z.boolean().optional(), z.boolean()).meta({
    description: 'Grants access to `/dev/fuse`.',
  }),
  /**
   * Grants access to `/dev/net/tun`.
   */
  virtualNetworking: wire(z.boolean().optional(), z.boolean()).meta({
    description: 'Grants access to `/dev/net/tun`.',
  }),
  /**
   * Grants /dev/kvm when present. The opening process must run as container root.
   */
  hardwareVirtualization: wire(z.boolean().optional(), z.boolean()).meta({
    description:
      'Grants /dev/kvm when present. The opening process must run as container root.',
  }),
  plugins: wire(z.array(PluginId).optional(), z.array(PluginId)),
  satisfies: wire(z.array(Version).optional(), z.array(Version)),
}).meta({
  id: 'PackageMetadata',
})

/**
 * Representation of JSON Patch (list of patch operations)
 */
export const Patch: z.ZodArray<typeof PatchOperation> = /* @__PURE__ */ z
  .array(PatchOperation)
  .meta({
    id: 'Patch',
    description: 'Representation of JSON Patch (list of patch operations)',
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
export const ProxyOptions: z.ZodIntersection<
  z.ZodObject<{ type: z.ZodLiteral<'http'> }>,
  z.ZodObject<{
    /**
     * Adds `X-Forwarded-For` and, on the SSL leg, `X-Forwarded-Proto: https`.
     * Client-supplied `X-Forwarded-*` headers are stripped on both legs.
     */
    addXForwardedHeaders: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
    /**
     * Validates every request before forwarding. Basic auth sets `X-Forwarded-User`.
     */
    auth: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<typeof ProxyAuth>>,
      z.ZodNullable<typeof ProxyAuth>
    >
  }>
> = /* @__PURE__ */ z
  .intersection(
    object({ type: z.literal('http') }),
    object({
      /**
       * Adds `X-Forwarded-For` and, on the SSL leg, `X-Forwarded-Proto: https`.
       * Client-supplied `X-Forwarded-*` headers are stripped on both legs.
       */
      addXForwardedHeaders: wire(z.boolean().optional(), z.boolean()).meta({
        description:
          'Adds `X-Forwarded-For` and, on the SSL leg, `X-Forwarded-Proto: https`.\nClient-supplied `X-Forwarded-*` headers are stripped on both legs.',
      }),
      /**
       * Validates every request before forwarding. Basic auth sets `X-Forwarded-User`.
       */
      auth: wire(ProxyAuth.nullable().optional(), ProxyAuth.nullable()).meta({
        description:
          'Validates every request before forwarding. Basic auth sets `X-Forwarded-User`.',
      }),
    }),
  )
  .meta({
    id: 'ProxyOptions',
  })
export const RegistryInfo: z.ZodObject<{
  name: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  icon: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof DataUrl>>,
    z.ZodNullable<typeof DataUrl>
  >
  /**
   * Markdown, shown above the registry's services.
   */
  description: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof LocaleString>>,
    z.ZodNullable<typeof LocaleString>
  >
  categories: z.ZodRecord<z.ZodString, typeof Category>
  /**
   * Registry-selected signers, not an assertion of trust.
   */
  trustSigners: z.ZodCodec<
    z.ZodOptional<z.ZodRecord<z.ZodString, typeof SignerInfo>>,
    z.ZodRecord<z.ZodString, typeof SignerInfo>
  >
}> = /* @__PURE__ */ object({
  name: wire(z.string().nullable().optional(), z.string().nullable()),
  icon: wire(DataUrl.nullable().optional(), DataUrl.nullable()),
  /**
   * Markdown, shown above the registry's services.
   */
  description: wire(
    LocaleString.nullable().optional(),
    LocaleString.nullable(),
  ).meta({
    description: "Markdown, shown above the registry's services.",
  }),
  categories: z.record(z.string(), Category),
  /**
   * Registry-selected signers, not an assertion of trust.
   */
  trustSigners: wire(
    z.record(z.string(), SignerInfo).optional(),
    z.record(z.string(), SignerInfo),
  ).meta({
    description: 'Registry-selected signers, not an assertion of trust.',
  }),
}).meta({
  id: 'RegistryInfo',
})
export const ResetPasswordParams: z.ZodObject<{
  newPassword: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof PasswordType>>,
    z.ZodNullable<typeof PasswordType>
  >
}> = /* @__PURE__ */ object({
  newPassword: wire(
    PasswordType.nullable().optional(),
    PasswordType.nullable(),
  ),
}).meta({
  id: 'ResetPasswordParams',
})
export const ServiceInterface: z.ZodObject<{
  id: typeof ServiceInterfaceId
  name: z.ZodString
  description: z.ZodString
  masked: z.ZodBoolean
  addressInfo: typeof AddressInfo
  type: typeof ServiceInterfaceType
  /**
   * The interface address Open UI should prefer.
   */
  preferredLauncherAddress: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  id: ServiceInterfaceId,
  name: z.string(),
  description: z.string(),
  masked: z.boolean(),
  addressInfo: AddressInfo,
  type: ServiceInterfaceType,
  /**
   * The interface address Open UI should prefer.
   */
  preferredLauncherAddress: wire(
    z.string().nullable().optional(),
    z.string().nullable(),
  ).meta({
    description: 'The interface address Open UI should prefer.',
  }),
}).meta({
  id: 'ServiceInterface',
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
export const SideloadParams: z.ZodObject<{
  acceptSigners: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof AcceptSigners>>,
    z.ZodNullable<typeof AcceptSigners>
  >
  /**
   * DER64 supplemental signatures for the uploaded archive.
   */
  signatures: z.ZodCodec<
    z.ZodOptional<z.ZodRecord<z.ZodString, typeof AnySignature>>,
    z.ZodRecord<z.ZodString, typeof AnySignature>
  >
  /**
   * Accepts an untrusted signer without skipping signature or integrity verification.
   */
  trust: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  acceptSigners: wire(
    AcceptSigners.nullable().optional(),
    AcceptSigners.nullable(),
  ),
  /**
   * DER64 supplemental signatures for the uploaded archive.
   */
  signatures: wire(
    z.record(z.string(), AnySignature).optional(),
    z.record(z.string(), AnySignature),
  ).meta({
    description: 'DER64 supplemental signatures for the uploaded archive.',
  }),
  /**
   * Accepts an untrusted signer without skipping signature or integrity verification.
   */
  trust: wire(z.boolean().optional(), z.boolean()).meta({
    description:
      'Accepts an untrusted signer without skipping signature or integrity verification.',
  }),
}).meta({
  id: 'SideloadParams',
})

/**
 * Opted-in rpcstream requests receive the dump directly; other callers receive a GUID.
 */
export const SubscribeResult: z.ZodUnion<
  readonly [typeof SubscribeRes, typeof Dump]
> = /* @__PURE__ */ z.union([SubscribeRes, Dump]).meta({
  id: 'SubscribeResult',
  description:
    'Opted-in rpcstream requests receive the dump directly; other callers receive a GUID.',
})

/**
 * Signing-key material and independent package and OS-update trust.
 */
export const TrustedKey: z.ZodObject<{
  name: z.ZodString
  /**
   * `manual`, or the base URL of the registry that first offered the key.
   */
  origin: z.ZodCodec<z.ZodOptional<z.ZodString>, z.ZodString>
  /**
   * Registry identity recorded when this key was first imported.
   */
  originRegistry: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof RegistryIdentity>>,
    z.ZodNullable<typeof RegistryIdentity>
  >
  publicKey: typeof AnyVerifyingKey
  /**
   * Original GPG exports; signing trust is determined by `public_key`.
   */
  gpgExports: z.ZodCodec<
    z.ZodOptional<z.ZodArray<typeof GpgPublicKeyExport>>,
    z.ZodArray<typeof GpgPublicKeyExport>
  >
  /**
   * Contact details registries publish for the key's signer.
   */
  contact: z.ZodCodec<
    z.ZodOptional<z.ZodArray<typeof ContactInfo>>,
    z.ZodArray<typeof ContactInfo>
  >
  addedAt: z.ZodString
  /**
   * False while a connected registry offers the key.
   */
  removable: z.ZodBoolean
  marketplace: z.ZodBoolean
  os: z.ZodBoolean
}> = /* @__PURE__ */ object({
  name: z.string(),
  /**
   * `manual`, or the base URL of the registry that first offered the key.
   */
  origin: wire(z.string().optional(), z.string()).meta({
    description:
      '`manual`, or the base URL of the registry that first offered the key.',
  }),
  /**
   * Registry identity recorded when this key was first imported.
   */
  originRegistry: wire(
    RegistryIdentity.nullable().optional(),
    RegistryIdentity.nullable(),
  ).meta({
    description: 'Registry identity recorded when this key was first imported.',
  }),
  publicKey: AnyVerifyingKey,
  /**
   * Original GPG exports; signing trust is determined by `public_key`.
   */
  gpgExports: wire(
    z.array(GpgPublicKeyExport).optional(),
    z.array(GpgPublicKeyExport),
  ).meta({
    description:
      'Original GPG exports; signing trust is determined by `public_key`.',
  }),
  /**
   * Contact details registries publish for the key's signer.
   */
  contact: wire(z.array(ContactInfo).optional(), z.array(ContactInfo)).meta({
    description: "Contact details registries publish for the key's signer.",
  }),
  addedAt: z.string(),
  /**
   * False while a connected registry offers the key.
   */
  removable: z.boolean().meta({
    description: 'False while a connected registry offers the key.',
  }),
  marketplace: z.boolean(),
  os: z.boolean(),
}).meta({
  id: 'TrustedKey',
  description:
    'Signing-key material and independent package and OS-update trust.',
})
export const UpdateSystemParams: z.ZodObject<{
  registry: z.ZodString
  /**
   * Additional signer requirements checked before starting the update.
   */
  acceptSigners: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof AcceptSigners>>,
    z.ZodNullable<typeof AcceptSigners>
  >
  /**
   * Accepts an untrusted signer without skipping signature or integrity verification.
   */
  trust: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  targetVersion: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Requests a legacy progress GUID; rpcstream observes completion independently.
   */
  progress: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  registry: z.string(),
  /**
   * Additional signer requirements checked before starting the update.
   */
  acceptSigners: wire(
    AcceptSigners.nullable().optional(),
    AcceptSigners.nullable(),
  ).meta({
    description:
      'Additional signer requirements checked before starting the update.',
  }),
  /**
   * Accepts an untrusted signer without skipping signature or integrity verification.
   */
  trust: wire(z.boolean().optional(), z.boolean()).meta({
    description:
      'Accepts an untrusted signer without skipping signature or integrity verification.',
  }),
  targetVersion: wire(z.string().nullable().optional(), z.string().nullable()),
  /**
   * Requests a legacy progress GUID; rpcstream observes completion independently.
   */
  progress: wire(z.boolean().optional(), z.boolean()).meta({
    description:
      'Requests a legacy progress GUID; rpcstream observes completion independently.',
  }),
}).meta({
  id: 'UpdateSystemParams',
})
export const WifiListInfo: z.ZodObject<{
  ssids: z.ZodRecord<z.ZodString, typeof SignalStrength>
  connected: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Ssid>>,
    z.ZodNullable<typeof Ssid>
  >
  country: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  ethernet: z.ZodBoolean
  availableWifi: z.ZodArray<typeof WifiListOut>
}> = /* @__PURE__ */ object({
  ssids: z.record(z.string(), SignalStrength),
  connected: wire(Ssid.nullable().optional(), Ssid.nullable()),
  country: wire(z.string().nullable().optional(), z.string().nullable()),
  ethernet: z.boolean(),
  availableWifi: z.array(WifiListOut),
}).meta({
  id: 'WifiListInfo',
})
export const ActionResult: z.ZodUnion<
  readonly [
    z.ZodIntersection<
      z.ZodObject<{ version: z.ZodLiteral<'0'> }>,
      typeof ActionResultV0
    >,
    z.ZodIntersection<
      z.ZodObject<{ version: z.ZodLiteral<'1'> }>,
      typeof ActionResultV1
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(object({ version: z.literal('0') }), ActionResultV0),
    z.intersection(object({ version: z.literal('1') }), ActionResultV1),
  ])
  .meta({
    id: 'ActionResult',
  })
export const AddSslOptions: z.ZodCodec<
  z.ZodObject<{
    preferredExternalPort: z.ZodInt
    /**
     * The application protocols StartOS answers a client with, from those it
     * asked for. Unset answers with whatever it asked for.
     */
    alpn: z.ZodOptional<z.ZodNullable<typeof CompatibleAlpnInfo>>
    /**
     * Certificate validation for the OS→container TLS leg when rewrapping.
     * `None` (the default) validates against the StartOS root CA.
     */
    upstreamCertValidation: z.ZodOptional<
      z.ZodNullable<typeof UpstreamCertValidation>
    >
  }>,
  z.ZodObject<{
    preferredExternalPort: z.ZodInt
    /**
     * The application protocols StartOS answers a client with, from those it
     * asked for. Unset answers with whatever it asked for.
     */
    alpn: z.ZodNullable<typeof AlpnInfo>
    /**
     * Certificate validation for the OS→container TLS leg when rewrapping.
     * `None` (the default) validates against the StartOS root CA.
     */
    upstreamCertValidation: z.ZodNullable<typeof UpstreamCertValidation>
  }>
> = /* @__PURE__ */ wire(
  object({
    preferredExternalPort: z.int(),
    /**
     * The application protocols StartOS answers a client with, from those it
     * asked for. Unset answers with whatever it asked for.
     */
    alpn: CompatibleAlpnInfo.nullable().optional().meta({
      description:
        'The application protocols StartOS answers a client with, from those it\nasked for. Unset answers with whatever it asked for.',
    }),
    /**
     * Certificate validation for the OS→container TLS leg when rewrapping.
     * `None` (the default) validates against the StartOS root CA.
     */
    upstreamCertValidation: UpstreamCertValidation.nullable().optional().meta({
      description:
        'Certificate validation for the OS→container TLS leg when rewrapping.\n`None` (the default) validates against the StartOS root CA.',
    }),
  }),
  object({
    preferredExternalPort: z.int(),
    /**
     * The application protocols StartOS answers a client with, from those it
     * asked for. Unset answers with whatever it asked for.
     */
    alpn: AlpnInfo.nullable().meta({
      description:
        'The application protocols StartOS answers a client with, from those it\nasked for. Unset answers with whatever it asked for.',
    }),
    /**
     * Certificate validation for the OS→container TLS leg when rewrapping.
     * `None` (the default) validates against the StartOS root CA.
     */
    upstreamCertValidation: UpstreamCertValidation.nullable().meta({
      description:
        'Certificate validation for the OS→container TLS leg when rewrapping.\n`None` (the default) validates against the StartOS root CA.',
    }),
  }),
).meta({
  id: 'AddSslOptions',
})

/**
 * A completed background backup with its original target name and final report.
 */
export const BackupRun: z.ZodObject<{
  id: z.ZodString
  targetId: typeof BackupTargetId
  targetName: z.ZodString
  startedAt: z.ZodString
  completedAt: z.ZodString
  report: typeof BackupReport
}> = /* @__PURE__ */ object({
  id: z.string(),
  targetId: BackupTargetId,
  targetName: z.string(),
  startedAt: z.string(),
  completedAt: z.string(),
  report: BackupReport,
}).meta({
  id: 'BackupRun',
  description:
    'A completed background backup with its original target name and final report.',
})
export const BindingSetAddressEnabledParams: z.ZodObject<{
  internalPort: z.ZodInt
  address: typeof HostnameInfo
  enabled: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodBoolean>>,
    z.ZodNullable<z.ZodBoolean>
  >
}> = /* @__PURE__ */ object({
  internalPort: z.int(),
  address: HostnameInfo,
  enabled: wire(z.boolean().nullable().optional(), z.boolean().nullable()),
}).meta({
  id: 'BindingSetAddressEnabledParams',
})
export const BindingSetGuaWanParams: z.ZodObject<{
  internalPort: z.ZodInt
  address: typeof HostnameInfo
  wan: z.ZodBoolean
}> = /* @__PURE__ */ object({
  internalPort: z.int(),
  address: HostnameInfo,
  wan: z.boolean(),
}).meta({
  id: 'BindingSetGuaWanParams',
})
export const DerivedAddressInfo: z.ZodObject<{
  /**
   * User override: enable these addresses (only for public IP & port)
   */
  enabled: z.ZodArray<z.ZodString>
  /**
   * User override: disable these addresses (only for domains and private IP & port)
   */
  disabled: z.ZodArray<z.ZodTuple<[z.ZodString, z.ZodInt], null>>
  /**
   * User override: IPv6 global-unicast addresses opted into WAN exposure.
   * Projected into `HostnameInfo.public` by `update_addresses`, so `public`
   * stays the single source of truth for WAN reachability (P2P address
   * selection, upstream pinholes). A GUA not in this set is LAN-only.
   */
  guaWan: z.ZodCodec<
    z.ZodOptional<z.ZodArray<z.ZodString>>,
    z.ZodArray<z.ZodString>
  >
  /**
   * User override: enable these private IPs regardless of their mDNS address.
   */
  lanEnabled: z.ZodCodec<
    z.ZodOptional<z.ZodArray<z.ZodTuple<[z.ZodString, z.ZodInt], null>>>,
    z.ZodArray<z.ZodTuple<[z.ZodString, z.ZodInt], null>>
  >
  /**
   * COMPUTED: NetServiceData::update — all possible addresses for this binding
   */
  available: z.ZodArray<typeof HostnameInfo>
}> = /* @__PURE__ */ object({
  /**
   * User override: enable these addresses (only for public IP & port)
   */
  enabled: z.array(z.string()).meta({
    description:
      'User override: enable these addresses (only for public IP & port)',
  }),
  /**
   * User override: disable these addresses (only for domains and private IP & port)
   */
  disabled: z.array(z.tuple([z.string(), z.int()])).meta({
    description:
      'User override: disable these addresses (only for domains and private IP & port)',
  }),
  /**
   * User override: IPv6 global-unicast addresses opted into WAN exposure.
   * Projected into `HostnameInfo.public` by `update_addresses`, so `public`
   * stays the single source of truth for WAN reachability (P2P address
   * selection, upstream pinholes). A GUA not in this set is LAN-only.
   */
  guaWan: wire(z.array(z.string()).optional(), z.array(z.string())).meta({
    description:
      'User override: IPv6 global-unicast addresses opted into WAN exposure.\nProjected into `HostnameInfo.public` by `update_addresses`, so `public`\nstays the single source of truth for WAN reachability (P2P address\nselection, upstream pinholes). A GUA not in this set is LAN-only.',
  }),
  /**
   * User override: enable these private IPs regardless of their mDNS address.
   */
  lanEnabled: wire(
    z.array(z.tuple([z.string(), z.int()])).optional(),
    z.array(z.tuple([z.string(), z.int()])),
  ).meta({
    description:
      'User override: enable these private IPs regardless of their mDNS address.',
  }),
  /**
   * COMPUTED: NetServiceData::update — all possible addresses for this binding
   */
  available: z.array(HostnameInfo).meta({
    description:
      'COMPUTED: NetServiceData::update — all possible addresses for this binding',
  }),
}).meta({
  id: 'DerivedAddressInfo',
})
export const DeviceInfo: z.ZodObject<{
  os: typeof OsInfo
  hardware: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof HardwareInfo>>,
    z.ZodNullable<typeof HardwareInfo>
  >
}> = /* @__PURE__ */ object({
  os: OsInfo,
  hardware: wire(HardwareInfo.nullable().optional(), HardwareInfo.nullable()),
}).meta({
  id: 'DeviceInfo',
})

/**
 * Database-generated patches contain only add, remove and replace operations.
 */
export const DiffPatch: typeof Patch = /* @__PURE__ */ Patch.meta({
  id: 'DiffPatch',
  description:
    'Database-generated patches contain only add, remove and replace operations.',
})
export const MetricsFollowResponse: z.ZodObject<{
  /**
   * Present for ordinary replies; typed observations use same-ID events.
   */
  guid: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Guid>>,
    z.ZodNullable<typeof Guid>
  >
  metrics: typeof Metrics
}> = /* @__PURE__ */ object({
  /**
   * Present for ordinary replies; typed observations use same-ID events.
   */
  guid: wire(Guid.nullable().optional(), Guid.nullable()).meta({
    description:
      'Present for ordinary replies; typed observations use same-ID events.',
  }),
  metrics: Metrics,
}).meta({
  id: 'MetricsFollowResponse',
})
export const NetworkGatewayInfo: z.ZodObject<{
  id: typeof GatewayId
  info: typeof NetworkInterfaceInfo
}> = /* @__PURE__ */ object({ id: GatewayId, info: NetworkInterfaceInfo }).meta(
  {
    id: 'NetworkGatewayInfo',
  },
)
export const OsIndex: z.ZodObject<{ versions: typeof OsVersionInfoMap }> =
  /* @__PURE__ */ object({ versions: OsVersionInfoMap }).meta({
    id: 'OsIndex',
  })
export const PackageVersionInfo: z.ZodIntersection<
  z.ZodObject<{
    icon: z.ZodOptional<z.ZodNullable<typeof PackageIcon>>
    dependencyMetadata: z.ZodRecord<z.ZodString, typeof DependencyMetadata>
    sourceVersion: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<z.ZodString>>,
      z.ZodNullable<z.ZodString>
    >
    s9pks: z.ZodArray<
      z.ZodTuple<
        [
          typeof HardwareRequirements,
          ReturnType<typeof RegistryAsset<typeof MerkleArchiveCommitment>>,
        ],
        null
      >
    >
  }>,
  typeof PackageMetadata
> = /* @__PURE__ */ z
  .intersection(
    object({
      icon: PackageIcon.nullable().optional(),
      dependencyMetadata: z.record(z.string(), DependencyMetadata),
      sourceVersion: wire(
        z.string().nullable().optional(),
        z.string().nullable(),
      ),
      s9pks: z.array(
        z.tuple([HardwareRequirements, RegistryAsset(MerkleArchiveCommitment)]),
      ),
    }),
    PackageMetadata,
  )
  .meta({
    id: 'PackageVersionInfo',
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

/**
 * Parameters for replacing the stored NUT configuration.
 */
export const SetNutParams: z.ZodObject<{ config: typeof NutConfig }> =
  /* @__PURE__ */ object({ config: NutConfig }).meta({
    id: 'SetNutParams',
    description: 'Parameters for replacing the stored NUT configuration.',
  })
export const BackupHistory: z.ZodObject<{
  runs: z.ZodArray<typeof BackupRun>
  total: z.ZodInt
}> = /* @__PURE__ */ object({ runs: z.array(BackupRun), total: z.int() }).meta({
  id: 'BackupHistory',
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
export const GetPackageResponse: z.ZodObject<{
  categories: z.ZodArray<z.ZodString>
  best: z.ZodRecord<z.ZodString, typeof PackageVersionInfo>
  otherVersions: z.ZodOptional<
    z.ZodNullable<z.ZodRecord<z.ZodString, typeof PackageInfoShort>>
  >
}> = /* @__PURE__ */ object({
  categories: z.array(z.string()),
  best: z.record(z.string(), PackageVersionInfo),
  otherVersions: z.record(z.string(), PackageInfoShort).nullable().optional(),
}).meta({
  id: 'GetPackageResponse',
})
export const GetPackageResponseFull: z.ZodObject<{
  categories: z.ZodArray<z.ZodString>
  best: z.ZodRecord<z.ZodString, typeof PackageVersionInfo>
  otherVersions: z.ZodRecord<z.ZodString, typeof PackageVersionInfo>
}> = /* @__PURE__ */ object({
  categories: z.array(z.string()),
  best: z.record(z.string(), PackageVersionInfo),
  otherVersions: z.record(z.string(), PackageVersionInfo),
}).meta({
  id: 'GetPackageResponseFull',
})
export const LegacyProxySslOptions: z.ZodIntersection<
  z.ZodObject<{
    addXForwardedHeaders: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
    auth: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<typeof ProxyAuth>>,
      z.ZodNullable<typeof ProxyAuth>
    >
  }>,
  typeof AddSslOptions
> = /* @__PURE__ */ z
  .intersection(
    object({
      addXForwardedHeaders: wire(z.boolean().optional(), z.boolean()),
      auth: wire(ProxyAuth.nullable().optional(), ProxyAuth.nullable()),
    }),
    AddSslOptions,
  )
  .meta({
    id: 'LegacyProxySslOptions',
  })
export const PackageInfo: z.ZodIntersection<
  z.ZodObject<{
    authorized: z.ZodRecord<z.ZodString, z.ZodString>
    versions: z.ZodRecord<z.ZodString, typeof PackageVersionInfo>
    categories: z.ZodArray<z.ZodString>
  }>,
  typeof PackageAccessControl
> = /* @__PURE__ */ z
  .intersection(
    object({
      authorized: z.record(z.string(), z.string()),
      versions: z.record(z.string(), PackageVersionInfo),
      categories: z.array(z.string()),
    }),
    PackageAccessControl,
  )
  .meta({
    id: 'PackageInfo',
  })
export const Revision: z.ZodObject<{ id: z.ZodInt; patch: typeof DiffPatch }> =
  /* @__PURE__ */ object({ id: z.int(), patch: DiffPatch }).meta({
    id: 'Revision',
  })
export const CompatibleBindOptions: z.ZodObject<{
  preferredExternalPort: z.ZodInt
  addSsl: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof LegacyProxySslOptions>>,
    z.ZodNullable<typeof LegacyProxySslOptions>
  >
  secure: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Security>>,
    z.ZodNullable<typeof Security>
  >
  proxy: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof ProxyOptions>>,
    z.ZodNullable<typeof ProxyOptions>
  >
}> = /* @__PURE__ */ object({
  preferredExternalPort: z.int(),
  addSsl: wire(
    LegacyProxySslOptions.nullable().optional(),
    LegacyProxySslOptions.nullable(),
  ),
  secure: wire(Security.nullable().optional(), Security.nullable()),
  proxy: wire(ProxyOptions.nullable().optional(), ProxyOptions.nullable()),
}).meta({
  id: 'CompatibleBindOptions',
})
export const GetPackageResult: z.ZodUnion<
  readonly [
    typeof GetPackageResponseFull,
    typeof GetPackageResponse,
    z.ZodRecord<z.ZodString, typeof GetPackageResponseFull>,
    z.ZodRecord<z.ZodString, typeof GetPackageResponse>,
  ]
> = /* @__PURE__ */ z
  .union([
    GetPackageResponseFull,
    GetPackageResponse,
    z.record(z.string(), GetPackageResponseFull),
    z.record(z.string(), GetPackageResponse),
  ])
  .meta({
    id: 'GetPackageResult',
  })
export const PackageIndex: z.ZodObject<{
  categories: z.ZodRecord<z.ZodString, typeof Category>
  packages: z.ZodRecord<z.ZodString, typeof PackageInfo>
}> = /* @__PURE__ */ object({
  categories: z.record(z.string(), Category),
  packages: z.record(z.string(), PackageInfo),
}).meta({
  id: 'PackageIndex',
})
export const BindOptions: z.ZodCodec<
  typeof CompatibleBindOptions,
  z.ZodObject<{
    preferredExternalPort: z.ZodInt
    addSsl: z.ZodNullable<typeof AddSslOptions>
    secure: z.ZodNullable<typeof Security>
    /**
     * HTTP request policy shared by every exposed leg of this binding.
     */
    proxy: z.ZodNullable<typeof ProxyOptions>
  }>
> = /* @__PURE__ */ wire(
  CompatibleBindOptions,
  object({
    preferredExternalPort: z.int(),
    addSsl: AddSslOptions.nullable(),
    secure: Security.nullable(),
    /**
     * HTTP request policy shared by every exposed leg of this binding.
     */
    proxy: ProxyOptions.nullable().meta({
      description:
        'HTTP request policy shared by every exposed leg of this binding.',
    }),
  }),
).meta({
  id: 'BindOptions',
})
export const FullIndex: z.ZodObject<{
  name: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  icon: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof DataUrl>>,
    z.ZodNullable<typeof DataUrl>
  >
  description: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof LocaleString>>,
    z.ZodNullable<typeof LocaleString>
  >
  package: typeof PackageIndex
  os: typeof OsIndex
  signers: z.ZodRecord<z.ZodString, typeof SignerInfo>
  trustSigners: z.ZodCodec<
    z.ZodOptional<z.ZodArray<typeof Guid>>,
    z.ZodArray<typeof Guid>
  >
}> = /* @__PURE__ */ object({
  name: wire(z.string().nullable().optional(), z.string().nullable()),
  icon: wire(DataUrl.nullable().optional(), DataUrl.nullable()),
  description: wire(
    LocaleString.nullable().optional(),
    LocaleString.nullable(),
  ),
  package: PackageIndex,
  os: OsIndex,
  signers: z.record(z.string(), SignerInfo),
  trustSigners: wire(z.array(Guid).optional(), z.array(Guid)),
}).meta({
  id: 'FullIndex',
})
export const BindInfo: z.ZodObject<{
  enabled: z.ZodBoolean
  options: typeof BindOptions
  net: typeof NetInfo
  addresses: typeof DerivedAddressInfo
  /**
   * Service interfaces exported from this binding (`Origin.export`). A single
   * binding (host + internal port) may back several interfaces (e.g. a `ui`
   * and an `api` on the same port), so this is keyed by interface id.
   */
  interfaces: z.ZodCodec<
    z.ZodOptional<z.ZodRecord<z.ZodString, typeof ServiceInterface>>,
    z.ZodRecord<z.ZodString, typeof ServiceInterface>
  >
}> = /* @__PURE__ */ object({
  enabled: z.boolean(),
  options: BindOptions,
  net: NetInfo,
  addresses: DerivedAddressInfo,
  /**
   * Service interfaces exported from this binding (`Origin.export`). A single
   * binding (host + internal port) may back several interfaces (e.g. a `ui`
   * and an `api` on the same port), so this is keyed by interface id.
   */
  interfaces: wire(
    z.record(z.string(), ServiceInterface).optional(),
    z.record(z.string(), ServiceInterface),
  ).meta({
    description:
      'Service interfaces exported from this binding (`Origin.export`). A single\nbinding (host + internal port) may back several interfaces (e.g. a `ui`\nand an `api` on the same port), so this is keyed by interface id.',
  }),
}).meta({
  id: 'BindInfo',
})
export const Bindings: z.ZodRecord<z.ZodString, typeof BindInfo> =
  /* @__PURE__ */ z.record(z.string(), BindInfo).meta({
    id: 'Bindings',
  })
export const Api = {
  _PARAMS: /* @__PURE__ */ empty,
  _CHILDREN: {
    auth: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        'get-pubkey': {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.record(z.string(), z.unknown()),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        login: {
          _PARAMS: /* @__PURE__ */ LoginParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        logout: {
          _PARAMS: /* @__PURE__ */ LogoutParams,
          _RETURN: /* @__PURE__ */ HasUnenrolledKeys.nullable(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'reset-password': {
          _PARAMS: /* @__PURE__ */ ResetPasswordParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        session: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            kill: {
              _PARAMS: /* @__PURE__ */ KillParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
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
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        token: {
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
                CreateTokenParams,
              ),
              _RETURN: /* @__PURE__ */ CreatedToken,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
      },
    },
    backup: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        create: {
          _PARAMS: /* @__PURE__ */ BackupParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        history: {
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
                HistoryParams,
              ),
              _RETURN: /* @__PURE__ */ BackupHistory,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        target: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            cifs: {
              _PARAMS: /* @__PURE__ */ empty,
              _CHILDREN: {
                add: {
                  _PARAMS: /* @__PURE__ */ CifsAddParams,
                  _RETURN: /* @__PURE__ */ z.record(z.string(), BackupTarget),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                remove: {
                  _PARAMS: /* @__PURE__ */ CifsRemoveParams,
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                update: {
                  _PARAMS: /* @__PURE__ */ CifsUpdateParams,
                  _RETURN: /* @__PURE__ */ z.record(z.string(), BackupTarget),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
              },
            },
            convert: {
              _PARAMS: /* @__PURE__ */ ConvertParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            create: {
              _PARAMS: /* @__PURE__ */ CreateSharedVolumeParams,
              _RETURN: /* @__PURE__ */ BackupTargetId,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'delete-legacy': {
              _PARAMS: /* @__PURE__ */ DeleteLegacyParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            find: {
              _PARAMS: /* @__PURE__ */ empty,
              _RETURN: /* @__PURE__ */ z.array(BackupTargetId),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
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
                InfoParams,
              ),
              _RETURN: /* @__PURE__ */ BackupInfo,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            inspect: {
              _PARAMS: /* @__PURE__ */ TargetParams,
              _RETURN: /* @__PURE__ */ SharedVolumeInspection,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
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
              _RETURN: /* @__PURE__ */ z.record(z.string(), BackupTarget),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            mount: {
              _PARAMS: /* @__PURE__ */ MountParams,
              _RETURN: /* @__PURE__ */ z.string(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            remove: {
              _PARAMS: /* @__PURE__ */ TargetParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            rename: {
              _PARAMS: /* @__PURE__ */ RenameTargetParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            umount: {
              _PARAMS: /* @__PURE__ */ UmountParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'update-cifs': {
              _PARAMS: /* @__PURE__ */ UpdateCifsTargetParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        volume: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            restore: {
              _PARAMS: /* @__PURE__ */ RestoreVolumeParams,
              _RETURN: /* @__PURE__ */ RestoreVolumeResult,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
      },
    },
    db: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        apply: {
          _PARAMS: /* @__PURE__ */ ApplyParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        dump: {
          _PARAMS: /* @__PURE__ */ DumpParams,
          _RETURN: /* @__PURE__ */ Dump,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        put: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            ui: {
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
                UiParams,
              ),
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        subscribe: {
          _PARAMS: /* @__PURE__ */ SubscribeParams,
          _RETURN: /* @__PURE__ */ SubscribeResult,
          _ITEM: /* @__PURE__ */ Revision,
          _FINAL: /* @__PURE__ */ z.null(),
        },
      },
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
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            repair: {
              _PARAMS: /* @__PURE__ */ empty,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
      },
    },
    disk: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        format: {
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
            FormatDiskParams,
          ),
          _RETURN: /* @__PURE__ */ z.string(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        repair: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
      },
    },
    echo: {
      _PARAMS: /* @__PURE__ */ EchoParams,
      _RETURN: /* @__PURE__ */ z.string(),
      _ITEM: /* @__PURE__ */ z.never(),
      _FINAL: /* @__PURE__ */ z.null(),
    },
    'git-info': {
      _PARAMS: /* @__PURE__ */ empty,
      _RETURN: /* @__PURE__ */ z.string(),
      _ITEM: /* @__PURE__ */ z.never(),
      _FINAL: /* @__PURE__ */ z.null(),
    },
    init: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
    kiosk: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        disable: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        enable: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
      },
    },
    net: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        acme: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            'check-challenge': {
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
                CheckChallengeParams,
              ),
              _RETURN: /* @__PURE__ */ CheckChallengeRes.nullable(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            init: {
              _PARAMS: /* @__PURE__ */ InitAcmeParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            remove: {
              _PARAMS: /* @__PURE__ */ RemoveAcmeParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        dns: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            'dump-table': {
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
              _RETURN: /* @__PURE__ */ z.record(
                z.string(),
                z.string().nullable(),
              ),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
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
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'set-static': {
              _PARAMS: /* @__PURE__ */ SetStaticDnsParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            status: {
              _PARAMS: /* @__PURE__ */ empty,
              _RETURN: /* @__PURE__ */ DnsStatus,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        forward: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            'dump-table': {
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
              _RETURN: /* @__PURE__ */ ForwardTable,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        gateway: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            'check-dns': {
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
                CheckDnsParams,
              ),
              _RETURN: /* @__PURE__ */ z.boolean(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'check-port': {
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
                CheckPortParams,
              ),
              _RETURN: /* @__PURE__ */ CheckPortRes,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'check-port-v6': {
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
                CheckPortParams,
              ),
              _RETURN: /* @__PURE__ */ CheckPortV6Res.nullable(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            edit: {
              _PARAMS: /* @__PURE__ */ EditGatewayParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            forget: {
              _PARAMS: /* @__PURE__ */ ForgetGatewayParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
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
              _RETURN: /* @__PURE__ */ z.record(
                z.string(),
                NetworkInterfaceInfo,
              ),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'set-default-outbound': {
              _PARAMS: /* @__PURE__ */ SetDefaultOutboundParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'set-name': {
              _PARAMS: /* @__PURE__ */ RenameGatewayParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'set-secure': {
              _PARAMS: /* @__PURE__ */ SetGatewaySecureParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'unset-secure': {
              _PARAMS: /* @__PURE__ */ UnsetGatewaySecureParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        ssl: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            'generate-certificate': {
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
                GenerateCertificateParams,
              ),
              _RETURN: /* @__PURE__ */ GenerateCertificateResponse,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        tunnel: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            add: {
              _PARAMS: /* @__PURE__ */ AddTunnelParams,
              _RETURN: /* @__PURE__ */ GatewayId,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            remove: {
              _PARAMS: /* @__PURE__ */ RemoveTunnelParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            update: {
              _PARAMS: /* @__PURE__ */ UpdateTunnelParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        vhost: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            'add-passthrough': {
              _PARAMS: /* @__PURE__ */ AddPassthroughParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'dump-table': {
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
              _RETURN: /* @__PURE__ */ z.record(
                z.string(),
                z.record(z.string(), z.array(z.string())),
              ),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'list-passthrough': {
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
              _RETURN: /* @__PURE__ */ z.array(PassthroughInfo),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'remove-passthrough': {
              _PARAMS: /* @__PURE__ */ RemovePassthroughParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
      },
    },
    notification: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        create: {
          _PARAMS: /* @__PURE__ */ CreateParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
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
            ListNotificationParams,
          ),
          _RETURN: /* @__PURE__ */ z.array(NotificationWithId),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'mark-seen': {
          _PARAMS: /* @__PURE__ */ ModifyNotificationParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'mark-seen-before': {
          _PARAMS: /* @__PURE__ */ ModifyNotificationBeforeParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'mark-unseen': {
          _PARAMS: /* @__PURE__ */ ModifyNotificationParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        push: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            key: {
              _PARAMS: /* @__PURE__ */ empty,
              _RETURN: /* @__PURE__ */ z.string(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            list: {
              _PARAMS: /* @__PURE__ */ empty,
              _RETURN: /* @__PURE__ */ z.array(PushSubscription),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            subscribe: {
              _PARAMS: /* @__PURE__ */ PushSubscription,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            test: {
              _PARAMS: /* @__PURE__ */ EndpointParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            unsubscribe: {
              _PARAMS: /* @__PURE__ */ EndpointParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        remove: {
          _PARAMS: /* @__PURE__ */ ModifyNotificationParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'remove-before': {
          _PARAMS: /* @__PURE__ */ ModifyNotificationBeforeParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
      },
    },
    package: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        action: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            'clear-task': {
              _PARAMS: /* @__PURE__ */ ClearTaskParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'get-input': {
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
                GetActionInputParams,
              ),
              _RETURN: /* @__PURE__ */ ActionInput.nullable(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            run: {
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
                RunActionParams,
              ),
              _RETURN: /* @__PURE__ */ ActionResult.nullable(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        attach: {
          _PARAMS: /* @__PURE__ */ AttachParams,
          _RETURN: /* @__PURE__ */ Guid,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        backup: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            restore: {
              _PARAMS: /* @__PURE__ */ RestorePackageParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        'cancel-install': {
          _PARAMS: /* @__PURE__ */ CancelInstallParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        exec: {
          _PARAMS: /* @__PURE__ */ ExecParams,
          _RETURN: /* @__PURE__ */ ExecResult,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
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
                      _CHILDREN: {
                        add: {
                          _PARAMS: /* @__PURE__ */ z.intersection(
                            z.intersection(RequiresPackageId, RequiresHostId),
                            AddPrivateDomainParams,
                          ),
                          _RETURN: /* @__PURE__ */ z.boolean(),
                          _ITEM: /* @__PURE__ */ z.never(),
                          _FINAL: /* @__PURE__ */ z.null(),
                        },
                        remove: {
                          _PARAMS: /* @__PURE__ */ z.intersection(
                            z.intersection(RequiresPackageId, RequiresHostId),
                            RemoveDomainParams,
                          ),
                          _RETURN: /* @__PURE__ */ z.null(),
                          _ITEM: /* @__PURE__ */ z.never(),
                          _FINAL: /* @__PURE__ */ z.null(),
                        },
                      },
                    },
                    public: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        RequiresPackageId,
                        RequiresHostId,
                      ),
                      _CHILDREN: {
                        add: {
                          _PARAMS: /* @__PURE__ */ z.intersection(
                            z.intersection(RequiresPackageId, RequiresHostId),
                            AddPublicDomainParams,
                          ),
                          _RETURN: /* @__PURE__ */ AddPublicDomainRes,
                          _ITEM: /* @__PURE__ */ z.never(),
                          _FINAL: /* @__PURE__ */ z.null(),
                        },
                        remove: {
                          _PARAMS: /* @__PURE__ */ z.intersection(
                            z.intersection(RequiresPackageId, RequiresHostId),
                            RemoveDomainParams,
                          ),
                          _RETURN: /* @__PURE__ */ z.null(),
                          _ITEM: /* @__PURE__ */ z.never(),
                          _FINAL: /* @__PURE__ */ z.null(),
                        },
                      },
                    },
                  },
                },
                list: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(RequiresPackageId, RequiresHostId),
                    z.intersection(
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
                  ),
                  _RETURN: /* @__PURE__ */ z.array(HostAddress),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
              },
            },
            binding: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                RequiresPackageId,
                RequiresHostId,
              ),
              _CHILDREN: {
                list: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(RequiresPackageId, RequiresHostId),
                    z.intersection(
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
                  ),
                  _RETURN: /* @__PURE__ */ Bindings,
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                'reassign-port': {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(RequiresPackageId, RequiresHostId),
                    ReassignPortParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                'set-address-enabled': {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(RequiresPackageId, RequiresHostId),
                    BindingSetAddressEnabledParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                'set-gua-wan': {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(RequiresPackageId, RequiresHostId),
                    BindingSetGuaWanParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                'set-range-address-enabled': {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(RequiresPackageId, RequiresHostId),
                    BindingSetAddressEnabledParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
              },
            },
            list: {
              _PARAMS: /* @__PURE__ */ RequiresPackageId,
              _RETURN: /* @__PURE__ */ z.array(HostId),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        install: {
          _PARAMS: /* @__PURE__ */ InstallParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'installed-version': {
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
            InstalledVersionParams,
          ),
          _RETURN: /* @__PURE__ */ Version.nullable(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
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
          _RETURN: /* @__PURE__ */ z.array(z.unknown()),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
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
                z
                  .union([
                    z.number().int().min(-2147483648).max(2147483647),
                    z.string(),
                  ])
                  .nullable()
                  .optional(),
                z
                  .union([
                    z.number().int().min(-2147483648).max(2147483647),
                    z.string(),
                  ])
                  .nullable(),
              ),
              before: wire(z.boolean().optional(), z.boolean()),
            }),
            PackageIdParams,
          ),
          _RETURN: /* @__PURE__ */ LogResponse,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
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
                    z
                      .union([
                        z.number().int().min(-2147483648).max(2147483647),
                        z.string(),
                      ])
                      .nullable()
                      .optional(),
                    z
                      .union([
                        z.number().int().min(-2147483648).max(2147483647),
                        z.string(),
                      ])
                      .nullable(),
                  ),
                  before: wire(z.boolean().optional(), z.boolean()),
                }),
                PackageIdParams,
              ),
              _RETURN: /* @__PURE__ */ LogFollowResponse,
              _ITEM: /* @__PURE__ */ LogEntry,
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        rebuild: {
          _PARAMS: /* @__PURE__ */ RebuildParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        restart: {
          _PARAMS: /* @__PURE__ */ ControlParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'set-outbound-gateway': {
          _PARAMS: /* @__PURE__ */ SetOutboundGatewayParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        sideload: {
          _PARAMS: /* @__PURE__ */ SideloadParams,
          _RETURN: /* @__PURE__ */ SideloadResponse,
          _ITEM: /* @__PURE__ */ FullProgress,
          _FINAL: /* @__PURE__ */ z.null(),
        },
        start: {
          _PARAMS: /* @__PURE__ */ ControlParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        stats: {
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
          _RETURN: /* @__PURE__ */ z.record(
            z.string(),
            ServiceStats.nullable(),
          ),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        stop: {
          _PARAMS: /* @__PURE__ */ ControlParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        uninstall: {
          _PARAMS: /* @__PURE__ */ UninstallParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
      },
    },
    registry: {
      _PARAMS: /* @__PURE__ */ z.intersection(empty, RegistryUrlParams),
      _CHILDREN: {
        admin: {
          _PARAMS: /* @__PURE__ */ z.intersection(empty, RegistryUrlParams),
          _CHILDREN: {
            add: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                AddAdminParams,
              ),
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            list: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                z.intersection(
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
              ),
              _RETURN: /* @__PURE__ */ z.record(z.string(), SignerInfo),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            remove: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                RemoveAdminParams,
              ),
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            signer: {
              _PARAMS: /* @__PURE__ */ z.intersection(empty, RegistryUrlParams),
              _CHILDREN: {
                add: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    AddSignerParams,
                  ),
                  _RETURN: /* @__PURE__ */ Guid,
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                edit: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    EditSignerParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                list: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    z.intersection(
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
                  ),
                  _RETURN: /* @__PURE__ */ z.record(z.string(), SignerInfo),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                remove: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    RemoveSignerParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
              },
            },
          },
        },
        db: {
          _PARAMS: /* @__PURE__ */ z.intersection(empty, RegistryUrlParams),
          _CHILDREN: {
            apply: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                ApplyWithPathParams,
              ),
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            dump: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                DumpParams,
              ),
              _RETURN: /* @__PURE__ */ Dump,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            subscribe: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                SubscribeIndexParams,
              ),
              _RETURN: /* @__PURE__ */ SubscribeResult,
              _ITEM: /* @__PURE__ */ Revision,
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        index: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(empty, RegistryUrlParams),
            z.intersection(
              z.intersection(
                object({
                  format: wire(
                    IoFormat.nullable().optional(),
                    IoFormat.nullable(),
                  ),
                }),
                empty,
              ),
              ReaderParams,
            ),
          ),
          _RETURN: /* @__PURE__ */ FullIndex,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        info: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(empty, RegistryUrlParams),
            z.intersection(
              object({
                format: wire(
                  IoFormat.nullable().optional(),
                  IoFormat.nullable(),
                ),
              }),
              empty,
            ),
          ),
          _RETURN: /* @__PURE__ */ RegistryInfo,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
          _CHILDREN: {
            'set-description': {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(
                  z.intersection(empty, RegistryUrlParams),
                  z.intersection(
                    object({
                      format: wire(
                        IoFormat.nullable().optional(),
                        IoFormat.nullable(),
                      ),
                    }),
                    empty,
                  ),
                ),
                SetDescriptionParams,
              ),
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'set-icon': {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(
                  z.intersection(empty, RegistryUrlParams),
                  z.intersection(
                    object({
                      format: wire(
                        IoFormat.nullable().optional(),
                        IoFormat.nullable(),
                      ),
                    }),
                    empty,
                  ),
                ),
                SetIconParams,
              ),
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'set-name': {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(
                  z.intersection(empty, RegistryUrlParams),
                  z.intersection(
                    object({
                      format: wire(
                        IoFormat.nullable().optional(),
                        IoFormat.nullable(),
                      ),
                    }),
                    empty,
                  ),
                ),
                SetNameParams,
              ),
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'set-trust-signers': {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(
                  z.intersection(empty, RegistryUrlParams),
                  z.intersection(
                    object({
                      format: wire(
                        IoFormat.nullable().optional(),
                        IoFormat.nullable(),
                      ),
                    }),
                    empty,
                  ),
                ),
                SetTrustSignersParams,
              ),
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        metrics: {
          _PARAMS: /* @__PURE__ */ z.intersection(empty, RegistryUrlParams),
          _CHILDREN: {
            downloads: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                z.intersection(
                  z.intersection(
                    object({
                      format: wire(
                        IoFormat.nullable().optional(),
                        IoFormat.nullable(),
                      ),
                    }),
                    empty,
                  ),
                  GetDownloadsParams,
                ),
              ),
              _RETURN: /* @__PURE__ */ DownloadsResponse,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            summary: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                z.intersection(
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
              ),
              _RETURN: /* @__PURE__ */ MetricsSummary,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            users: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                z.intersection(
                  z.intersection(
                    object({
                      format: wire(
                        IoFormat.nullable().optional(),
                        IoFormat.nullable(),
                      ),
                    }),
                    empty,
                  ),
                  GetUsersParams,
                ),
              ),
              _RETURN: /* @__PURE__ */ UsersResponse,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        os: {
          _PARAMS: /* @__PURE__ */ z.intersection(empty, RegistryUrlParams),
          _CHILDREN: {
            asset: {
              _PARAMS: /* @__PURE__ */ z.intersection(empty, RegistryUrlParams),
              _CHILDREN: {
                add: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    empty,
                    RegistryUrlParams,
                  ),
                  _CHILDREN: {
                    img: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        z.intersection(empty, RegistryUrlParams),
                        AddAssetParams,
                      ),
                      _RETURN: /* @__PURE__ */ z.null(),
                      _ITEM: /* @__PURE__ */ z.never(),
                      _FINAL: /* @__PURE__ */ z.null(),
                    },
                    iso: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        z.intersection(empty, RegistryUrlParams),
                        AddAssetParams,
                      ),
                      _RETURN: /* @__PURE__ */ z.null(),
                      _ITEM: /* @__PURE__ */ z.never(),
                      _FINAL: /* @__PURE__ */ z.null(),
                    },
                    squashfs: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        z.intersection(empty, RegistryUrlParams),
                        AddAssetParams,
                      ),
                      _RETURN: /* @__PURE__ */ z.null(),
                      _ITEM: /* @__PURE__ */ z.never(),
                      _FINAL: /* @__PURE__ */ z.null(),
                    },
                  },
                },
                get: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    empty,
                    RegistryUrlParams,
                  ),
                  _CHILDREN: {
                    img: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        z.intersection(empty, RegistryUrlParams),
                        GetOsAssetParams,
                      ),
                      _RETURN: /* @__PURE__ */ RegistryAsset(Blake3Commitment),
                      _ITEM: /* @__PURE__ */ z.never(),
                      _FINAL: /* @__PURE__ */ z.null(),
                    },
                    iso: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        z.intersection(empty, RegistryUrlParams),
                        GetOsAssetParams,
                      ),
                      _RETURN: /* @__PURE__ */ RegistryAsset(Blake3Commitment),
                      _ITEM: /* @__PURE__ */ z.never(),
                      _FINAL: /* @__PURE__ */ z.null(),
                    },
                    squashfs: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        z.intersection(empty, RegistryUrlParams),
                        GetOsAssetParams,
                      ),
                      _RETURN: /* @__PURE__ */ RegistryAsset(Blake3Commitment),
                      _ITEM: /* @__PURE__ */ z.never(),
                      _FINAL: /* @__PURE__ */ z.null(),
                    },
                  },
                },
                remove: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    empty,
                    RegistryUrlParams,
                  ),
                  _CHILDREN: {
                    img: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        z.intersection(empty, RegistryUrlParams),
                        RemoveAssetParams,
                      ),
                      _RETURN: /* @__PURE__ */ z.boolean(),
                      _ITEM: /* @__PURE__ */ z.never(),
                      _FINAL: /* @__PURE__ */ z.null(),
                    },
                    iso: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        z.intersection(empty, RegistryUrlParams),
                        RemoveAssetParams,
                      ),
                      _RETURN: /* @__PURE__ */ z.boolean(),
                      _ITEM: /* @__PURE__ */ z.never(),
                      _FINAL: /* @__PURE__ */ z.null(),
                    },
                    squashfs: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        z.intersection(empty, RegistryUrlParams),
                        RemoveAssetParams,
                      ),
                      _RETURN: /* @__PURE__ */ z.boolean(),
                      _ITEM: /* @__PURE__ */ z.never(),
                      _FINAL: /* @__PURE__ */ z.null(),
                    },
                  },
                },
                sign: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    empty,
                    RegistryUrlParams,
                  ),
                  _CHILDREN: {
                    img: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        z.intersection(empty, RegistryUrlParams),
                        SignAssetParams,
                      ),
                      _RETURN: /* @__PURE__ */ z.null(),
                      _ITEM: /* @__PURE__ */ z.never(),
                      _FINAL: /* @__PURE__ */ z.null(),
                    },
                    iso: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        z.intersection(empty, RegistryUrlParams),
                        SignAssetParams,
                      ),
                      _RETURN: /* @__PURE__ */ z.null(),
                      _ITEM: /* @__PURE__ */ z.never(),
                      _FINAL: /* @__PURE__ */ z.null(),
                    },
                    squashfs: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        z.intersection(empty, RegistryUrlParams),
                        SignAssetParams,
                      ),
                      _RETURN: /* @__PURE__ */ z.null(),
                      _ITEM: /* @__PURE__ */ z.never(),
                      _FINAL: /* @__PURE__ */ z.null(),
                    },
                  },
                },
              },
            },
            index: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                z.intersection(
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
              ),
              _RETURN: /* @__PURE__ */ OsIndex,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            version: {
              _PARAMS: /* @__PURE__ */ z.intersection(empty, RegistryUrlParams),
              _CHILDREN: {
                add: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    AddVersionParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                get: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    z.intersection(
                      z.intersection(
                        object({
                          format: wire(
                            IoFormat.nullable().optional(),
                            IoFormat.nullable(),
                          ),
                        }),
                        empty,
                      ),
                      GetOsVersionParams,
                    ),
                  ),
                  _RETURN: /* @__PURE__ */ OsVersionInfoMap,
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                remove: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    RemoveVersionParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                signer: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    empty,
                    RegistryUrlParams,
                  ),
                  _CHILDREN: {
                    add: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        z.intersection(empty, RegistryUrlParams),
                        VersionSignerParams,
                      ),
                      _RETURN: /* @__PURE__ */ z.null(),
                      _ITEM: /* @__PURE__ */ z.never(),
                      _FINAL: /* @__PURE__ */ z.null(),
                    },
                    list: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        z.intersection(empty, RegistryUrlParams),
                        z.intersection(
                          z.intersection(
                            object({
                              format: wire(
                                IoFormat.nullable().optional(),
                                IoFormat.nullable(),
                              ),
                            }),
                            empty,
                          ),
                          ListVersionSignersParams,
                        ),
                      ),
                      _RETURN: /* @__PURE__ */ z.record(z.string(), SignerInfo),
                      _ITEM: /* @__PURE__ */ z.never(),
                      _FINAL: /* @__PURE__ */ z.null(),
                    },
                    remove: {
                      _PARAMS: /* @__PURE__ */ z.intersection(
                        z.intersection(empty, RegistryUrlParams),
                        VersionSignerParams,
                      ),
                      _RETURN: /* @__PURE__ */ z.null(),
                      _ITEM: /* @__PURE__ */ z.never(),
                      _FINAL: /* @__PURE__ */ z.null(),
                    },
                  },
                },
              },
            },
          },
        },
        package: {
          _PARAMS: /* @__PURE__ */ z.intersection(empty, RegistryUrlParams),
          _CHILDREN: {
            access: {
              _PARAMS: /* @__PURE__ */ z.intersection(empty, RegistryUrlParams),
              _CHILDREN: {
                'add-reader': {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    PackageReaderParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                get: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    z.intersection(
                      z.intersection(
                        object({
                          format: wire(
                            IoFormat.nullable().optional(),
                            IoFormat.nullable(),
                          ),
                        }),
                        empty,
                      ),
                      GetPackageAccessParams,
                    ),
                  ),
                  _RETURN: /* @__PURE__ */ PackageAccess,
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                'remove-reader': {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    PackageReaderParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                'set-private': {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    SetPackagePrivateParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
              },
            },
            add: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                AddPackageParams,
              ),
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'add-mirror': {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                AddMirrorParams,
              ),
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            category: {
              _PARAMS: /* @__PURE__ */ z.intersection(empty, RegistryUrlParams),
              _CHILDREN: {
                add: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    AddCategoryParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                'add-package': {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    AddPackageToCategoryParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                list: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    z.intersection(
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
                  ),
                  _RETURN: /* @__PURE__ */ z.record(z.string(), Category),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                remove: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    RemoveCategoryParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                'remove-package': {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    RemovePackageFromCategoryParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
              },
            },
            get: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                z.intersection(
                  z.intersection(
                    object({
                      format: wire(
                        IoFormat.nullable().optional(),
                        IoFormat.nullable(),
                      ),
                    }),
                    empty,
                  ),
                  GetPackageParams,
                ),
              ),
              _RETURN: /* @__PURE__ */ GetPackageResult,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            index: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                z.intersection(
                  z.intersection(
                    object({
                      format: wire(
                        IoFormat.nullable().optional(),
                        IoFormat.nullable(),
                      ),
                    }),
                    empty,
                  ),
                  ReaderParams,
                ),
              ),
              _RETURN: /* @__PURE__ */ PackageIndex,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            remove: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                RemovePackageParams,
              ),
              _RETURN: /* @__PURE__ */ z.boolean(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'remove-mirror': {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                RemoveMirrorParams,
              ),
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            sign: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(empty, RegistryUrlParams),
                SignPackageParams,
              ),
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            signer: {
              _PARAMS: /* @__PURE__ */ z.intersection(empty, RegistryUrlParams),
              _CHILDREN: {
                add: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    AddPackageSignerParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                list: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    z.intersection(
                      z.intersection(
                        object({
                          format: wire(
                            IoFormat.nullable().optional(),
                            IoFormat.nullable(),
                          ),
                        }),
                        empty,
                      ),
                      ListPackageSignersParams,
                    ),
                  ),
                  _RETURN: /* @__PURE__ */ z.record(
                    z.string(),
                    z.tuple([SignerInfo, z.string()]),
                  ),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                remove: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(empty, RegistryUrlParams),
                    RemovePackageSignerParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
              },
            },
          },
        },
      },
    },
    server: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        'clear-nut': {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'clear-smtp': {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'device-info': {
          _PARAMS: /* @__PURE__ */ z.intersection(
            object({
              format: wire(IoFormat.nullable().optional(), IoFormat.nullable()),
            }),
            empty,
          ),
          _RETURN: /* @__PURE__ */ DeviceInfo,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
          _CHILDREN: {},
        },
        epp: {
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
            EppParams,
          ),
          _RETURN: /* @__PURE__ */ EppInfo,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        experimental: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            zram: {
              _PARAMS: /* @__PURE__ */ ZramParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        governor: {
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
            GovernorParams,
          ),
          _RETURN: /* @__PURE__ */ GovernorInfo,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        host: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            address: {
              _PARAMS: /* @__PURE__ */ empty,
              _CHILDREN: {
                domain: {
                  _PARAMS: /* @__PURE__ */ empty,
                  _CHILDREN: {
                    private: {
                      _PARAMS: /* @__PURE__ */ empty,
                      _CHILDREN: {
                        add: {
                          _PARAMS: /* @__PURE__ */ AddPrivateDomainParams,
                          _RETURN: /* @__PURE__ */ z.boolean(),
                          _ITEM: /* @__PURE__ */ z.never(),
                          _FINAL: /* @__PURE__ */ z.null(),
                        },
                        remove: {
                          _PARAMS: /* @__PURE__ */ RemoveDomainParams,
                          _RETURN: /* @__PURE__ */ z.null(),
                          _ITEM: /* @__PURE__ */ z.never(),
                          _FINAL: /* @__PURE__ */ z.null(),
                        },
                      },
                    },
                    public: {
                      _PARAMS: /* @__PURE__ */ empty,
                      _CHILDREN: {
                        add: {
                          _PARAMS: /* @__PURE__ */ AddPublicDomainParams,
                          _RETURN: /* @__PURE__ */ AddPublicDomainRes,
                          _ITEM: /* @__PURE__ */ z.never(),
                          _FINAL: /* @__PURE__ */ z.null(),
                        },
                        remove: {
                          _PARAMS: /* @__PURE__ */ RemoveDomainParams,
                          _RETURN: /* @__PURE__ */ z.null(),
                          _ITEM: /* @__PURE__ */ z.never(),
                          _FINAL: /* @__PURE__ */ z.null(),
                        },
                      },
                    },
                  },
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
                  _RETURN: /* @__PURE__ */ z.array(HostAddress),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
              },
            },
            binding: {
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
                  _RETURN: /* @__PURE__ */ Bindings,
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                'reassign-port': {
                  _PARAMS: /* @__PURE__ */ ReassignPortParams,
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                'set-address-enabled': {
                  _PARAMS: /* @__PURE__ */ BindingSetAddressEnabledParams,
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                'set-gua-wan': {
                  _PARAMS: /* @__PURE__ */ BindingSetGuaWanParams,
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                'set-range-address-enabled': {
                  _PARAMS: /* @__PURE__ */ BindingSetAddressEnabledParams,
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
              },
            },
          },
        },
        'kernel-logs': {
          _PARAMS: /* @__PURE__ */ z.intersection(
            object({
              limit: wire(z.int().nullable().optional(), z.int().nullable()),
              cursor: wire(
                z.string().nullable().optional(),
                z.string().nullable(),
              ),
              boot: wire(
                z
                  .union([
                    z.number().int().min(-2147483648).max(2147483647),
                    z.string(),
                  ])
                  .nullable()
                  .optional(),
                z
                  .union([
                    z.number().int().min(-2147483648).max(2147483647),
                    z.string(),
                  ])
                  .nullable(),
              ),
              before: wire(z.boolean().optional(), z.boolean()),
            }),
            empty,
          ),
          _RETURN: /* @__PURE__ */ LogResponse,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
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
                    z
                      .union([
                        z.number().int().min(-2147483648).max(2147483647),
                        z.string(),
                      ])
                      .nullable()
                      .optional(),
                    z
                      .union([
                        z.number().int().min(-2147483648).max(2147483647),
                        z.string(),
                      ])
                      .nullable(),
                  ),
                  before: wire(z.boolean().optional(), z.boolean()),
                }),
                empty,
              ),
              _RETURN: /* @__PURE__ */ LogFollowResponse,
              _ITEM: /* @__PURE__ */ LogEntry,
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
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
                z
                  .union([
                    z.number().int().min(-2147483648).max(2147483647),
                    z.string(),
                  ])
                  .nullable()
                  .optional(),
                z
                  .union([
                    z.number().int().min(-2147483648).max(2147483647),
                    z.string(),
                  ])
                  .nullable(),
              ),
              before: wire(z.boolean().optional(), z.boolean()),
            }),
            empty,
          ),
          _RETURN: /* @__PURE__ */ LogResponse,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
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
                    z
                      .union([
                        z.number().int().min(-2147483648).max(2147483647),
                        z.string(),
                      ])
                      .nullable()
                      .optional(),
                    z
                      .union([
                        z.number().int().min(-2147483648).max(2147483647),
                        z.string(),
                      ])
                      .nullable(),
                  ),
                  before: wire(z.boolean().optional(), z.boolean()),
                }),
                empty,
              ),
              _RETURN: /* @__PURE__ */ LogFollowResponse,
              _ITEM: /* @__PURE__ */ LogEntry,
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        metrics: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            object({
              format: wire(IoFormat.nullable().optional(), IoFormat.nullable()),
            }),
            empty,
          ),
          _RETURN: /* @__PURE__ */ Metrics,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
          _CHILDREN: {
            follow: {
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
                MetricsFollowParams,
              ),
              _RETURN: /* @__PURE__ */ MetricsFollowResponse,
              _ITEM: /* @__PURE__ */ Metrics,
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        'nut-list-ups': {
          _PARAMS: /* @__PURE__ */ ListNutUpsParams,
          _RETURN: /* @__PURE__ */ z.array(NutRemoteUps),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'nut-scan': {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.array(NutDevice),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'nut-status': {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ NutStatus,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        rebuild: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        registries: {
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
                ConnectRegistryParams,
              ),
              _RETURN: /* @__PURE__ */ z.array(z.string()),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
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
              _RETURN: /* @__PURE__ */ z.record(
                z.string(),
                z.string().nullable(),
              ),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            refresh: {
              _PARAMS: /* @__PURE__ */ TrustedRegistryParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            remove: {
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
                TrustedRegistryParams,
              ),
              _RETURN: /* @__PURE__ */ z.array(z.string()),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'set-os': {
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
                TrustedRegistryParams,
              ),
              _RETURN: /* @__PURE__ */ z.array(z.string()),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        restart: {
          _PARAMS: /* @__PURE__ */ ShutdownParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'set-echoip-urls': {
          _PARAMS: /* @__PURE__ */ SetEchoipUrlsParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'set-hostname': {
          _PARAMS: /* @__PURE__ */ SetServerHostnameParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'set-keyboard': {
          _PARAMS: /* @__PURE__ */ KeyboardOptions,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'set-language': {
          _PARAMS: /* @__PURE__ */ SetLanguageParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'set-log-level': {
          _PARAMS: /* @__PURE__ */ SetLogLevelParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'set-nut': {
          _PARAMS: /* @__PURE__ */ SetNutParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'set-smtp': {
          _PARAMS: /* @__PURE__ */ SmtpValue,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        shutdown: {
          _PARAMS: /* @__PURE__ */ ShutdownParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'test-smtp': {
          _PARAMS: /* @__PURE__ */ TestSmtpParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        time: {
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
          _RETURN: /* @__PURE__ */ TimeInfo,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'trust-ca': {
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
            TrustCaParams,
          ),
          _RETURN: /* @__PURE__ */ TrustedCa,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'trusted-keys': {
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
                AddTrustedKeyParams,
              ),
              _RETURN: /* @__PURE__ */ AddTrustedKeyRes,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
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
              _RETURN: /* @__PURE__ */ z.record(z.string(), TrustedKey),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            remove: {
              _PARAMS: /* @__PURE__ */ RemoveTrustedKeyParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'remove-many': {
              _PARAMS: /* @__PURE__ */ RemoveTrustedKeysParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            update: {
              _PARAMS: /* @__PURE__ */ UpdateTrustedKeyParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        update: {
          _PARAMS: /* @__PURE__ */ UpdateSystemParams,
          _RETURN: /* @__PURE__ */ UpdateSystemRes,
          _ITEM: /* @__PURE__ */ FullProgress,
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'update-firmware': {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ RequiresReboot,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
      },
    },
    setup: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        cifs: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
        disk: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
      },
    },
    ssh: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        add: {
          _PARAMS: /* @__PURE__ */ SshAddParams,
          _RETURN: /* @__PURE__ */ SshKeyResponse,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
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
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        remove: {
          _PARAMS: /* @__PURE__ */ SshDeleteParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
      },
    },
    state: {
      _PARAMS: /* @__PURE__ */ empty,
      _RETURN: /* @__PURE__ */ ApiState,
      _ITEM: /* @__PURE__ */ z.never(),
      _FINAL: /* @__PURE__ */ z.null(),
    },
    util: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
    volumes: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        create: {
          _PARAMS: /* @__PURE__ */ CreateSharedVolumeParams,
          _RETURN: /* @__PURE__ */ SharedVolumeId,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        inspect: {
          _PARAMS: /* @__PURE__ */ SharedVolumeParams,
          _RETURN: /* @__PURE__ */ SharedVolumeInspection,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        remove: {
          _PARAMS: /* @__PURE__ */ SharedVolumeParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        rename: {
          _PARAMS: /* @__PURE__ */ RenameSharedVolumeParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'set-grant': {
          _PARAMS: /* @__PURE__ */ SetSharedVolumeGrantParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'update-cifs': {
          _PARAMS: /* @__PURE__ */ UpdateCifsSharedVolumeParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
      },
    },
    wifi: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        add: {
          _PARAMS: /* @__PURE__ */ WifiAddParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        available: {
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
                empty,
              ),
              _RETURN: /* @__PURE__ */ z.array(WifiListOut),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        connect: {
          _PARAMS: /* @__PURE__ */ WifiSsidParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        country: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            set: {
              _PARAMS: /* @__PURE__ */ SetCountryParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
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
          _RETURN: /* @__PURE__ */ WifiListInfo,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        remove: {
          _PARAMS: /* @__PURE__ */ WifiSsidParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'set-enabled': {
          _PARAMS: /* @__PURE__ */ SetWifiEnabledParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
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
