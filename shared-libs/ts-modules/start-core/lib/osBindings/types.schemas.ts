import { z } from 'zod'
const wire = <A extends z.ZodType, B extends z.ZodType>(input: A, output: B) =>
  z.codec(input, output, { decode: v => v as never, encode: v => v as never })
const object = <Shape extends z.core.$ZodLooseShape>(shape: Shape) =>
  z.object(shape).meta({ additionalProperties: true })
const empty: z.ZodType<{}, {}> = /* @__PURE__ */ object({})
const literal_0 = /* @__PURE__ */ z.string()
const literal_1 = /* @__PURE__ */ z.record(z.string(), z.unknown())
const literal_2 = /* @__PURE__ */ z.record(z.string(), z.unknown()).nullable()
const literal_3 = /* @__PURE__ */ z.union([
  z.string(),
  z.record(z.string(), z.string()),
])
const literal_4 = /* @__PURE__ */ z.union([z.string(), z.array(z.number())])
const literal_5 = /* @__PURE__ */ z.enum(['processor', 'display'])
const literal_6 = /* @__PURE__ */ z.array(z.record(z.string(), z.unknown()))
const literal_7 = /* @__PURE__ */ z.union([
  z.object({ only: z.array(z.number()) }),
  z.object({ except: z.array(z.number()) }),
])
const literal_8 = /* @__PURE__ */ z.union([
  z.object({ only: z.array(z.string()) }),
  z.object({ except: z.array(z.string()) }),
])
const literal_9 = /* @__PURE__ */ z.number()
const literal_10 = /* @__PURE__ */ z.object({
  value: z.string(),
  unit: z.string(),
})
export const AcmeProvider: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'AcmeProvider',
})
export const AcmeSettings: z.ZodObject<{ contact: z.ZodArray<z.ZodString> }> =
  /* @__PURE__ */ object({ contact: z.array(z.string()) }).meta({
    id: 'AcmeSettings',
  })
export const ActionAccess: z.ZodEnum<{
  public: 'public'
  dependent: 'dependent'
  user: 'user'
}> = /* @__PURE__ */ z.enum(['public', 'dependent', 'user']).meta({
  id: 'ActionAccess',
})
export const ActionId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'ActionId',
})
export const ActionResultV0: z.ZodObject<{
  message: z.ZodString
  value: z.ZodDefault<z.ZodNullable<z.ZodString>>
  copyable: z.ZodBoolean
  qr: z.ZodBoolean
}> = /* @__PURE__ */ object({
  message: z.string(),
  value: z
    .string()
    .nullable()
    .default(() => null),
  copyable: z.boolean(),
  qr: z.boolean(),
}).meta({
  id: 'ActionResultV0',
})
export const ActionVisibility: z.ZodUnion<
  readonly [
    z.ZodLiteral<'hidden'>,
    z.ZodObject<{ disabled: z.ZodString }>,
    z.ZodLiteral<'enabled'>,
  ]
> = /* @__PURE__ */ z
  .union([
    z.literal('hidden'),
    object({ disabled: z.string() }),
    z.literal('enabled'),
  ])
  .meta({
    id: 'ActionVisibility',
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
export const Algorithm: z.ZodEnum<{ ecdsa: 'ecdsa'; ed25519: 'ed25519' }> =
  /* @__PURE__ */ z.enum(['ecdsa', 'ed25519']).meta({
    id: 'Algorithm',
  })
export const AllowedStatuses: z.ZodEnum<{
  'only-running': 'only-running'
  'only-stopped': 'only-stopped'
  any: 'any'
}> = /* @__PURE__ */ z.enum(['only-running', 'only-stopped', 'any']).meta({
  id: 'AllowedStatuses',
})
export const AnySignature: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'AnySignature',
})
export const AnySigningKey: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'AnySigningKey',
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

/**
 * A registered-target identity; migrated CIFS targets retain their existing IDs.
 */
export const BackupTargetId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'BackupTargetId',
  description:
    'A registered-target identity; migrated CIFS targets retain their existing IDs.',
})
export const Base64: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'Base64',
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
export const BlockDev: z.ZodObject<{ logicalname: z.ZodString }> =
  /* @__PURE__ */ object({ logicalname: z.string() }).meta({
    id: 'BlockDev',
  })
export const BuildArg: z.ZodUnion<
  readonly [z.ZodString, z.ZodObject<{ env: z.ZodString }>]
> = /* @__PURE__ */ z.union([z.string(), object({ env: z.string() })]).meta({
  id: 'BuildArg',
})
export const CallbackId: typeof literal_9 = /* @__PURE__ */ literal_9.meta({
  id: 'CallbackId',
})
export const CapabilityVerdict: z.ZodObject<{
  /**
   * `None` = never probed.
   */
  supported: z.ZodDefault<z.ZodNullable<z.ZodBoolean>>
  at: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  /**
   * `None` = never probed.
   */
  supported: z
    .boolean()
    .nullable()
    .default(() => null)
    .meta({
      description: '`None` = never probed.',
    }),
  at: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'CapabilityVerdict',
})
export const Celsius: typeof literal_10 = /* @__PURE__ */ literal_10.meta({
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
  password: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  hostname: z.string(),
  path: z.string(),
  username: z.string(),
  password: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'Cifs',
})
export const CifsAddParams: z.ZodObject<{
  hostname: z.ZodString
  path: z.ZodString
  username: z.ZodString
  password: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  hostname: z.string(),
  path: z.string(),
  username: z.string(),
  password: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'CifsAddParams',
})
export const ClearCallbacksParams: typeof literal_7 =
  /* @__PURE__ */ literal_7.meta({
    id: 'ClearCallbacksParams',
  })
export const ClearTasksParams: typeof literal_8 =
  /* @__PURE__ */ literal_8.meta({
    id: 'ClearTasksParams',
  })
export const CliSetIconParams: z.ZodObject<{
  icon: z.ZodDefault<z.ZodNullable<z.ZodString>>
  clear: z.ZodBoolean
}> = /* @__PURE__ */ object({
  icon: z
    .string()
    .nullable()
    .default(() => null),
  clear: z.boolean(),
}).meta({
  id: 'CliSetIconParams',
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

/**
 * Lifecycle of a daemon-owned conversion attempt.
 */
export const ConversionState: z.ZodEnum<{
  running: 'running'
  complete: 'complete'
  failed: 'failed'
}> = /* @__PURE__ */ z.enum(['running', 'complete', 'failed']).meta({
  id: 'ConversionState',
  description: 'Lifecycle of a daemon-owned conversion attempt.',
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
   * The session id the token authenticates as, for `auth.token.remove`.
   */
  id: z.ZodString
  /**
   * The bearer secret. It is not stored and cannot be retrieved again.
   */
  token: z.ZodString
}> = /* @__PURE__ */ object({
  /**
   * The session id the token authenticates as, for `auth.token.remove`.
   */
  id: z.string().meta({
    description:
      'The session id the token authenticates as, for `auth.token.remove`.',
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
  product: z.ZodDefault<z.ZodNullable<z.ZodString>>
  vendor: z.ZodDefault<z.ZodNullable<z.ZodString>>
  capabilities: z.ZodDefault<z.ZodNullable<z.ZodArray<z.ZodString>>>
  driver: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  description: z.string(),
  class: literal_5,
  product: z
    .string()
    .nullable()
    .default(() => null),
  vendor: z
    .string()
    .nullable()
    .default(() => null),
  capabilities: z
    .array(z.string())
    .nullable()
    .default(() => null),
  driver: z
    .string()
    .nullable()
    .default(() => null),
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
export const Duration: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'Duration',
})
export const EchoParams: z.ZodObject<{ message: z.ZodString }> =
  /* @__PURE__ */ object({ message: z.string() }).meta({
    id: 'EchoParams',
  })
export const EncryptedWire: z.ZodObject<{ encrypted: z.ZodUnknown }> =
  /* @__PURE__ */ object({ encrypted: z.unknown() }).meta({
    id: 'EncryptedWire',
  })

/**
 * Selects how aggressively an EPP-capable CPU pursues performance.
 */
export const Epp: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'Epp',
  description:
    'Selects how aggressively an EPP-capable CPU pursues performance.',
})
export const ErrorData: z.ZodObject<{
  details: z.ZodString
  debug: z.ZodString
  info: z.ZodCodec<z.ZodOptional<z.ZodUnknown>, z.ZodUnknown>
}> = /* @__PURE__ */ object({
  details: z.string(),
  debug: z.string(),
  info: wire(z.unknown().optional(), z.unknown()),
}).meta({
  id: 'ErrorData',
})
export const FileCommitment: z.ZodObject<{
  /**
   * BLAKE3 digest encoded with padded standard Base64.
   */
  hash: z.ZodString
  size: z.ZodInt
}> = /* @__PURE__ */ object({
  /**
   * BLAKE3 digest encoded with padded standard Base64.
   */
  hash: z.string().meta({
    description: 'BLAKE3 digest encoded with padded standard Base64.',
  }),
  size: z.int(),
}).meta({
  id: 'FileCommitment',
})
export const FileType: z.ZodEnum<{
  file: 'file'
  directory: 'directory'
  infer: 'infer'
}> = /* @__PURE__ */ z.enum(['file', 'directory', 'infer']).meta({
  id: 'FileType',
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
  ed25519: z.ZodDefault<z.ZodBoolean>
}> = /* @__PURE__ */ object({
  hostnames: z.array(z.string()),
  ed25519: z.boolean().default(() => false),
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
  pkgId: z.ZodDefault<z.ZodNullable<z.ZodString>>
  /**
   * Filter by version
   */
  version: z.ZodDefault<z.ZodNullable<z.ZodString>>
  /**
   * Start of time range (RFC 3339)
   */
  after: z.ZodDefault<z.ZodNullable<z.ZodString>>
  /**
   * End of time range (RFC 3339)
   */
  before: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  /**
   * Filter by package ID
   */
  pkgId: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description: 'Filter by package ID',
    }),
  /**
   * Filter by version
   */
  version: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description: 'Filter by version',
    }),
  /**
   * Start of time range (RFC 3339)
   */
  after: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description: 'Start of time range (RFC 3339)',
    }),
  /**
   * End of time range (RFC 3339)
   */
  before: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
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
  sourceVersion: z.ZodDefault<z.ZodNullable<z.ZodString>>
  targetVersion: z.ZodDefault<z.ZodNullable<z.ZodString>>
  serverId: z.ZodDefault<z.ZodNullable<z.ZodString>>
  platform: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  sourceVersion: z
    .string()
    .nullable()
    .default(() => null),
  targetVersion: z
    .string()
    .nullable()
    .default(() => null),
  serverId: z
    .string()
    .nullable()
    .default(() => null),
  platform: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'GetOsVersionParams',
})
export const GetUsersParams: z.ZodObject<{
  /**
   * Start of time range (RFC 3339)
   */
  after: z.ZodDefault<z.ZodNullable<z.ZodString>>
  /**
   * End of time range (RFC 3339)
   */
  before: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  /**
   * Start of time range (RFC 3339)
   */
  after: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description: 'Start of time range (RFC 3339)',
    }),
  /**
   * End of time range (RFC 3339)
   */
  before: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description: 'End of time range (RFC 3339)',
    }),
}).meta({
  id: 'GetUsersParams',
})
export const GigaBytes: typeof literal_10 = /* @__PURE__ */ literal_10.meta({
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
  identities: z.ZodDefault<z.ZodRecord<z.ZodString, z.ZodArray<z.ZodString>>>
  /**
   * OpenPGP fingerprints indexed by the signing key's canonical DER encoding.
   */
  keyFingerprints: z.ZodDefault<
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
  identities: z
    .record(z.string(), z.array(z.string()))
    .default(() => ({}))
    .meta({
      description:
        "Self-certified claims indexed by the signing key's canonical DER encoding.",
    }),
  /**
   * OpenPGP fingerprints indexed by the signing key's canonical DER encoding.
   */
  keyFingerprints: z
    .record(z.string(), z.array(z.string()))
    .default(() => ({}))
    .meta({
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
export const HealthCheckId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'HealthCheckId',
})
export const HostId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'HostId',
})
export const IdMap: z.ZodObject<{
  fromId: z.ZodInt
  toId: z.ZodInt
  range: z.ZodInt
}> = /* @__PURE__ */ object({
  fromId: z.int(),
  toId: z.int(),
  range: z.int(),
}).meta({
  id: 'IdMap',
})
export const ImageId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'ImageId',
})
export const ImageMetadata: z.ZodObject<{
  /**
   * Image environment defaults. Absent in packages using a legacy `.env` file.
   */
  env: z.ZodOptional<
    z.ZodNullable<z.ZodRecord<z.ZodString, z.ZodNullable<z.ZodString>>>
  >
  workdir: z.ZodString
  user: z.ZodString
  entrypoint: z.ZodDefault<z.ZodNullable<z.ZodArray<z.ZodString>>>
  cmd: z.ZodDefault<z.ZodNullable<z.ZodArray<z.ZodString>>>
}> = /* @__PURE__ */ object({
  /**
   * Image environment defaults. Absent in packages using a legacy `.env` file.
   */
  env: z.record(z.string(), z.string().nullable()).nullable().optional().meta({
    description:
      'Image environment defaults. Absent in packages using a legacy `.env` file.',
  }),
  workdir: z.string(),
  user: z.string(),
  entrypoint: z
    .array(z.string())
    .nullable()
    .default(() => null),
  cmd: z
    .array(z.string())
    .nullable()
    .default(() => null),
}).meta({
  id: 'ImageMetadata',
})

/**
 * An AES-256-CTR keystream with a 128-bit big-endian counter starting at `iv`.
 *
 * Ciphertext has the plaintext's length, and any byte range decrypts on its own.
 */
export const KeyStreamInfo: z.ZodObject<{
  key: typeof literal_0
  iv: typeof literal_0
}> = /* @__PURE__ */ object({ key: literal_0, iv: literal_0 }).meta({
  id: 'KeyStreamInfo',
  description:
    "An AES-256-CTR keystream with a 128-bit big-endian counter starting at `iv`.\n\nCiphertext has the plaintext's length, and any byte range decrypts on its own.",
})
export const KeyboardOptions: z.ZodObject<{
  layout: z.ZodString
  keymap: z.ZodDefault<z.ZodNullable<z.ZodString>>
  model: z.ZodDefault<z.ZodNullable<z.ZodString>>
  variant: z.ZodDefault<z.ZodNullable<z.ZodString>>
  options: z.ZodDefault<z.ZodArray<z.ZodString>>
}> = /* @__PURE__ */ object({
  layout: z.string(),
  keymap: z
    .string()
    .nullable()
    .default(() => null),
  model: z
    .string()
    .nullable()
    .default(() => null),
  variant: z
    .string()
    .nullable()
    .default(() => null),
  options: z.array(z.string()).default(() => []),
}).meta({
  id: 'KeyboardOptions',
})
export const LegacyHealthCheckResultKind: z.ZodIntersection<
  z.ZodObject<{ result: z.ZodLiteral<'waiting'> }>,
  z.ZodObject<{ message: z.ZodDefault<z.ZodNullable<z.ZodString>> }>
> = /* @__PURE__ */ z
  .intersection(
    object({ result: z.literal('waiting') }),
    object({
      message: z
        .string()
        .nullable()
        .default(() => null),
    }),
  )
  .meta({
    id: 'LegacyHealthCheckResultKind',
  })
export const ListNotificationParams: z.ZodObject<{
  before: z.ZodDefault<z.ZodNullable<z.ZodInt>>
  limit: z.ZodDefault<z.ZodNullable<z.ZodInt>>
}> = /* @__PURE__ */ object({
  before: z
    .int()
    .nullable()
    .default(() => null),
  limit: z
    .int()
    .nullable()
    .default(() => null),
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
export const ListVersionSignersParams: z.ZodObject<{ version: z.ZodString }> =
  /* @__PURE__ */ object({ version: z.string() }).meta({
    id: 'ListVersionSignersParams',
  })
export const ListedVersion: z.ZodObject<{
  /**
   * Raw `s9pks` positions in the installer's hardware-preference order.
   */
  s9pks: z.ZodDefault<z.ZodArray<z.ZodInt>>
  /**
   * A newer same-flavor revision covers this release's hardware.
   */
  superseded: z.ZodOptional<z.ZodBoolean>
  /**
   * This StartOS can install the release; others need a StartOS update first.
   */
  compatible: z.ZodBoolean
}> = /* @__PURE__ */ object({
  /**
   * Raw `s9pks` positions in the installer's hardware-preference order.
   */
  s9pks: z
    .array(z.int())
    .default(() => [])
    .meta({
      description:
        "Raw `s9pks` positions in the installer's hardware-preference order.",
    }),
  /**
   * A newer same-flavor revision covers this release's hardware.
   */
  superseded: z.boolean().optional().meta({
    description: "A newer same-flavor revision covers this release's hardware.",
  }),
  /**
   * This StartOS can install the release; others need a StartOS update first.
   */
  compatible: z.boolean().meta({
    description:
      'This StartOS can install the release; others need a StartOS update first.',
  }),
}).meta({
  id: 'ListedVersion',
})
export const LocaleString: typeof literal_3 = /* @__PURE__ */ literal_3.meta({
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
export const LogsParams: z.ZodIntersection<
  z.ZodObject<{
    limit: z.ZodDefault<z.ZodNullable<z.ZodInt>>
    cursor: z.ZodDefault<z.ZodNullable<z.ZodString>>
    boot: z.ZodDefault<
      z.ZodNullable<z.ZodUnion<readonly [z.ZodNumber, z.ZodString]>>
    >
    before: z.ZodDefault<z.ZodBoolean>
  }>,
  typeof empty
> = /* @__PURE__ */ z
  .intersection(
    object({
      limit: z
        .int()
        .nullable()
        .default(() => null),
      cursor: z
        .string()
        .nullable()
        .default(() => null),
      boot: z
        .union([z.number().int().min(-2147483648).max(2147483647), z.string()])
        .nullable()
        .default(() => null),
      before: z.boolean().default(() => false),
    }),
    empty,
  )
  .meta({
    id: 'LogsParams',
  })
export const LshwDisplay: z.ZodObject<{
  product: z.ZodDefault<z.ZodNullable<z.ZodString>>
  vendor: z.ZodDefault<z.ZodNullable<z.ZodString>>
  capabilities: z.ZodArray<z.ZodString>
  driver: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  product: z
    .string()
    .nullable()
    .default(() => null),
  vendor: z
    .string()
    .nullable()
    .default(() => null),
  capabilities: z.array(z.string()),
  driver: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'LshwDisplay',
})
export const LshwProcessor: z.ZodObject<{
  product: z.ZodDefault<z.ZodNullable<z.ZodString>>
  vendor: z.ZodDefault<z.ZodNullable<z.ZodString>>
  capabilities: z.ZodArray<z.ZodString>
}> = /* @__PURE__ */ object({
  product: z
    .string()
    .nullable()
    .default(() => null),
  vendor: z
    .string()
    .nullable()
    .default(() => null),
  capabilities: z.array(z.string()),
}).meta({
  id: 'LshwProcessor',
})
export const MaybeUtf8String: typeof literal_4 = /* @__PURE__ */ literal_4.meta(
  {
    id: 'MaybeUtf8String',
  },
)
export const MebiBytes: typeof literal_10 = /* @__PURE__ */ literal_10.meta({
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
export const ModifyNotificationBeforeParams: z.ZodObject<{ before: z.ZodInt }> =
  /* @__PURE__ */ object({ before: z.int() }).meta({
    id: 'ModifyNotificationBeforeParams',
  })
export const ModifyNotificationParams: z.ZodObject<{
  ids: z.ZodArray<z.ZodInt>
}> = /* @__PURE__ */ object({ ids: z.array(z.int()) }).meta({
  id: 'ModifyNotificationParams',
})
export const NamedHealthCheckResultKind: z.ZodUnion<
  readonly [
    z.ZodIntersection<
      z.ZodObject<{ result: z.ZodLiteral<'success'> }>,
      z.ZodObject<{ message: z.ZodDefault<z.ZodNullable<z.ZodString>> }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ result: z.ZodLiteral<'disabled'> }>,
      z.ZodObject<{ message: z.ZodDefault<z.ZodNullable<z.ZodString>> }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ result: z.ZodLiteral<'starting'> }>,
      z.ZodObject<{
        message: z.ZodDefault<z.ZodNullable<z.ZodString>>
        waiting: z.ZodDefault<z.ZodBoolean>
      }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ result: z.ZodLiteral<'loading'> }>,
      z.ZodObject<{ message: z.ZodString; waiting: z.ZodDefault<z.ZodBoolean> }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ result: z.ZodLiteral<'failure'> }>,
      z.ZodObject<{ message: z.ZodString }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(
      object({ result: z.literal('success') }),
      object({
        message: z
          .string()
          .nullable()
          .default(() => null),
      }),
    ),
    z.intersection(
      object({ result: z.literal('disabled') }),
      object({
        message: z
          .string()
          .nullable()
          .default(() => null),
      }),
    ),
    z.intersection(
      object({ result: z.literal('starting') }),
      object({
        message: z
          .string()
          .nullable()
          .default(() => null),
        waiting: z.boolean().default(() => false),
      }),
    ),
    z.intersection(
      object({ result: z.literal('loading') }),
      object({
        message: z.string(),
        waiting: z.boolean().default(() => false),
      }),
    ),
    z.intersection(
      object({ result: z.literal('failure') }),
      object({ message: z.string() }),
    ),
  ])
  .meta({
    id: 'NamedHealthCheckResultKind',
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
  assignedPort: z.ZodDefault<z.ZodNullable<z.ZodInt>>
  assignedSslPort: z.ZodDefault<z.ZodNullable<z.ZodInt>>
}> = /* @__PURE__ */ object({
  assignedPort: z
    .int()
    .nullable()
    .default(() => null),
  assignedSslPort: z
    .int()
    .nullable()
    .default(() => null),
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
  vendorId: z.ZodDefault<z.ZodNullable<z.ZodString>>
  productId: z.ZodDefault<z.ZodNullable<z.ZodString>>
  serial: z.ZodDefault<z.ZodNullable<z.ZodString>>
  bus: z.ZodDefault<z.ZodNullable<z.ZodString>>
  /**
   * Pins devices without a serial number to the selected USB port.
   */
  busPort: z.ZodDefault<z.ZodNullable<z.ZodString>>
  vendor: z.ZodDefault<z.ZodNullable<z.ZodString>>
  product: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  driver: z.string(),
  port: z.string(),
  vendorId: z
    .string()
    .nullable()
    .default(() => null),
  productId: z
    .string()
    .nullable()
    .default(() => null),
  serial: z
    .string()
    .nullable()
    .default(() => null),
  bus: z
    .string()
    .nullable()
    .default(() => null),
  /**
   * Pins devices without a serial number to the selected USB port.
   */
  busPort: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description:
        'Pins devices without a serial number to the selected USB port.',
    }),
  vendor: z
    .string()
    .nullable()
    .default(() => null),
  product: z
    .string()
    .nullable()
    .default(() => null),
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
  monitoringError: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  target: z.string(),
  variables: z.record(z.string(), z.string()),
  monitoringError: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'NutStatus',
  description: 'Current variables reported by the configured UPS.',
})

/**
 * An OS update available for review.
 */
export const OsUpdateAvailable: z.ZodIntersection<
  z.ZodObject<{
    kind: z.ZodCodec<
      z.ZodOptional<z.ZodLiteral<'os-update-available'>>,
      z.ZodLiteral<'os-update-available'>
    >
  }>,
  typeof empty
> = /* @__PURE__ */ z
  .intersection(
    object({
      kind: wire(
        z.literal('os-update-available').optional(),
        z.literal('os-update-available'),
      ),
    }),
    empty,
  )
  .meta({
    id: 'OsUpdateAvailable',
    description: 'An OS update available for review.',
  })

/**
 * An absent update preserves DNS intent; null servers select gateway DNS.
 */
export const OutboundDnsUpdate: z.ZodObject<{
  servers: z.ZodDefault<z.ZodNullable<z.ZodArray<z.ZodString>>>
}> = /* @__PURE__ */ object({
  servers: z
    .array(z.string())
    .nullable()
    .default(() => null),
}).meta({
  id: 'OutboundDnsUpdate',
  description:
    'An absent update preserves DNS intent; null servers select gateway DNS.',
})
export const PackageBackupReport: z.ZodObject<{
  error: z.ZodDefault<z.ZodNullable<z.ZodString>>
  duration_ms: z.ZodInt
}> = /* @__PURE__ */ object({
  error: z
    .string()
    .nullable()
    .default(() => null),
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
export const PartitionTable: z.ZodEnum<{ mbr: 'mbr'; gpt: 'gpt' }> =
  /* @__PURE__ */ z.enum(['mbr', 'gpt']).meta({
    id: 'PartitionTable',
  })
export const PathOrUrl: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'PathOrUrl',
})
export const Pem: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'Pem',
})
export const Percentage: typeof literal_10 = /* @__PURE__ */ literal_10.meta({
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
  ipv4: z.ZodDefault<z.ZodNullable<z.ZodString>>
  ipv6: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  ipv4: z
    .string()
    .nullable()
    .default(() => null),
  ipv6: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'QueryDnsRes',
  description:
    "What a public domain currently resolves to, per address family. A public\ndomain is DualStack: the operator points an `A` record at the gateway's WAN\nIPv4 and an `AAAA` at the box's IPv6 GUA. Either may be absent.",
})
export const ReaderParams: typeof empty = /* @__PURE__ */ empty.meta({
  id: 'ReaderParams',
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
    /**
     * Keystream the hosted bytes are encrypted with. Null for hosted plaintext visible to everyone.
     */
    private: KeyStreamInfo.nullable().optional().meta({
      description:
        'Keystream the hosted bytes are encrypted with. Null for hosted plaintext visible to everyone.',
    }),
    /**
     * Reader keys in `FullIndex.readers` allowed to see this private asset.
     */
    readers: z.array(Guid).optional().meta({
      description:
        'Reader keys in `FullIndex.readers` allowed to see this private asset.',
    }),
  })
  return self
}

/**
 * A key a connected registry newly offers, imported untrusted.
 */
export const RegistryKeyAdded: z.ZodIntersection<
  z.ZodObject<{
    kind: z.ZodCodec<
      z.ZodOptional<z.ZodLiteral<'registry-key-added'>>,
      z.ZodLiteral<'registry-key-added'>
    >
  }>,
  z.ZodObject<{ registry: z.ZodString; key: z.ZodString }>
> = /* @__PURE__ */ z
  .intersection(
    object({
      kind: wire(
        z.literal('registry-key-added').optional(),
        z.literal('registry-key-added'),
      ),
    }),
    object({ registry: z.string(), key: z.string() }),
  )
  .meta({
    id: 'RegistryKeyAdded',
    description: 'A key a connected registry newly offers, imported untrusted.',
  })

/**
 * A key a connected registry stopped offering; its trust is unchanged.
 */
export const RegistryKeyDropped: z.ZodIntersection<
  z.ZodObject<{
    kind: z.ZodCodec<
      z.ZodOptional<z.ZodLiteral<'registry-key-dropped'>>,
      z.ZodLiteral<'registry-key-dropped'>
    >
  }>,
  z.ZodObject<{ registry: z.ZodString; key: z.ZodString }>
> = /* @__PURE__ */ z
  .intersection(
    object({
      kind: wire(
        z.literal('registry-key-dropped').optional(),
        z.literal('registry-key-dropped'),
      ),
    }),
    object({ registry: z.string(), key: z.string() }),
  )
  .meta({
    id: 'RegistryKeyDropped',
    description:
      'A key a connected registry stopped offering; its trust is unchanged.',
  })
export const RegistryStatus: z.ZodEnum<{
  connecting: 'connecting'
  connected: 'connected'
  error: 'error'
}> = /* @__PURE__ */ z.enum(['connecting', 'connected', 'error']).meta({
  id: 'RegistryStatus',
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
export const RemoveOsVulnerabilityParams: z.ZodObject<{
  vulnerabilityId: z.ZodString
}> = /* @__PURE__ */ object({ vulnerabilityId: z.string() }).meta({
  id: 'RemoveOsVulnerabilityParams',
})
export const RemoveTokensParams: z.ZodObject<{
  tokens: z.ZodArray<z.ZodString>
}> = /* @__PURE__ */ object({ tokens: z.array(z.string()) }).meta({
  id: 'RemoveTokensParams',
})
export const RemoveVersionParams: z.ZodObject<{ version: z.ZodString }> =
  /* @__PURE__ */ object({ version: z.string() }).meta({
    id: 'RemoveVersionParams',
  })
export const ReplayId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'ReplayId',
})
export const RequestCommitment: z.ZodIntersection<
  z.ZodObject<{
    type: z.ZodCodec<
      z.ZodOptional<z.ZodLiteral<'request'>>,
      z.ZodLiteral<'request'>
    >
  }>,
  z.ZodObject<{
    timestamp: z.ZodInt
    nonce: z.ZodInt
    size: z.ZodInt
    blake3: typeof literal_0
  }>
> = /* @__PURE__ */ z
  .intersection(
    object({
      type: wire(z.literal('request').optional(), z.literal('request')),
    }),
    object({
      timestamp: z.int(),
      nonce: z.int(),
      size: z.int(),
      blake3: literal_0,
    }),
  )
  .meta({
    id: 'RequestCommitment',
  })
export const RestartReason: z.ZodEnum<{
  mdns: 'mdns'
  language: 'language'
  kiosk: 'kiosk'
  update: 'update'
}> = /* @__PURE__ */ z.enum(['mdns', 'language', 'kiosk', 'update']).meta({
  id: 'RestartReason',
})
export const Security: z.ZodObject<{ ssl: z.ZodBoolean }> =
  /* @__PURE__ */ object({ ssl: z.boolean() }).meta({
    id: 'Security',
  })
export const SensorKind: z.ZodEnum<{
  cpu: 'cpu'
  gpu: 'gpu'
  memory: 'memory'
  storage: 'storage'
  network: 'network'
  chipset: 'chipset'
  system: 'system'
  other: 'other'
}> = /* @__PURE__ */ z
  .enum([
    'cpu',
    'gpu',
    'memory',
    'storage',
    'network',
    'chipset',
    'system',
    'other',
  ])
  .meta({
    id: 'SensorKind',
  })
export const ServerBackupReport: z.ZodObject<{
  attempted: z.ZodBoolean
  error: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  attempted: z.boolean(),
  error: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'ServerBackupReport',
})
export const ServerHostname: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'ServerHostname',
})
export const ServerSpecs: z.ZodObject<{
  cpu: z.ZodString
  disk: z.ZodString
  memory: z.ZodString
}> = /* @__PURE__ */ object({
  cpu: z.string(),
  disk: z.string(),
  memory: z.string(),
}).meta({
  id: 'ServerSpecs',
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
  name: z.ZodDefault<z.ZodNullable<z.ZodString>>
  loggedIn: z.ZodCodec<z.ZodOptional<z.ZodString>, z.ZodString>
  lastActive: z.ZodCodec<z.ZodOptional<z.ZodString>, z.ZodString>
  userAgent: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  /**
   * A friendly name for the key, if one was assigned at enrollment (e.g.
   * tunnel device keys). UI-enrolled keys are unnamed.
   */
  name: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description:
        'A friendly name for the key, if one was assigned at enrollment (e.g.\ntunnel device keys). UI-enrolled keys are unnamed.',
    }),
  loggedIn: wire(z.string().optional(), z.string()),
  lastActive: wire(z.string().optional(), z.string()),
  userAgent: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'Session',
})
export const SetCountryParams: z.ZodObject<{ country: z.ZodString }> =
  /* @__PURE__ */ object({ country: z.string() }).meta({
    id: 'SetCountryParams',
  })
export const SetDataVersionParams: z.ZodObject<{
  version: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  version: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'SetDataVersionParams',
})
export const SetDefaultLocaleParams: z.ZodObject<{
  defaultLocale: z.ZodNullable<z.ZodString>
}> = /* @__PURE__ */ object({ defaultLocale: z.string().nullable() }).meta({
  id: 'SetDefaultLocaleParams',
})
export const SetLanguageParams: z.ZodObject<{ language: z.ZodString }> =
  /* @__PURE__ */ object({ language: z.string() }).meta({
    id: 'SetLanguageParams',
  })
export const SetMainStatusStatus: z.ZodEnum<{
  running: 'running'
  stopped: 'stopped'
}> = /* @__PURE__ */ z.enum(['running', 'stopped']).meta({
  id: 'SetMainStatusStatus',
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
export const SetupInfo: z.ZodObject<{
  guid: z.ZodDefault<z.ZodNullable<z.ZodString>>
  attach: z.ZodBoolean
  mokEnrolled: z.ZodBoolean
  /**
   * The whole disk the OS is installed on, recorded by whatever installed it
   * (os_install, or init_resize for a pre-installed image). When the device
   * is pre-installed the wizard fixes this as the OS drive and asks only for
   * a data drive.
   */
  osDrive: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  guid: z
    .string()
    .nullable()
    .default(() => null),
  attach: z.boolean(),
  mokEnrolled: z.boolean(),
  /**
   * The whole disk the OS is installed on, recorded by whatever installed it
   * (os_install, or init_resize for a pre-installed image). When the device
   * is pre-installed the wizard fixes this as the OS drive and asks only for
   * a data drive.
   */
  osDrive: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description:
        'The whole disk the OS is installed on, recorded by whatever installed it\n(os_install, or init_resize for a pre-installed image). When the device\nis pre-installed the wizard fixes this as the OS drive and asks only for\na data drive.',
    }),
}).meta({
  id: 'SetupInfo',
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
  vendor: z.ZodDefault<z.ZodNullable<z.ZodString>>
  model: z.ZodDefault<z.ZodNullable<z.ZodString>>
  label: z.ZodDefault<z.ZodNullable<z.ZodString>>
  capacity: z.ZodInt
}> = /* @__PURE__ */ object({
  vendor: z
    .string()
    .nullable()
    .default(() => null),
  model: z
    .string()
    .nullable()
    .default(() => null),
  label: z
    .string()
    .nullable()
    .default(() => null),
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
export const SharedVolumeUnavailableReason: z.ZodEnum<{
  sourceUnavailable: 'sourceUnavailable'
  timeout: 'timeout'
}> = /* @__PURE__ */ z.enum(['sourceUnavailable', 'timeout']).meta({
  id: 'SharedVolumeUnavailableReason',
})
export const ShutdownParams: z.ZodObject<{
  /**
   * Block until graceful teardown completes (default over the CLI; the
   * frontend omits this and gets an immediate reply). Cleared with
   * `--nowait`. The wait can't outlive the webserver teardown that follows
   * container shutdown, so the connection drops once services are stopped.
   */
  wait: z.ZodDefault<z.ZodBoolean>
}> = /* @__PURE__ */ object({
  /**
   * Block until graceful teardown completes (default over the CLI; the
   * frontend omits this and gets an immediate reply). Cleared with
   * `--nowait`. The wait can't outlive the webserver teardown that follows
   * container shutdown, so the connection drops once services are stopped.
   */
  wait: z
    .boolean()
    .default(() => false)
    .meta({
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
export const StartStop: z.ZodEnum<{ start: 'start'; stop: 'stop' }> =
  /* @__PURE__ */ z.enum(['start', 'stop']).meta({
    id: 'StartStop',
  })

/**
 * RPC reply metadata for live database synchronization.
 */
export const SyncDbMetadata: z.ZodObject<{
  /**
   * Database revision the caller must observe before using the result.
   */
  patchSequence: z.ZodOptional<z.ZodNullable<z.ZodInt>>
}> = /* @__PURE__ */ object({
  /**
   * Database revision the caller must observe before using the result.
   */
  patchSequence: z.int().nullable().optional().meta({
    description:
      'Database revision the caller must observe before using the result.',
  }),
}).meta({
  id: 'SyncDbMetadata',
  description: 'RPC reply metadata for live database synchronization.',
})
export const TaskCondition: z.ZodEnum<{
  'input-not-matches': 'input-not-matches'
}> = /* @__PURE__ */ z.enum(['input-not-matches']).meta({
  id: 'TaskCondition',
})
export const TaskInputRepr: z.ZodIntersection<
  z.ZodObject<{ kind: z.ZodLiteral<'partial'> }>,
  z.ZodObject<{
    accept: z.ZodDefault<z.ZodNullable<z.ZodArray<z.ZodUnknown>>>
    set: z.ZodDefault<z.ZodNullable<z.ZodUnknown>>
    value: z.ZodDefault<z.ZodNullable<z.ZodUnknown>>
  }>
> = /* @__PURE__ */ z
  .intersection(
    object({ kind: z.literal('partial') }),
    object({
      accept: z
        .array(z.unknown())
        .nullable()
        .default(() => null),
      set: z
        .unknown()
        .nullable()
        .default(() => null),
      value: z
        .unknown()
        .nullable()
        .default(() => null),
    }),
  )
  .meta({
    id: 'TaskInputRepr',
  })
export const TaskSeverity: z.ZodEnum<{
  optional: 'optional'
  important: 'important'
  critical: 'critical'
}> = /* @__PURE__ */ z.enum(['optional', 'important', 'critical']).meta({
  id: 'TaskSeverity',
})
export const TimeInfo: z.ZodObject<{ now: z.ZodString; uptime: z.ZodInt }> =
  /* @__PURE__ */ object({ now: z.string(), uptime: z.int() }).meta({
    id: 'TimeInfo',
  })
export const TransportProtocol: z.ZodEnum<{ tcp: 'tcp'; udp: 'udp' }> =
  /* @__PURE__ */ z.enum(['tcp', 'udp']).meta({
    id: 'TransportProtocol',
  })
export const TrustContext: z.ZodEnum<{ marketplace: 'marketplace'; os: 'os' }> =
  /* @__PURE__ */ z.enum(['marketplace', 'os']).meta({
    id: 'TrustContext',
  })
export const TrustedOsUpdateSigner: z.ZodObject<{ label: z.ZodString }> =
  /* @__PURE__ */ object({ label: z.string() }).meta({
    id: 'TrustedOsUpdateSigner',
  })
export const UploadLimits: z.ZodObject<{
  /**
   * Maximum uploaded bytes, inclusive; null leaves the size unbounded.
   */
  maxSize: z.ZodDefault<z.ZodNullable<z.ZodInt>>
}> = /* @__PURE__ */ object({
  /**
   * Maximum uploaded bytes, inclusive; null leaves the size unbounded.
   */
  maxSize: z
    .int()
    .nullable()
    .default(() => null)
    .meta({
      description:
        'Maximum uploaded bytes, inclusive; null leaves the size unbounded.',
    }),
}).meta({
  id: 'UploadLimits',
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
export const VolumeId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'VolumeId',
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
  spec: typeof literal_1
  value: z.ZodCodec<z.ZodOptional<typeof literal_2>, typeof literal_2>
}> = /* @__PURE__ */ object({
  eventId: wire(Guid.optional(), Guid),
  spec: literal_1,
  value: wire(literal_2.optional(), literal_2),
}).meta({
  id: 'ActionInput',
})
export const ActionMetadata: z.ZodObject<{
  /**
   * A human-readable name
   */
  name: z.ZodString
  /**
   * A detailed description of what the action will do
   */
  description: z.ZodString
  /**
   * Presents as an alert prior to executing the action. Should be used sparingly but important if the action could have harmful, unintended consequences
   */
  warning: z.ZodDefault<z.ZodNullable<z.ZodString>>
  /**
   * One of: "enabled", "hidden", or { disabled: "" }
   *   - "enabled" - the action is available be run
   *   - "hidden" - the action cannot be seen or run
   *   - { disabled: "example explanation" } means the action is visible but cannot be run. Replace "example explanation" with a reason why the action is disable to prevent user confusion.
   */
  visibility: z.ZodCodec<
    z.ZodOptional<typeof ActionVisibility>,
    typeof ActionVisibility
  >
  /**
   * Service run states that permit this action.
   */
  allowedStatuses: typeof AllowedStatuses
  hasInput: z.ZodBoolean
  /**
   * If provided, this action will be nested under a header of this value, along with other actions of the same group
   */
  group: z.ZodDefault<z.ZodNullable<z.ZodString>>
  /**
   * Who is allowed to invoke this action directly via `effects.action.run`.
   *   - "public" — any installed package
   *   - "dependent" — only services that declare this package as a current dependency
   *   - "user" — only the user (other services must create a task). Default when omitted.
   * Services that lack direct access can always queue a task with `effects.action.createTask`.
   */
  access: z.ZodDefault<z.ZodNullable<typeof ActionAccess>>
}> = /* @__PURE__ */ object({
  /**
   * A human-readable name
   */
  name: z.string().meta({
    description: 'A human-readable name',
  }),
  /**
   * A detailed description of what the action will do
   */
  description: z.string().meta({
    description: 'A detailed description of what the action will do',
  }),
  /**
   * Presents as an alert prior to executing the action. Should be used sparingly but important if the action could have harmful, unintended consequences
   */
  warning: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description:
        'Presents as an alert prior to executing the action. Should be used sparingly but important if the action could have harmful, unintended consequences',
    }),
  /**
   * One of: "enabled", "hidden", or { disabled: "" }
   *   - "enabled" - the action is available be run
   *   - "hidden" - the action cannot be seen or run
   *   - { disabled: "example explanation" } means the action is visible but cannot be run. Replace "example explanation" with a reason why the action is disable to prevent user confusion.
   */
  visibility: wire(ActionVisibility.optional(), ActionVisibility).meta({
    description:
      'One of: "enabled", "hidden", or { disabled: "" }\n  - "enabled" - the action is available be run\n  - "hidden" - the action cannot be seen or run\n  - { disabled: "example explanation" } means the action is visible but cannot be run. Replace "example explanation" with a reason why the action is disable to prevent user confusion.',
  }),
  /**
   * Service run states that permit this action.
   */
  allowedStatuses: AllowedStatuses.meta({
    description: 'Service run states that permit this action.',
  }),
  hasInput: z.boolean(),
  /**
   * If provided, this action will be nested under a header of this value, along with other actions of the same group
   */
  group: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description:
        'If provided, this action will be nested under a header of this value, along with other actions of the same group',
    }),
  /**
   * Who is allowed to invoke this action directly via `effects.action.run`.
   *   - "public" — any installed package
   *   - "dependent" — only services that declare this package as a current dependency
   *   - "user" — only the user (other services must create a task). Default when omitted.
   * Services that lack direct access can always queue a task with `effects.action.createTask`.
   */
  access: ActionAccess.nullable()
    .default(() => null)
    .meta({
      description:
        'Who is allowed to invoke this action directly via `effects.action.run`.\n  - "public" — any installed package\n  - "dependent" — only services that declare this package as a current dependency\n  - "user" — only the user (other services must create a task). Default when omitted.\nServices that lack direct access can always queue a task with `effects.action.createTask`.',
    }),
}).meta({
  id: 'ActionMetadata',
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
        copyable: z.ZodDefault<z.ZodNullable<z.ZodBoolean>>
        /**
         * (optional) Whether or not to also display the value as a QR code
         */
        qr: z.ZodDefault<z.ZodNullable<z.ZodBoolean>>
        /**
         * (optional) Whether or not to mask the value using ●●●●●●●, which is useful for password or other sensitive information
         */
        masked: z.ZodDefault<z.ZodNullable<z.ZodBoolean>>
        /**
         * (optional) Whether or not to include an open in new tab icon to launch the value, which must be an http(s) URL
         */
        launchable: z.ZodDefault<z.ZodNullable<z.ZodBoolean>>
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
        copyable: z.ZodDefault<z.ZodNullable<z.ZodBoolean>>
        /**
         * (optional) Whether or not to also display the value as a QR code
         */
        qr: z.ZodDefault<z.ZodNullable<z.ZodBoolean>>
        /**
         * (optional) Whether or not to blur the value until the user reveals it, which is useful for a private key or other sensitive information
         */
        masked: z.ZodDefault<z.ZodNullable<z.ZodBoolean>>
        /**
         * (optional) Also offer the value as a download under this file name, such as "diagnostics.txt"
         */
        filename: z.ZodDefault<z.ZodNullable<z.ZodString>>
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
        copyable: z
          .boolean()
          .nullable()
          .default(() => null)
          .meta({
            description:
              '(optional) Whether or not to include a copy to clipboard icon to copy the value',
          }),
        /**
         * (optional) Whether or not to also display the value as a QR code
         */
        qr: z
          .boolean()
          .nullable()
          .default(() => null)
          .meta({
            description:
              '(optional) Whether or not to also display the value as a QR code',
          }),
        /**
         * (optional) Whether or not to mask the value using ●●●●●●●, which is useful for password or other sensitive information
         */
        masked: z
          .boolean()
          .nullable()
          .default(() => null)
          .meta({
            description:
              '(optional) Whether or not to mask the value using ●●●●●●●, which is useful for password or other sensitive information',
          }),
        /**
         * (optional) Whether or not to include an open in new tab icon to launch the value, which must be an http(s) URL
         */
        launchable: z
          .boolean()
          .nullable()
          .default(() => null)
          .meta({
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
        copyable: z
          .boolean()
          .nullable()
          .default(() => null)
          .meta({
            description:
              '(optional) Whether or not to include a copy to clipboard icon to copy the value',
          }),
        /**
         * (optional) Whether or not to also display the value as a QR code
         */
        qr: z
          .boolean()
          .nullable()
          .default(() => null)
          .meta({
            description:
              '(optional) Whether or not to also display the value as a QR code',
          }),
        /**
         * (optional) Whether or not to blur the value until the user reveals it, which is useful for a private key or other sensitive information
         */
        masked: z
          .boolean()
          .nullable()
          .default(() => null)
          .meta({
            description:
              '(optional) Whether or not to blur the value until the user reveals it, which is useful for a private key or other sensitive information',
          }),
        /**
         * (optional) Also offer the value as a download under this file name, such as "diagnostics.txt"
         */
        filename: z
          .string()
          .nullable()
          .default(() => null)
          .meta({
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
  signatures: z.ZodDefault<z.ZodRecord<z.ZodString, typeof AnySignature>>
  commitment: typeof Blake3Commitment
}> = /* @__PURE__ */ object({
  version: z.string(),
  platform: z.string(),
  url: z.string(),
  signature: AnySignature,
  signatures: z.record(z.string(), AnySignature).default(() => ({})),
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
export const AddKeyParams: z.ZodObject<{
  key: typeof AnyVerifyingKey
  name: z.ZodDefault<z.ZodNullable<z.ZodString>>
  userAgent: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  key: AnyVerifyingKey,
  name: z
    .string()
    .nullable()
    .default(() => null),
  userAgent: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'AddKeyParams',
})
export const AddMirrorParams: z.ZodObject<{
  url: z.ZodString
  commitment: typeof MerkleArchiveCommitment
  signature: typeof AnySignature
  signatures: z.ZodDefault<z.ZodRecord<z.ZodString, typeof AnySignature>>
}> = /* @__PURE__ */ object({
  url: z.string(),
  commitment: MerkleArchiveCommitment,
  signature: AnySignature,
  signatures: z.record(z.string(), AnySignature).default(() => ({})),
}).meta({
  id: 'AddMirrorParams',
})
export const AddPackageParams: z.ZodObject<{
  urls: z.ZodArray<z.ZodString>
  commitment: typeof MerkleArchiveCommitment
  signature: typeof AnySignature
  signatures: z.ZodDefault<z.ZodRecord<z.ZodString, typeof AnySignature>>
  /**
   * Keystream the hosted bytes are encrypted with. Omitted for a public s9pk.
   */
  private: z.ZodDefault<z.ZodNullable<typeof KeyStreamInfo>>
  /**
   * Keys in the registry's reader directory allowed to see this private s9pk.
   */
  readers: z.ZodDefault<z.ZodArray<typeof AnyVerifyingKey>>
}> = /* @__PURE__ */ object({
  urls: z.array(z.string()),
  commitment: MerkleArchiveCommitment,
  signature: AnySignature,
  signatures: z.record(z.string(), AnySignature).default(() => ({})),
  /**
   * Keystream the hosted bytes are encrypted with. Omitted for a public s9pk.
   */
  private: KeyStreamInfo.nullable()
    .default(() => null)
    .meta({
      description:
        'Keystream the hosted bytes are encrypted with. Omitted for a public s9pk.',
    }),
  /**
   * Keys in the registry's reader directory allowed to see this private s9pk.
   */
  readers: z
    .array(AnyVerifyingKey)
    .default(() => [])
    .meta({
      description:
        "Keys in the registry's reader directory allowed to see this private s9pk.",
    }),
}).meta({
  id: 'AddPackageParams',
})
export const AddPackageSignerParams: z.ZodObject<{
  id: typeof PackageId
  signer: typeof Guid
  versions: z.ZodDefault<z.ZodNullable<z.ZodString>>
  merge: z.ZodBoolean
}> = /* @__PURE__ */ object({
  id: PackageId,
  signer: Guid,
  versions: z
    .string()
    .nullable()
    .default(() => null),
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
export const AddPrivateDomainParams: z.ZodObject<{
  fqdn: z.ZodString
  gateway: typeof GatewayId
}> = /* @__PURE__ */ object({ fqdn: z.string(), gateway: GatewayId }).meta({
  id: 'AddPrivateDomainParams',
})
export const AddPublicDomainParams: z.ZodObject<{
  fqdn: z.ZodString
  acme: z.ZodDefault<z.ZodNullable<typeof AcmeProvider>>
  gateway: typeof GatewayId
  internalPort: z.ZodInt
}> = /* @__PURE__ */ object({
  fqdn: z.string(),
  acme: AcmeProvider.nullable().default(() => null),
  gateway: GatewayId,
  internalPort: z.int(),
}).meta({
  id: 'AddPublicDomainParams',
})
export const AddReaderParams: z.ZodObject<{
  label: z.ZodString
  key: typeof AnyVerifyingKey
}> = /* @__PURE__ */ object({ label: z.string(), key: AnyVerifyingKey }).meta({
  id: 'AddReaderParams',
})
export const AddTunnelParams: z.ZodObject<{
  name: z.ZodString
  config: z.ZodString
  type: z.ZodDefault<z.ZodNullable<typeof GatewayType>>
  setAsDefaultOutbound: z.ZodBoolean
}> = /* @__PURE__ */ object({
  name: z.string(),
  config: z.string(),
  type: GatewayType.nullable().default(() => null),
  setAsDefaultOutbound: z.boolean(),
}).meta({
  id: 'AddTunnelParams',
})
export const AddVersionParams: z.ZodObject<{
  version: z.ZodString
  releaseNotes: typeof LocaleString
  sourceVersion: z.ZodString
  gitHash: z.ZodDefault<z.ZodNullable<typeof GitHash>>
  expectedGitHash: z.ZodDefault<z.ZodNullable<typeof GitHash>>
  security: z.ZodDefault<z.ZodBoolean>
}> = /* @__PURE__ */ object({
  version: z.string(),
  releaseNotes: LocaleString,
  sourceVersion: z.string(),
  gitHash: GitHash.nullable().default(() => null),
  expectedGitHash: GitHash.nullable().default(() => null),
  security: z.boolean().default(() => false),
}).meta({
  id: 'AddVersionParams',
})
export const AddressInfo: z.ZodObject<{
  username: z.ZodDefault<z.ZodNullable<z.ZodString>>
  hostId: typeof HostId
  internalPort: z.ZodInt
  scheme: z.ZodDefault<z.ZodNullable<z.ZodString>>
  sslScheme: z.ZodDefault<z.ZodNullable<z.ZodString>>
  suffix: z.ZodString
}> = /* @__PURE__ */ object({
  username: z
    .string()
    .nullable()
    .default(() => null),
  hostId: HostId,
  internalPort: z.int(),
  scheme: z
    .string()
    .nullable()
    .default(() => null),
  sslScheme: z
    .string()
    .nullable()
    .default(() => null),
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

/**
 * Enrolled sign-ins indexed by canonical DER64 SPKI.
 */
export const AuthKeys: z.ZodRecord<z.ZodString, typeof Session> =
  /* @__PURE__ */ z.record(z.string(), Session).meta({
    id: 'AuthKeys',
    description: 'Enrolled sign-ins indexed by canonical DER64 SPKI.',
  })
export const BackupInitParams: z.ZodObject<{ id: typeof Guid }> =
  /* @__PURE__ */ object({ id: Guid }).meta({
    id: 'BackupInitParams',
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
export const BindId: z.ZodObject<{
  id: typeof HostId
  internalPort: z.ZodInt
}> = /* @__PURE__ */ object({ id: HostId, internalPort: z.int() }).meta({
  id: 'BindId',
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
  port: z.ZodDefault<z.ZodNullable<typeof CheckPortRes>>
  portV6: z.ZodDefault<z.ZodNullable<typeof CheckPortV6Res>>
}> = /* @__PURE__ */ object({
  port: CheckPortRes.nullable().default(() => null),
  portV6: CheckPortV6Res.nullable().default(() => null),
}).meta({
  id: 'CheckChallengeRes',
  description:
    'Reachability of the port a certificate authority validates on. A null leg\nwent unprobed.',
})
export const CheckDependenciesParam: z.ZodObject<{
  packageIds: z.ZodDefault<z.ZodNullable<z.ZodArray<typeof PackageId>>>
}> = /* @__PURE__ */ object({
  packageIds: z
    .array(PackageId)
    .nullable()
    .default(() => null),
}).meta({
  id: 'CheckDependenciesParam',
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
  password: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  id: BackupTargetId,
  hostname: z.string(),
  path: z.string(),
  username: z.string(),
  password: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'CifsUpdateParams',
})
export const ClearActionsParams: z.ZodObject<{
  except: z.ZodArray<typeof ActionId>
}> = /* @__PURE__ */ object({ except: z.array(ActionId) }).meta({
  id: 'ClearActionsParams',
})
export const ClearServiceInterfacesParams: z.ZodObject<{
  except: z.ZodArray<typeof ServiceInterfaceId>
}> = /* @__PURE__ */ object({ except: z.array(ServiceInterfaceId) }).meta({
  id: 'ClearServiceInterfacesParams',
})
export const ClearTaskParams: z.ZodObject<{
  packageId: typeof PackageId
  replayId: typeof ReplayId
  force: z.ZodDefault<z.ZodBoolean>
}> = /* @__PURE__ */ object({
  packageId: PackageId,
  replayId: ReplayId,
  force: z.boolean().default(() => false),
}).meta({
  id: 'ClearTaskParams',
})
export const CliSetDescriptionParams: z.ZodObject<{
  description: z.ZodDefault<z.ZodNullable<typeof LocaleString>>
  clear: z.ZodBoolean
}> = /* @__PURE__ */ object({
  description: LocaleString.nullable().default(() => null),
  clear: z.boolean(),
}).meta({
  id: 'CliSetDescriptionParams',
})
export const CompatibleHealthCheckResultKind: z.ZodUnion<
  readonly [
    typeof NamedHealthCheckResultKind,
    typeof LegacyHealthCheckResultKind,
  ]
> = /* @__PURE__ */ z
  .union([NamedHealthCheckResultKind, LegacyHealthCheckResultKind])
  .meta({
    id: 'CompatibleHealthCheckResultKind',
  })
export const ControlParams: z.ZodObject<{ id: typeof PackageId }> =
  /* @__PURE__ */ object({ id: PackageId }).meta({
    id: 'ControlParams',
  })
export const CreateNotificationParams: z.ZodObject<{
  level: typeof NotificationLevel
  title: z.ZodString
  message: z.ZodString
  /**
   * Optional long-form, markdown-formatted body that the UI renders in a
   * "View Details" modal when present (release notes, post-update
   * changelogs, structured error reports). When omitted, the notification
   * has no extra payload.
   */
  data: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  level: NotificationLevel,
  title: z.string(),
  message: z.string(),
  /**
   * Optional long-form, markdown-formatted body that the UI renders in a
   * "View Details" modal when present (release notes, post-update
   * changelogs, structured error reports). When omitted, the notification
   * has no extra payload.
   */
  data: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description:
        'Optional long-form, markdown-formatted body that the UI renders in a\n"View Details" modal when present (release notes, post-update\nchangelogs, structured error reports). When omitted, the notification\nhas no extra payload.',
    }),
}).meta({
  id: 'CreateNotificationParams',
})
export const CreateSubcontainerFsParams: z.ZodObject<{
  imageId: typeof ImageId
  name: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  imageId: ImageId,
  name: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'CreateSubcontainerFsParams',
})
export const CurrentDependencyKind: z.ZodUnion<
  readonly [
    z.ZodObject<{ kind: z.ZodLiteral<'exists'> }>,
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'running'> }>,
      z.ZodObject<{
        healthChecks: z.ZodDefault<z.ZodArray<typeof HealthCheckId>>
      }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    object({ kind: z.literal('exists') }),
    z.intersection(
      object({ kind: z.literal('running') }),
      object({ healthChecks: z.array(HealthCheckId).default(() => []) }),
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
export const DependencyRequirement: z.ZodUnion<
  readonly [
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'running'> }>,
      z.ZodObject<{
        id: typeof PackageId
        healthChecks: z.ZodArray<typeof HealthCheckId>
        versionRange: z.ZodString
      }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'exists'> }>,
      z.ZodObject<{ id: typeof PackageId; versionRange: z.ZodString }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(
      object({ kind: z.literal('running') }),
      object({
        id: PackageId,
        healthChecks: z.array(HealthCheckId),
        versionRange: z.string(),
      }),
    ),
    z.intersection(
      object({ kind: z.literal('exists') }),
      object({ id: PackageId, versionRange: z.string() }),
    ),
  ])
  .meta({
    id: 'DependencyRequirement',
  })
export const Description: z.ZodObject<{
  short: typeof LocaleString
  long: typeof LocaleString
}> = /* @__PURE__ */ object({ short: LocaleString, long: LocaleString }).meta({
  id: 'Description',
})
export const DesiredStatus: z.ZodUnion<
  readonly [
    z.ZodObject<{ main: z.ZodLiteral<'stopped'> }>,
    z.ZodIntersection<
      z.ZodObject<{ main: z.ZodLiteral<'restarting'> }>,
      z.ZodObject<{ restartAgain: z.ZodDefault<z.ZodBoolean> }>
    >,
    z.ZodObject<{ main: z.ZodLiteral<'running'> }>,
    z.ZodIntersection<
      z.ZodObject<{ main: z.ZodLiteral<'backing-up'> }>,
      z.ZodObject<{ onComplete: typeof StartStop }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ main: z.ZodLiteral<'updating'> }>,
      z.ZodObject<{ onComplete: typeof StartStop }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    object({ main: z.literal('stopped') }),
    z.intersection(
      object({ main: z.literal('restarting') }),
      object({ restartAgain: z.boolean().default(() => false) }),
    ),
    object({ main: z.literal('running') }),
    z.intersection(
      object({ main: z.literal('backing-up') }),
      object({ onComplete: StartStop }),
    ),
    z.intersection(
      object({ main: z.literal('updating') }),
      object({ onComplete: StartStop }),
    ),
  ])
  .meta({
    id: 'DesiredStatus',
  })
export const DestroySubcontainerFsParams: z.ZodObject<{ guid: typeof Guid }> =
  /* @__PURE__ */ object({ guid: Guid }).meta({
    id: 'DestroySubcontainerFsParams',
  })

/**
 * Per-gateway DNS intent and effective upstreams. `static_servers` is the persisted
 * override; `servers` and `source` are derived from it and the link configuration.
 * Without an override or gateway-provided resolvers, the public fallback is used.
 */
export const DnsSettings: z.ZodObject<{
  staticServers: z.ZodDefault<z.ZodNullable<z.ZodArray<z.ZodString>>>
  servers: z.ZodArray<z.ZodString>
  source: typeof DnsSource
}> = /* @__PURE__ */ object({
  staticServers: z
    .array(z.string())
    .nullable()
    .default(() => null),
  servers: z.array(z.string()),
  source: DnsSource,
}).meta({
  id: 'DnsSettings',
  description:
    'Per-gateway DNS intent and effective upstreams. `static_servers` is the persisted\noverride; `servers` and `source` are derived from it and the link configuration.\nWithout an override or gateway-provided resolvers, the public fallback is used.',
})
export const DomainSettings: z.ZodObject<{ gateway: typeof GatewayId }> =
  /* @__PURE__ */ object({ gateway: GatewayId }).meta({
    id: 'DomainSettings',
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
  name: z.ZodDefault<z.ZodNullable<z.ZodString>>
  dns: z.ZodDefault<z.ZodNullable<typeof OutboundDnsUpdate>>
}> = /* @__PURE__ */ object({
  id: GatewayId,
  name: z
    .string()
    .nullable()
    .default(() => null),
  dns: OutboundDnsUpdate.nullable().default(() => null),
}).meta({
  id: 'EditGatewayParams',
})
export const EditReaderParams: z.ZodObject<{
  id: typeof Guid
  label: z.ZodString
}> = /* @__PURE__ */ object({ id: Guid, label: z.string() }).meta({
  id: 'EditReaderParams',
})
export const EditSignerParams: z.ZodObject<{
  id: typeof Guid
  setName: z.ZodDefault<z.ZodNullable<z.ZodString>>
  addContact: z.ZodArray<typeof ContactInfo>
  addKey: z.ZodArray<z.ZodString>
  removeContact: z.ZodArray<typeof ContactInfo>
  removeKey: z.ZodArray<typeof AnyVerifyingKey>
  /**
   * Removes whole exports containing a matching primary-key fingerprint, preserving signing keys.
   */
  removeGpgExport: z.ZodDefault<z.ZodArray<z.ZodString>>
}> = /* @__PURE__ */ object({
  id: Guid,
  setName: z
    .string()
    .nullable()
    .default(() => null),
  addContact: z.array(ContactInfo),
  addKey: z.array(z.string()),
  removeContact: z.array(ContactInfo),
  removeKey: z.array(AnyVerifyingKey),
  /**
   * Removes whole exports containing a matching primary-key fingerprint, preserving signing keys.
   */
  removeGpgExport: z
    .array(z.string())
    .default(() => [])
    .meta({
      description:
        'Removes whole exports containing a matching primary-key fingerprint, preserving signing keys.',
    }),
}).meta({
  id: 'EditSignerParams',
})

/**
 * Edits an existing release; omitted fields, assets and authorized signers remain unchanged.
 */
export const EditVersionParams: z.ZodObject<{
  version: z.ZodString
  releaseNotes: z.ZodDefault<z.ZodNullable<typeof LocaleString>>
  sourceVersion: z.ZodDefault<z.ZodNullable<z.ZodString>>
  gitHash: z.ZodDefault<z.ZodNullable<typeof GitHash>>
  expectedGitHash: z.ZodDefault<z.ZodNullable<typeof GitHash>>
  security: z.ZodDefault<z.ZodNullable<z.ZodBoolean>>
}> = /* @__PURE__ */ object({
  version: z.string(),
  releaseNotes: LocaleString.nullable().default(() => null),
  sourceVersion: z
    .string()
    .nullable()
    .default(() => null),
  gitHash: GitHash.nullable().default(() => null),
  expectedGitHash: GitHash.nullable().default(() => null),
  security: z
    .boolean()
    .nullable()
    .default(() => null),
}).meta({
  id: 'EditVersionParams',
  description:
    'Edits an existing release; omitted fields, assets and authorized signers remain unchanged.',
})
export const EffectsRunActionParams: z.ZodObject<{
  packageId: z.ZodDefault<z.ZodNullable<typeof PackageId>>
  actionId: typeof ActionId
  input: z.ZodUnknown
}> = /* @__PURE__ */ object({
  packageId: PackageId.nullable().default(() => null),
  actionId: ActionId,
  input: z.unknown(),
}).meta({
  id: 'EffectsRunActionParams',
})
export const ExportPackageVersionParams: z.ZodIntersection<
  z.ZodObject<{ id: typeof PackageId; version: typeof Version }>,
  typeof ReaderParams
> = /* @__PURE__ */ z
  .intersection(object({ id: PackageId, version: Version }), ReaderParams)
  .meta({
    id: 'ExportPackageVersionParams',
  })
export const ExportRangeServiceInterfaceParams: z.ZodObject<{
  hostId: typeof HostId
  internalStartPort: z.ZodInt
  id: typeof ServiceInterfaceId
  name: z.ZodString
  description: z.ZodString
  scheme: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  hostId: HostId,
  internalStartPort: z.int(),
  id: ServiceInterfaceId,
  name: z.string(),
  description: z.string(),
  scheme: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'ExportRangeServiceInterfaceParams',
})
export const ForgetGatewayParams: z.ZodObject<{ gateway: typeof GatewayId }> =
  /* @__PURE__ */ object({ gateway: GatewayId }).meta({
    id: 'ForgetGatewayParams',
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
export const GatewayInfo: z.ZodObject<{
  id: typeof GatewayId
  name: z.ZodString
  public: z.ZodBoolean
}> = /* @__PURE__ */ object({
  id: GatewayId,
  name: z.string(),
  public: z.boolean(),
}).meta({
  id: 'GatewayInfo',
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
  packageId: z.ZodDefault<z.ZodNullable<typeof PackageId>>
  actionId: typeof ActionId
  prefill: z.ZodCodec<z.ZodOptional<typeof literal_2>, typeof literal_2>
}> = /* @__PURE__ */ object({
  packageId: PackageId.nullable().default(() => null),
  actionId: ActionId,
  prefill: wire(literal_2.optional(), literal_2),
}).meta({
  id: 'GetActionInputParams',
})
export const GetContainerIpParams: z.ZodObject<{
  packageId: z.ZodDefault<z.ZodNullable<typeof PackageId>>
  callback: z.ZodDefault<z.ZodNullable<typeof CallbackId>>
}> = /* @__PURE__ */ object({
  packageId: PackageId.nullable().default(() => null),
  callback: CallbackId.nullable().default(() => null),
}).meta({
  id: 'GetContainerIpParams',
})
export const GetHostInfoParams: z.ZodObject<{
  hostId: typeof HostId
  packageId: z.ZodDefault<z.ZodNullable<typeof PackageId>>
  callback: z.ZodDefault<z.ZodNullable<typeof CallbackId>>
}> = /* @__PURE__ */ object({
  hostId: HostId,
  packageId: PackageId.nullable().default(() => null),
  callback: CallbackId.nullable().default(() => null),
}).meta({
  id: 'GetHostInfoParams',
})
export const GetOutboundGatewayParams: z.ZodObject<{
  callback: z.ZodDefault<z.ZodNullable<typeof CallbackId>>
}> = /* @__PURE__ */ object({
  callback: CallbackId.nullable().default(() => null),
}).meta({
  id: 'GetOutboundGatewayParams',
})
export const GetPackageParams: z.ZodIntersection<
  z.ZodObject<{
    id: z.ZodDefault<z.ZodNullable<typeof PackageId>>
    targetVersion: z.ZodDefault<z.ZodNullable<z.ZodString>>
    sourceVersion: z.ZodDefault<z.ZodNullable<typeof Version>>
    otherVersions: z.ZodDefault<z.ZodNullable<typeof PackageDetailLevel>>
    allRevisions: z.ZodDefault<z.ZodBoolean>
  }>,
  typeof ReaderParams
> = /* @__PURE__ */ z
  .intersection(
    object({
      id: PackageId.nullable().default(() => null),
      targetVersion: z
        .string()
        .nullable()
        .default(() => null),
      sourceVersion: Version.nullable().default(() => null),
      otherVersions: PackageDetailLevel.nullable().default(() => null),
      allRevisions: z.boolean().default(() => false),
    }),
    ReaderParams,
  )
  .meta({
    id: 'GetPackageParams',
  })
export const GetServiceInterfaceParams: z.ZodObject<{
  packageId: z.ZodDefault<z.ZodNullable<typeof PackageId>>
  serviceInterfaceId: typeof ServiceInterfaceId
  callback: z.ZodDefault<z.ZodNullable<typeof CallbackId>>
}> = /* @__PURE__ */ object({
  packageId: PackageId.nullable().default(() => null),
  serviceInterfaceId: ServiceInterfaceId,
  callback: CallbackId.nullable().default(() => null),
}).meta({
  id: 'GetServiceInterfaceParams',
})
export const GetServiceManifestParams: z.ZodObject<{
  packageId: typeof PackageId
  callback: z.ZodDefault<z.ZodNullable<typeof CallbackId>>
}> = /* @__PURE__ */ object({
  packageId: PackageId,
  callback: CallbackId.nullable().default(() => null),
}).meta({
  id: 'GetServiceManifestParams',
})
export const GetServicePortForwardParams: z.ZodObject<{
  packageId: z.ZodDefault<z.ZodNullable<typeof PackageId>>
  hostId: typeof HostId
  internalPort: z.ZodInt
}> = /* @__PURE__ */ object({
  packageId: PackageId.nullable().default(() => null),
  hostId: HostId,
  internalPort: z.int(),
}).meta({
  id: 'GetServicePortForwardParams',
})
export const GetSslCertificateParams: z.ZodObject<{
  hostnames: z.ZodArray<z.ZodString>
  algorithm: z.ZodDefault<z.ZodNullable<typeof Algorithm>>
  callback: z.ZodDefault<z.ZodNullable<typeof CallbackId>>
}> = /* @__PURE__ */ object({
  hostnames: z.array(z.string()),
  algorithm: Algorithm.nullable().default(() => null),
  callback: CallbackId.nullable().default(() => null),
}).meta({
  id: 'GetSslCertificateParams',
})
export const GetSslKeyParams: z.ZodObject<{
  hostnames: z.ZodArray<z.ZodString>
  algorithm: z.ZodDefault<z.ZodNullable<typeof Algorithm>>
}> = /* @__PURE__ */ object({
  hostnames: z.array(z.string()),
  algorithm: Algorithm.nullable().default(() => null),
}).meta({
  id: 'GetSslKeyParams',
})
export const GetStatusParams: z.ZodObject<{
  packageId: z.ZodDefault<z.ZodNullable<typeof PackageId>>
  callback: z.ZodDefault<z.ZodNullable<typeof CallbackId>>
}> = /* @__PURE__ */ object({
  packageId: PackageId.nullable().default(() => null),
  callback: CallbackId.nullable().default(() => null),
}).meta({
  id: 'GetStatusParams',
})
export const GetSystemSmtpParams: z.ZodObject<{
  callback: z.ZodDefault<z.ZodNullable<typeof CallbackId>>
}> = /* @__PURE__ */ object({
  callback: CallbackId.nullable().default(() => null),
}).meta({
  id: 'GetSystemSmtpParams',
})
export const GrantedSharedVolume: z.ZodObject<{
  id: typeof SharedVolumeId
  name: z.ZodString
  readonly: z.ZodBoolean
  availability: typeof SharedVolumeAvailability
}> = /* @__PURE__ */ object({
  id: SharedVolumeId,
  name: z.string(),
  readonly: z.boolean(),
  availability: SharedVolumeAvailability,
}).meta({
  id: 'GrantedSharedVolume',
})
export const HardwareRequirements: z.ZodObject<{
  device: z.ZodDefault<z.ZodArray<typeof DeviceFilter>>
  ram: z.ZodDefault<z.ZodNullable<z.ZodInt>>
  arch: z.ZodDefault<z.ZodNullable<z.ZodArray<z.ZodString>>>
}> = /* @__PURE__ */ object({
  device: z.array(DeviceFilter).default(() => []),
  ram: z
    .int()
    .nullable()
    .default(() => null),
  arch: z
    .array(z.string())
    .nullable()
    .default(() => null),
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
        removeAction: z.ZodDefault<z.ZodNullable<typeof ActionId>>
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
        removeAction: ActionId.nullable().default(() => null),
        overflowActions: z.array(ActionId),
        info: wire(z.unknown().optional(), z.unknown()),
      }),
    ),
  ])
  .meta({
    id: 'HostnameMetadata',
  })
export const ImageSource: z.ZodUnion<
  readonly [
    z.ZodLiteral<'packed'>,
    z.ZodObject<{
      dockerBuild: z.ZodObject<{
        workdir: z.ZodDefault<z.ZodNullable<z.ZodString>>
        dockerfile: z.ZodDefault<z.ZodNullable<z.ZodString>>
        buildArgs: z.ZodOptional<
          z.ZodNullable<z.ZodRecord<z.ZodString, typeof BuildArg>>
        >
      }>
    }>,
    z.ZodObject<{ dockerTag: z.ZodString }>,
    z.ZodObject<{
      /**
       * A self-contained local OCI image-layout directory or tar archive.
       */
      ociLayout: z.ZodObject<{
        path: z.ZodString
        /**
         * Top-level reference annotation, Docker image name, or descriptor digest. Unset selects by platform.
         */
        reference: z.ZodOptional<z.ZodNullable<z.ZodString>>
      }>
    }>,
  ]
> = /* @__PURE__ */ z
  .union([
    z.literal('packed'),
    object({
      dockerBuild: object({
        workdir: z
          .string()
          .nullable()
          .default(() => null),
        dockerfile: z
          .string()
          .nullable()
          .default(() => null),
        buildArgs: z.record(z.string(), BuildArg).nullable().optional(),
      }),
    }),
    object({ dockerTag: z.string() }),
    object({
      /**
       * A self-contained local OCI image-layout directory or tar archive.
       */
      ociLayout: object({
        path: z.string(),
        /**
         * Top-level reference annotation, Docker image name, or descriptor digest. Unset selects by platform.
         */
        reference: z.string().nullable().optional().meta({
          description:
            'Top-level reference annotation, Docker image name, or descriptor digest. Unset selects by platform.',
        }),
      }).meta({
        description:
          'A self-contained local OCI image-layout directory or tar archive.',
      }),
    }).meta({
      description:
        'A self-contained local OCI image-layout directory or tar archive.',
    }),
  ])
  .meta({
    id: 'ImageSource',
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
  deviceType: z.ZodDefault<z.ZodNullable<typeof NetworkInterfaceType>>
  subnets: z.ZodArray<z.ZodString>
  lanIp: z.ZodArray<z.ZodString>
  wanIp: z.ZodDefault<z.ZodNullable<z.ZodString>>
  ntpServers: z.ZodArray<z.ZodString>
  dnsServers: z.ZodArray<z.ZodString>
  /**
   * Lowest IPv4 default-route metric in `main`, or `None` when absent.
   */
  defaultRouteV4: z.ZodDefault<z.ZodNullable<z.ZodInt>>
  /**
   * Lowest IPv6 default-route metric in `main`, or `None` when absent.
   */
  defaultRouteV6: z.ZodDefault<z.ZodNullable<z.ZodInt>>
}> = /* @__PURE__ */ object({
  name: z.string(),
  scopeId: z.int(),
  deviceType: NetworkInterfaceType.nullable().default(() => null),
  subnets: z.array(z.string()),
  lanIp: z.array(z.string()),
  wanIp: z
    .string()
    .nullable()
    .default(() => null),
  ntpServers: z.array(z.string()),
  dnsServers: z.array(z.string()),
  /**
   * Lowest IPv4 default-route metric in `main`, or `None` when absent.
   */
  defaultRouteV4: z
    .int()
    .nullable()
    .default(() => null)
    .meta({
      description:
        'Lowest IPv4 default-route metric in `main`, or `None` when absent.',
    }),
  /**
   * Lowest IPv6 default-route metric in `main`, or `None` when absent.
   */
  defaultRouteV6: z
    .int()
    .nullable()
    .default(() => null)
    .meta({
      description:
        'Lowest IPv6 default-route metric in `main`, or `None` when absent.',
    }),
}).meta({
  id: 'IpInfo',
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
export const ListServiceInterfacesParams: z.ZodObject<{
  packageId: z.ZodDefault<z.ZodNullable<typeof PackageId>>
  callback: z.ZodDefault<z.ZodNullable<typeof CallbackId>>
}> = /* @__PURE__ */ object({
  packageId: PackageId.nullable().default(() => null),
  callback: CallbackId.nullable().default(() => null),
}).meta({
  id: 'ListServiceInterfacesParams',
})
export const LogFollowResponse: z.ZodObject<{
  startCursor: z.ZodDefault<z.ZodNullable<z.ZodString>>
  /**
   * Present for ordinary replies; typed observations use same-ID events.
   */
  guid: z.ZodDefault<z.ZodNullable<typeof Guid>>
}> = /* @__PURE__ */ object({
  startCursor: z
    .string()
    .nullable()
    .default(() => null),
  /**
   * Present for ordinary replies; typed observations use same-ID events.
   */
  guid: Guid.nullable()
    .default(() => null)
    .meta({
      description:
        'Present for ordinary replies; typed observations use same-ID events.',
    }),
}).meta({
  id: 'LogFollowResponse',
})
export const LogResponse: z.ZodObject<{
  entries: z.ZodArray<typeof LogEntry>
  startCursor: z.ZodDefault<z.ZodNullable<z.ZodString>>
  endCursor: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  entries: z.array(LogEntry),
  startCursor: z
    .string()
    .nullable()
    .default(() => null),
  endCursor: z
    .string()
    .nullable()
    .default(() => null),
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
  ephemeral: z.ZodDefault<z.ZodBoolean>
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
  ephemeral: z
    .boolean()
    .default(() => false)
    .meta({
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
export const Metadata: z.ZodObject<{
  title: typeof LocaleString
  icon: typeof PathOrUrl
}> = /* @__PURE__ */ object({ title: LocaleString, icon: PathOrUrl }).meta({
  id: 'Metadata',
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
export const MountSharedVolumeParams: z.ZodObject<{
  id: typeof SharedVolumeId
  subcontainer: typeof Guid
  mountpoint: z.ZodString
  subpath: z.ZodDefault<z.ZodNullable<z.ZodString>>
  readonly: z.ZodBoolean
}> = /* @__PURE__ */ object({
  id: SharedVolumeId,
  subcontainer: Guid,
  mountpoint: z.string(),
  subpath: z
    .string()
    .nullable()
    .default(() => null),
  readonly: z.boolean(),
}).meta({
  id: 'MountSharedVolumeParams',
})
export const MountTarget: z.ZodCodec<
  z.ZodObject<{
    packageId: typeof PackageId
    volumeId: typeof VolumeId
    subpath: z.ZodOptional<z.ZodNullable<z.ZodString>>
    readonly: z.ZodBoolean
    idmap: z.ZodOptional<z.ZodArray<typeof IdMap>>
  }>,
  z.ZodObject<{
    packageId: typeof PackageId
    volumeId: typeof VolumeId
    subpath: z.ZodNullable<z.ZodString>
    readonly: z.ZodBoolean
    idmap: z.ZodArray<typeof IdMap>
  }>
> = /* @__PURE__ */ wire(
  object({
    packageId: PackageId,
    volumeId: VolumeId,
    subpath: z.string().nullable().optional(),
    readonly: z.boolean(),
    idmap: z.array(IdMap).optional(),
  }),
  object({
    packageId: PackageId,
    volumeId: VolumeId,
    subpath: z.string().nullable(),
    readonly: z.boolean(),
    idmap: z.array(IdMap),
  }),
).meta({
  id: 'MountTarget',
})
export const Notification: z.ZodObject<{
  packageId: z.ZodDefault<z.ZodNullable<typeof PackageId>>
  createdAt: z.ZodString
  code: z.ZodInt
  level: typeof NotificationLevel
  title: z.ZodString
  message: z.ZodString
  data: z.ZodUnknown
  seen: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  packageId: PackageId.nullable().default(() => null),
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
        listenAll: z.ZodDefault<z.ZodBoolean>
        remoteUsername: z.ZodDefault<z.ZodNullable<z.ZodString>>
        remotePassword: z.ZodDefault<z.ZodNullable<z.ZodString>>
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
        listenAll: z.boolean().default(() => false),
        remoteUsername: z
          .string()
          .nullable()
          .default(() => null),
        remotePassword: z
          .string()
          .nullable()
          .default(() => null),
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
export const OsVersionInfo: z.ZodObject<{
  releaseNotes: typeof LocaleString
  sourceVersion: z.ZodString
  /**
   * Commit that produced the OS image, if recorded at publication.
   */
  gitHash: z.ZodDefault<z.ZodNullable<typeof GitHash>>
  /**
   * Whether the release includes security fixes.
   */
  security: z.ZodDefault<z.ZodBoolean>
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
  releaseNotes: LocaleString,
  sourceVersion: z.string(),
  /**
   * Commit that produced the OS image, if recorded at publication.
   */
  gitHash: GitHash.nullable()
    .default(() => null)
    .meta({
      description:
        'Commit that produced the OS image, if recorded at publication.',
    }),
  /**
   * Whether the release includes security fixes.
   */
  security: z
    .boolean()
    .default(() => false)
    .meta({
      description: 'Whether the release includes security fixes.',
    }),
  authorized: z.array(Guid),
  iso: z.record(z.string(), RegistryAsset(Blake3Commitment)),
  squashfs: z.record(z.string(), RegistryAsset(Blake3Commitment)),
  img: z.record(z.string(), RegistryAsset(Blake3Commitment)),
}).meta({
  id: 'OsVersionInfo',
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
export const PackageInfoShort: z.ZodObject<{
  releaseNotes: typeof LocaleString
  /**
   * `PackageMetadata::default_locale`.
   */
  defaultLocale: z.ZodOptional<z.ZodNullable<z.ZodString>>
  /**
   * A newer same-flavor revision covers this release's hardware.
   */
  superseded: z.ZodOptional<z.ZodBoolean>
}> = /* @__PURE__ */ object({
  releaseNotes: LocaleString,
  /**
   * `PackageMetadata::default_locale`.
   */
  defaultLocale: z.string().nullable().optional().meta({
    description: '`PackageMetadata::default_locale`.',
  }),
  /**
   * A newer same-flavor revision covers this release's hardware.
   */
  superseded: z.boolean().optional().meta({
    description: "A newer same-flavor revision covers this release's hardware.",
  }),
}).meta({
  id: 'PackageInfoShort',
})

/**
 * A package's releases this server can run, by version; release data stays in the saved index.
 */
export const PackageSelection: z.ZodObject<{
  /**
   * The newest release of each flavor, preferring releases this StartOS can install.
   */
  best: z.ZodArray<typeof Version>
  /**
   * Every release this server can run, keyed exactly as in the saved index.
   */
  versions: z.ZodRecord<z.ZodString, typeof ListedVersion>
  /**
   * When this registry first published any version it still carries.
   */
  firstPublishedAt: z.ZodOptional<z.ZodNullable<z.ZodString>>
  /**
   * Newest release of the installed flavor that the installed version can migrate to and this
   * server can install.
   */
  updateTarget: z.ZodOptional<z.ZodNullable<typeof Version>>
  /**
   * Published vulnerabilities, in the saved index, that a key trusted for packages signed.
   */
  vulnerabilities: z.ZodOptional<z.ZodArray<z.ZodString>>
}> = /* @__PURE__ */ object({
  /**
   * The newest release of each flavor, preferring releases this StartOS can install.
   */
  best: z.array(Version).meta({
    description:
      'The newest release of each flavor, preferring releases this StartOS can install.',
  }),
  /**
   * Every release this server can run, keyed exactly as in the saved index.
   */
  versions: z.record(z.string(), ListedVersion).meta({
    description:
      'Every release this server can run, keyed exactly as in the saved index.',
  }),
  /**
   * When this registry first published any version it still carries.
   */
  firstPublishedAt: z.string().nullable().optional().meta({
    description:
      'When this registry first published any version it still carries.',
  }),
  /**
   * Newest release of the installed flavor that the installed version can migrate to and this
   * server can install.
   */
  updateTarget: Version.nullable().optional().meta({
    description:
      'Newest release of the installed flavor that the installed version can migrate to and this\nserver can install.',
  }),
  /**
   * Published vulnerabilities, in the saved index, that a key trusted for packages signed.
   */
  vulnerabilities: z.array(z.string()).optional().meta({
    description:
      'Published vulnerabilities, in the saved index, that a key trusted for packages signed.',
  }),
}).meta({
  id: 'PackageSelection',
  description:
    "A package's releases this server can run, by version; release data stays in the saved index.",
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
export const PluginHostnameInfo: z.ZodCodec<
  z.ZodObject<{
    /**
     * [`PackageId::start_os`] identifies the server's own host (the StartOS UI).
     */
    packageId: z.ZodOptional<z.ZodNullable<typeof PackageId>>
    hostId: typeof HostId
    internalPort: z.ZodInt
    ssl: z.ZodBoolean
    public: z.ZodBoolean
    hostname: z.ZodString
    port: z.ZodOptional<z.ZodNullable<z.ZodInt>>
    info: z.ZodOptional<z.ZodUnknown>
  }>,
  z.ZodObject<{
    /**
     * [`PackageId::start_os`] identifies the server's own host (the StartOS UI).
     */
    packageId: typeof PackageId
    hostId: typeof HostId
    internalPort: z.ZodInt
    ssl: z.ZodBoolean
    public: z.ZodBoolean
    hostname: z.ZodString
    port: z.ZodNullable<z.ZodInt>
    info: z.ZodUnknown
  }>
> = /* @__PURE__ */ wire(
  object({
    /**
     * [`PackageId::start_os`] identifies the server's own host (the StartOS UI).
     */
    packageId: PackageId.nullable().optional().meta({
      description:
        "[`PackageId::start_os`] identifies the server's own host (the StartOS UI).",
    }),
    hostId: HostId,
    internalPort: z.int(),
    ssl: z.boolean(),
    public: z.boolean(),
    hostname: z.string(),
    port: z.int().nullable().optional(),
    info: z.unknown().optional(),
  }),
  object({
    /**
     * [`PackageId::start_os`] identifies the server's own host (the StartOS UI).
     */
    packageId: PackageId.meta({
      description:
        "[`PackageId::start_os`] identifies the server's own host (the StartOS UI).",
    }),
    hostId: HostId,
    internalPort: z.int(),
    ssl: z.boolean(),
    public: z.boolean(),
    hostname: z.string(),
    port: z.int().nullable(),
    info: z.unknown(),
  }),
).meta({
  id: 'PluginHostnameInfo',
})
export const PortForward: z.ZodObject<{
  src: z.ZodString
  dst: z.ZodString
  gateway: typeof GatewayId
  /**
   * Number of contiguous ports covered by this forward (always >= 1).
   * Set to >1 by [`crate::net::host::binding::RangeBindInfo`] entries to
   * represent a single iptables port-range rule rather than N per-port
   * rules. Defaults to 1 to keep the on-disk shape compatible with
   * existing single-port `PortForward`s.
   */
  count: z.ZodCodec<z.ZodOptional<z.ZodInt>, z.ZodInt>
  /**
   * Whether this forward receives local traffic. Used to gate hairpinning check.
   */
  local: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  src: z.string(),
  dst: z.string(),
  gateway: GatewayId,
  /**
   * Number of contiguous ports covered by this forward (always >= 1).
   * Set to >1 by [`crate::net::host::binding::RangeBindInfo`] entries to
   * represent a single iptables port-range rule rather than N per-port
   * rules. Defaults to 1 to keep the on-disk shape compatible with
   * existing single-port `PortForward`s.
   */
  count: wire(z.int().optional(), z.int()).meta({
    description:
      'Number of contiguous ports covered by this forward (always >= 1).\nSet to >1 by [`crate::net::host::binding::RangeBindInfo`] entries to\nrepresent a single iptables port-range rule rather than N per-port\nrules. Defaults to 1 to keep the on-disk shape compatible with\nexisting single-port `PortForward`s.',
  }),
  /**
   * Whether this forward receives local traffic. Used to gate hairpinning check.
   */
  local: wire(z.boolean().optional(), z.boolean()).meta({
    description:
      'Whether this forward receives local traffic. Used to gate hairpinning check.',
  }),
}).meta({
  id: 'PortForward',
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
        realm: z.ZodDefault<z.ZodNullable<z.ZodString>>
      }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ type: z.ZodLiteral<'basic'> }>,
      z.ZodObject<{
        credentials: z.ZodArray<typeof BasicCredential>
        realm: z.ZodDefault<z.ZodNullable<z.ZodString>>
      }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(
      object({ type: z.literal('bearer') }),
      object({
        tokens: z.array(z.string()),
        realm: z
          .string()
          .nullable()
          .default(() => null),
      }),
    ),
    z.intersection(
      object({ type: z.literal('basic') }),
      object({
        credentials: z.array(BasicCredential),
        realm: z
          .string()
          .nullable()
          .default(() => null),
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
  acme: z.ZodDefault<z.ZodNullable<typeof AcmeProvider>>
}> = /* @__PURE__ */ object({
  gateway: GatewayId,
  acme: AcmeProvider.nullable().default(() => null),
}).meta({
  id: 'PublicDomainConfig',
})

/**
 * The single restricted service interface a port-range binding may export.
 *
 * Unlike [`ServiceInterface`], a range interface is always `api`-typed and
 * carries no `masked` / `username` / `path` / `query` and no per-address
 * [`AddressInfo`] — its address is the host plus the range's external port
 * span, taken from the [`RangeBindInfo`](crate::net::host::binding::RangeBindInfo)
 * it lives under. `scheme` is an optional transport prefix (e.g. `tcp` for
 * bitcoin ZMQ endpoints); most ranges (coturn RTP, FTP data) omit it.
 */
export const RangeServiceInterface: z.ZodObject<{
  id: typeof ServiceInterfaceId
  name: z.ZodString
  description: z.ZodString
  scheme: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  id: ServiceInterfaceId,
  name: z.string(),
  description: z.string(),
  scheme: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'RangeServiceInterface',
  description:
    "The single restricted service interface a port-range binding may export.\n\nUnlike [`ServiceInterface`], a range interface is always `api`-typed and\ncarries no `masked` / `username` / `path` / `query` and no per-address\n[`AddressInfo`] — its address is the host plus the range's external port\nspan, taken from the [`RangeBindInfo`](crate::net::host::binding::RangeBindInfo)\nit lives under. `scheme` is an optional transport prefix (e.g. `tcp` for\nbitcoin ZMQ endpoints); most ranges (coturn RTP, FTP data) omit it.",
})

/**
 * A key allowed to read the private s9pks that list it.
 */
export const ReaderKey: z.ZodObject<{
  label: z.ZodString
  key: typeof AnyVerifyingKey
}> = /* @__PURE__ */ object({ label: z.string(), key: AnyVerifyingKey }).meta({
  id: 'ReaderKey',
  description: 'A key allowed to read the private s9pks that list it.',
})
export const RebuildParams: z.ZodObject<{ id: typeof PackageId }> =
  /* @__PURE__ */ object({ id: PackageId }).meta({
    id: 'RebuildParams',
  })
export const RegistryIdentity: z.ZodObject<{
  name: z.ZodDefault<z.ZodNullable<z.ZodString>>
  icon: z.ZodDefault<z.ZodNullable<typeof DataUrl>>
}> = /* @__PURE__ */ object({
  name: z
    .string()
    .nullable()
    .default(() => null),
  icon: DataUrl.nullable().default(() => null),
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
export const RemoveKeyParams: z.ZodObject<{
  keys: z.ZodArray<typeof AnyVerifyingKey>
}> = /* @__PURE__ */ object({ keys: z.array(AnyVerifyingKey) }).meta({
  id: 'RemoveKeyParams',
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
  version: z.ZodDefault<z.ZodNullable<typeof Version>>
  sighash: z.ZodDefault<z.ZodNullable<typeof literal_0>>
  force: z.ZodDefault<z.ZodBoolean>
}> = /* @__PURE__ */ object({
  id: PackageId,
  version: Version.nullable().default(() => null),
  sighash: literal_0.nullable().default(() => null),
  force: z.boolean().default(() => false),
}).meta({
  id: 'RemovePackageParams',
})
export const RemovePackageSignerParams: z.ZodObject<{
  id: typeof PackageId
  signer: typeof Guid
}> = /* @__PURE__ */ object({ id: PackageId, signer: Guid }).meta({
  id: 'RemovePackageSignerParams',
})
export const RemovePackageVulnerabilityParams: z.ZodObject<{
  id: typeof PackageId
  vulnerabilityId: z.ZodString
}> = /* @__PURE__ */ object({
  id: PackageId,
  vulnerabilityId: z.string(),
}).meta({
  id: 'RemovePackageVulnerabilityParams',
})
export const RemoveReaderParams: z.ZodObject<{ id: typeof Guid }> =
  /* @__PURE__ */ object({ id: Guid }).meta({
    id: 'RemoveReaderParams',
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
export const RestorePackageParams: z.ZodObject<{
  targetId: typeof BackupTargetId
  password: z.ZodString
  ids: z.ZodArray<typeof PackageId>
  serverId: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  targetId: BackupTargetId,
  password: z.string(),
  ids: z.array(PackageId),
  serverId: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'RestorePackageParams',
})
export const RestoreVolumeParams: z.ZodObject<{
  targetId: typeof BackupTargetId
  password: z.ZodString
  ids: z.ZodArray<typeof SharedVolumeId>
  serverId: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  targetId: BackupTargetId,
  password: z.string(),
  ids: z.array(SharedVolumeId),
  serverId: z
    .string()
    .nullable()
    .default(() => null),
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
export const RetireBindingParams: z.ZodObject<{
  id: typeof HostId
  internalPort: z.ZodInt
}> = /* @__PURE__ */ object({ id: HostId, internalPort: z.int() }).meta({
  id: 'RetireBindingParams',
})
export const RetireHostParams: z.ZodObject<{ id: typeof HostId }> =
  /* @__PURE__ */ object({ id: HostId }).meta({
    id: 'RetireHostParams',
  })
export const RunActionParams: z.ZodObject<{
  packageId: typeof PackageId
  eventId: z.ZodDefault<z.ZodNullable<typeof Guid>>
  actionId: typeof ActionId
  input: z.ZodDefault<z.ZodNullable<z.ZodUnknown>>
}> = /* @__PURE__ */ object({
  packageId: PackageId,
  eventId: Guid.nullable().default(() => null),
  actionId: ActionId,
  input: z
    .unknown()
    .nullable()
    .default(() => null),
}).meta({
  id: 'RunActionParams',
})
export const ServiceDependencyMetadata: z.ZodObject<{
  title: typeof LocaleString
}> = /* @__PURE__ */ object({ title: LocaleString }).meta({
  id: 'ServiceDependencyMetadata',
})
export const SetDefaultOutboundParams: z.ZodObject<{
  gateway: z.ZodDefault<z.ZodNullable<typeof GatewayId>>
  dns: z.ZodDefault<z.ZodNullable<typeof OutboundDnsUpdate>>
}> = /* @__PURE__ */ object({
  gateway: GatewayId.nullable().default(() => null),
  dns: OutboundDnsUpdate.nullable().default(() => null),
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
  secure: z.ZodDefault<z.ZodNullable<z.ZodBoolean>>
}> = /* @__PURE__ */ object({
  gateway: GatewayId,
  secure: z
    .boolean()
    .nullable()
    .default(() => null),
}).meta({
  id: 'SetGatewaySecureParams',
})
export const SetIconParams: z.ZodObject<{
  icon: z.ZodNullable<typeof DataUrl>
}> = /* @__PURE__ */ object({ icon: DataUrl.nullable() }).meta({
  id: 'SetIconParams',
})
export const SetMainStatus: z.ZodObject<{
  status: typeof SetMainStatusStatus
}> = /* @__PURE__ */ object({ status: SetMainStatusStatus }).meta({
  id: 'SetMainStatus',
})
export const SetOutboundGatewayParams: z.ZodObject<{
  package: typeof PackageId
  gateway: z.ZodDefault<z.ZodNullable<typeof GatewayId>>
  dns: z.ZodDefault<z.ZodNullable<typeof OutboundDnsUpdate>>
}> = /* @__PURE__ */ object({
  package: PackageId,
  gateway: GatewayId.nullable().default(() => null),
  dns: OutboundDnsUpdate.nullable().default(() => null),
}).meta({
  id: 'SetOutboundGatewayParams',
})
export const SetStaticDnsParams: z.ZodObject<{
  gateway: typeof GatewayId
  servers: z.ZodDefault<z.ZodNullable<z.ZodArray<z.ZodString>>>
}> = /* @__PURE__ */ object({
  gateway: GatewayId,
  servers: z
    .array(z.string())
    .nullable()
    .default(() => null),
}).meta({
  id: 'SetStaticDnsParams',
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
export const SharedVolumeMountResult: z.ZodUnion<
  readonly [
    z.ZodObject<{ kind: z.ZodLiteral<'mounted'> }>,
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'unavailable'> }>,
      z.ZodObject<{ reason: typeof SharedVolumeUnavailableReason }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    object({ kind: z.literal('mounted') }),
    z.intersection(
      object({ kind: z.literal('unavailable') }),
      object({ reason: SharedVolumeUnavailableReason }),
    ),
  ])
  .meta({
    id: 'SharedVolumeMountResult',
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
        drive: z.ZodDefault<z.ZodNullable<typeof SharedVolumeDrive>>
      }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ kind: z.ZodLiteral<'cifs'> }>,
      z.ZodObject<{
        hostname: z.ZodString
        path: z.ZodString
        username: z.ZodDefault<z.ZodString>
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
        drive: SharedVolumeDrive.nullable()
          .default(() => null)
          .meta({
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
        username: z.string().default(() => ''),
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
  progress: z.ZodDefault<z.ZodNullable<typeof Guid>>
}> = /* @__PURE__ */ object({
  upload: Guid,
  /**
   * Legacy progress continuation, absent for rpcstream observers.
   */
  progress: Guid.nullable()
    .default(() => null)
    .meta({
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
  gpgExports: z.ZodDefault<z.ZodArray<typeof GpgPublicKeyExport>>
}> = /* @__PURE__ */ object({
  name: z.string(),
  contact: z.array(ContactInfo),
  keys: z.array(AnyVerifyingKey),
  gpgExports: z.array(GpgPublicKeyExport).default(() => []),
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
 * Authorized-key intent shared by SSH mutations and subscribed clients.
 */
export const SshKeys: z.ZodRecord<
  z.ZodString,
  z.ZodObject<{
    created_at: z.ZodString
    updated_at: z.ZodString
    value: typeof SshPubKey
  }>
> = /* @__PURE__ */ z
  .record(
    z.string(),
    object({
      created_at: z.string(),
      updated_at: z.string(),
      value: SshPubKey,
    }),
  )
  .meta({
    id: 'SshKeys',
    description:
      'Authorized-key intent shared by SSH mutations and subscribed clients.',
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
export const TaskInput: z.ZodCodec<
  typeof TaskInputRepr,
  z.ZodIntersection<
    z.ZodObject<{ kind: z.ZodLiteral<'partial'> }>,
    z.ZodObject<{ accept: typeof literal_6; set: typeof literal_1 }>
  >
> = /* @__PURE__ */ wire(
  TaskInputRepr,
  z.intersection(
    object({ kind: z.literal('partial') }),
    object({ accept: literal_6, set: literal_1 }),
  ),
).meta({
  id: 'TaskInput',
})
export const TaskTrigger: z.ZodObject<{
  once: z.ZodDefault<z.ZodBoolean>
  condition: typeof TaskCondition
}> = /* @__PURE__ */ object({
  once: z.boolean().default(() => false),
  condition: TaskCondition,
}).meta({
  id: 'TaskTrigger',
})

/**
 * One part's representative reading, with the limits it is judged against.
 */
export const TemperatureSensor: z.ZodObject<{
  /**
   * The lm-sensors chip name.
   */
  id: z.ZodString
  kind: typeof SensorKind
  temperature: typeof Celsius
  /**
   * Sustained readings at or above this are hot.
   */
  high: typeof Celsius
  critical: typeof Celsius
}> = /* @__PURE__ */ object({
  /**
   * The lm-sensors chip name.
   */
  id: z.string().meta({
    description: 'The lm-sensors chip name.',
  }),
  kind: SensorKind,
  temperature: Celsius,
  /**
   * Sustained readings at or above this are hot.
   */
  high: Celsius.meta({
    description: 'Sustained readings at or above this are hot.',
  }),
  critical: Celsius,
}).meta({
  id: 'TemperatureSensor',
  description:
    "One part's representative reading, with the limits it is judged against.",
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
export const TransportWire: z.ZodUnion<
  readonly [typeof TransportProtocol, z.ZodArray<typeof TransportProtocol>]
> = /* @__PURE__ */ z
  .union([TransportProtocol, z.array(TransportProtocol)])
  .meta({
    id: 'TransportWire',
  })
export const UmountParams: z.ZodObject<{
  targetId: z.ZodDefault<z.ZodNullable<typeof BackupTargetId>>
}> = /* @__PURE__ */ object({
  targetId: BackupTargetId.nullable().default(() => null),
}).meta({
  id: 'UmountParams',
})
export const UninstallParams: z.ZodObject<{
  id: typeof PackageId
  soft: z.ZodDefault<z.ZodBoolean>
  force: z.ZodDefault<z.ZodBoolean>
}> = /* @__PURE__ */ object({
  id: PackageId,
  soft: z.boolean().default(() => false),
  force: z.boolean().default(() => false),
}).meta({
  id: 'UninstallParams',
})
export const UnsetGatewaySecureParams: z.ZodObject<{
  gateway: typeof GatewayId
}> = /* @__PURE__ */ object({ gateway: GatewayId }).meta({
  id: 'UnsetGatewaySecureParams',
})
export const UpdateSystemRes: z.ZodObject<{
  target: z.ZodDefault<z.ZodNullable<z.ZodString>>
  /**
   * Legacy progress continuation, absent for rpcstream observers.
   */
  progress: z.ZodDefault<z.ZodNullable<typeof Guid>>
}> = /* @__PURE__ */ object({
  target: z
    .string()
    .nullable()
    .default(() => null),
  /**
   * Legacy progress continuation, absent for rpcstream observers.
   */
  progress: Guid.nullable()
    .default(() => null)
    .meta({
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
export const UploadedFile: z.ZodObject<{
  /**
   * Untrusted original filename supplied by the uploader.
   */
  name: z.ZodString
  /**
   * The actual volume-relative destination of the installed file.
   */
  path: z.ZodString
  /**
   * Untrusted Content-Type supplied by the uploader.
   */
  mime: z.ZodDefault<z.ZodNullable<z.ZodString>>
  commitment: typeof FileCommitment
}> = /* @__PURE__ */ object({
  /**
   * Untrusted original filename supplied by the uploader.
   */
  name: z.string().meta({
    description: 'Untrusted original filename supplied by the uploader.',
  }),
  /**
   * The actual volume-relative destination of the installed file.
   */
  path: z.string().meta({
    description:
      'The actual volume-relative destination of the installed file.',
  }),
  /**
   * Untrusted Content-Type supplied by the uploader.
   */
  mime: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description: 'Untrusted Content-Type supplied by the uploader.',
    }),
  commitment: FileCommitment,
}).meta({
  id: 'UploadedFile',
})
export const UrlPluginRegisterParams: z.ZodObject<{
  tableAction: typeof ActionId
}> = /* @__PURE__ */ object({ tableAction: ActionId }).meta({
  id: 'UrlPluginRegisterParams',
})
export const UrlPluginRegistration: z.ZodObject<{
  tableAction: typeof ActionId
}> = /* @__PURE__ */ object({ tableAction: ActionId }).meta({
  id: 'UrlPluginRegistration',
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

/**
 * A security vulnerability in a range of versions.
 */
export const Vulnerability: z.ZodObject<{
  /**
   * Versions the vulnerability affects.
   */
  affected: z.ZodString
  /**
   * When the affected versions are exploitable.
   */
  circumstances: typeof LocaleString
  /**
   * Damage a successful exploit can do.
   */
  impact: typeof LocaleString
  /**
   * Whether to stop the service until it leaves the affected range.
   */
  stopRecommended: z.ZodDefault<z.ZodBoolean>
}> = /* @__PURE__ */ object({
  /**
   * Versions the vulnerability affects.
   */
  affected: z.string().meta({
    description: 'Versions the vulnerability affects.',
  }),
  /**
   * When the affected versions are exploitable.
   */
  circumstances: LocaleString.meta({
    description: 'When the affected versions are exploitable.',
  }),
  /**
   * Damage a successful exploit can do.
   */
  impact: LocaleString.meta({
    description: 'Damage a successful exploit can do.',
  }),
  /**
   * Whether to stop the service until it leaves the affected range.
   */
  stopRecommended: z
    .boolean()
    .default(() => false)
    .meta({
      description:
        'Whether to stop the service until it leaves the affected range.',
    }),
}).meta({
  id: 'Vulnerability',
  description: 'A security vulnerability in a range of versions.',
})

/**
 * A newly trusted vulnerability affecting an installed package or this StartOS.
 */
export const VulnerabilityFound: z.ZodIntersection<
  z.ZodObject<{
    kind: z.ZodCodec<
      z.ZodOptional<z.ZodLiteral<'vulnerability-found'>>,
      z.ZodLiteral<'vulnerability-found'>
    >
  }>,
  z.ZodObject<{
    registry: z.ZodString
    /**
     * The affected package; absent for StartOS.
     */
    packageId: z.ZodDefault<z.ZodNullable<typeof PackageId>>
    version: typeof Version
    vulnerabilityId: z.ZodString
    stopRecommended: z.ZodBoolean
  }>
> = /* @__PURE__ */ z
  .intersection(
    object({
      kind: wire(
        z.literal('vulnerability-found').optional(),
        z.literal('vulnerability-found'),
      ),
    }),
    object({
      registry: z.string(),
      /**
       * The affected package; absent for StartOS.
       */
      packageId: PackageId.nullable()
        .default(() => null)
        .meta({
          description: 'The affected package; absent for StartOS.',
        }),
      version: Version,
      vulnerabilityId: z.string(),
      stopRecommended: z.boolean(),
    }),
  )
  .meta({
    id: 'VulnerabilityFound',
    description:
      'A newly trusted vulnerability affecting an installed package or this StartOS.',
  })
export const WifiInfo: z.ZodObject<{
  enabled: z.ZodBoolean
  interface: z.ZodDefault<z.ZodNullable<typeof GatewayId>>
  ssids: z.ZodArray<z.ZodString>
  selected: z.ZodDefault<z.ZodNullable<z.ZodString>>
  lastRegion: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  enabled: z.boolean(),
  interface: GatewayId.nullable().default(() => null),
  ssids: z.array(z.string()),
  selected: z
    .string()
    .nullable()
    .default(() => null),
  lastRegion: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'WifiInfo',
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
    description: z.ZodDefault<z.ZodNullable<z.ZodString>>
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
      description: z
        .string()
        .nullable()
        .default(() => null)
        .meta({
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
  message: z.ZodDefault<z.ZodNullable<z.ZodString>>
  /**
   * (optional) Structured data to present inside the modal
   */
  result: z.ZodDefault<z.ZodNullable<typeof ActionResultValue>>
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
  message: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description:
        '(optional) A general message for the user, just under the title. Rendered as Markdown, with a single line break kept as a line break.',
    }),
  /**
   * (optional) Structured data to present inside the modal
   */
  result: ActionResultValue.nullable()
    .default(() => null)
    .meta({
      description: '(optional) Structured data to present inside the modal',
    }),
}).meta({
  id: 'ActionResultV1',
})
export const AddOsVulnerabilityParams: z.ZodObject<{
  vulnerabilityId: z.ZodString
  vulnerability: typeof Vulnerability
  signature: typeof AnySignature
}> = /* @__PURE__ */ object({
  vulnerabilityId: z.string(),
  vulnerability: Vulnerability,
  signature: AnySignature,
}).meta({
  id: 'AddOsVulnerabilityParams',
})
export const AddPackageVulnerabilityParams: z.ZodObject<{
  id: typeof PackageId
  vulnerabilityId: z.ZodString
  vulnerability: typeof Vulnerability
  signature: typeof AnySignature
}> = /* @__PURE__ */ object({
  id: PackageId,
  vulnerabilityId: z.string(),
  vulnerability: Vulnerability,
  signature: AnySignature,
}).meta({
  id: 'AddPackageVulnerabilityParams',
})
export const AddPublicDomainRes: z.ZodObject<{
  dns: typeof QueryDnsRes
  port: typeof CheckPortRes
  portV6: z.ZodDefault<z.ZodNullable<typeof CheckPortV6Res>>
  /**
   * The authority's own reachability requirement, where the domain has one.
   */
  challenge: z.ZodDefault<z.ZodNullable<typeof CheckChallengeRes>>
}> = /* @__PURE__ */ object({
  dns: QueryDnsRes,
  port: CheckPortRes,
  portV6: CheckPortV6Res.nullable().default(() => null),
  /**
   * The authority's own reachability requirement, where the domain has one.
   */
  challenge: CheckChallengeRes.nullable()
    .default(() => null)
    .meta({
      description:
        "The authority's own reachability requirement, where the domain has one.",
    }),
}).meta({
  id: 'AddPublicDomainRes',
})
export const AttachParams: z.ZodObject<{
  password: z.ZodDefault<z.ZodNullable<typeof PasswordType>>
  guid: z.ZodString
  kiosk: z.ZodBoolean
}> = /* @__PURE__ */ object({
  password: PasswordType.nullable().default(() => null),
  guid: z.string(),
  kiosk: z.boolean(),
}).meta({
  id: 'AttachParams',
})
export const BackupInfo: z.ZodObject<{
  version: z.ZodString
  timestamp: z.ZodDefault<z.ZodNullable<z.ZodString>>
  packageBackups: z.ZodRecord<z.ZodString, typeof PackageBackupInfo>
  sharedVolumeBackups: z.ZodDefault<
    z.ZodRecord<z.ZodString, typeof SharedVolumeBackupInfo>
  >
}> = /* @__PURE__ */ object({
  version: z.string(),
  timestamp: z
    .string()
    .nullable()
    .default(() => null),
  packageBackups: z.record(z.string(), PackageBackupInfo),
  sharedVolumeBackups: z
    .record(z.string(), SharedVolumeBackupInfo)
    .default(() => ({})),
}).meta({
  id: 'BackupInfo',
})
export const BackupParams: z.ZodObject<{
  targetId: typeof BackupTargetId
  oldPassword: z.ZodDefault<z.ZodNullable<typeof PasswordType>>
  packageIds: z.ZodDefault<z.ZodNullable<z.ZodArray<typeof PackageId>>>
  sharedVolumeIds: z.ZodDefault<
    z.ZodNullable<z.ZodArray<typeof SharedVolumeId>>
  >
  password: typeof PasswordType
}> = /* @__PURE__ */ object({
  targetId: BackupTargetId,
  oldPassword: PasswordType.nullable().default(() => null),
  packageIds: z
    .array(PackageId)
    .nullable()
    .default(() => null),
  sharedVolumeIds: z
    .array(SharedVolumeId)
    .nullable()
    .default(() => null),
  password: PasswordType,
}).meta({
  id: 'BackupParams',
})
export const BackupReport: z.ZodObject<{
  server: typeof ServerBackupReport
  packages: z.ZodRecord<z.ZodString, typeof PackageBackupReport>
  sharedVolumes: z.ZodDefault<
    z.ZodRecord<z.ZodString, typeof VolumeBackupReport>
  >
}> = /* @__PURE__ */ object({
  server: ServerBackupReport,
  packages: z.record(z.string(), PackageBackupReport),
  sharedVolumes: z.record(z.string(), VolumeBackupReport).default(() => ({})),
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
    startOs: z.ZodDefault<z.ZodRecord<z.ZodString, typeof StartOsRecoveryInfo>>
    legacyBackup: z.ZodDefault<z.ZodBoolean>
  }>,
  typeof SharedVolumeInspection
> = /* @__PURE__ */ z
  .intersection(
    object({
      name: z.string(),
      source: SharedVolumeSource,
      startOs: z.record(z.string(), StartOsRecoveryInfo).default(() => ({})),
      legacyBackup: z.boolean().default(() => false),
    }),
    SharedVolumeInspection,
  )
  .meta({
    id: 'BackupTarget',
    description:
      'A registered backup destination and its last source observation.',
  })
export const ClearBindingsParams: z.ZodObject<{
  except: z.ZodDefault<z.ZodArray<typeof BindId>>
}> = /* @__PURE__ */ object({ except: z.array(BindId).default(() => []) }).meta(
  {
    id: 'ClearBindingsParams',
  },
)
export const CompatibleAlpnInfo: z.ZodUnion<
  readonly [z.ZodArray<typeof MaybeUtf8String>, typeof LegacyAlpnInfo]
> = /* @__PURE__ */ z.union([z.array(MaybeUtf8String), LegacyAlpnInfo]).meta({
  id: 'CompatibleAlpnInfo',
})

/**
 * Latest observed conversion attempt for one target/server backup.
 */
export const ConversionJob: z.ZodObject<{
  state: typeof ConversionState
  progress: typeof FullProgress
  error: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  state: ConversionState,
  progress: FullProgress,
  error: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'ConversionJob',
  description:
    'Latest observed conversion attempt for one target/server backup.',
})
export const CurrentDependencyInfo: z.ZodIntersection<
  z.ZodObject<{
    title: z.ZodDefault<z.ZodNullable<typeof LocaleString>>
    versionRange: z.ZodString
  }>,
  typeof CurrentDependencyKind
> = /* @__PURE__ */ z
  .intersection(
    object({
      title: LocaleString.nullable().default(() => null),
      versionRange: z.string(),
    }),
    CurrentDependencyKind,
  )
  .meta({
    id: 'CurrentDependencyInfo',
  })
export const DependencyMetadata: z.ZodIntersection<
  z.ZodObject<{
    title: z.ZodDefault<z.ZodNullable<typeof LocaleString>>
    icon: z.ZodOptional<z.ZodNullable<typeof PackageIcon>>
    description: z.ZodDefault<z.ZodNullable<typeof LocaleString>>
    optional: z.ZodBoolean
    versionRange: z.ZodDefault<z.ZodNullable<z.ZodString>>
  }>,
  z.ZodUnion<
    readonly [z.ZodNullable<typeof CurrentDependencyKind>, typeof empty]
  >
> = /* @__PURE__ */ z
  .intersection(
    object({
      title: LocaleString.nullable().default(() => null),
      icon: PackageIcon.nullable().optional(),
      description: LocaleString.nullable().default(() => null),
      optional: z.boolean(),
      versionRange: z
        .string()
        .nullable()
        .default(() => null),
    }),
    z.union([CurrentDependencyKind.nullable(), empty]),
  )
  .meta({
    id: 'DependencyMetadata',
  })
export const ExportActionParams: z.ZodObject<{
  id: typeof ActionId
  metadata: typeof ActionMetadata
}> = /* @__PURE__ */ object({ id: ActionId, metadata: ActionMetadata }).meta({
  id: 'ExportActionParams',
})
export const ExportServiceInterfaceParams: z.ZodObject<{
  id: typeof ServiceInterfaceId
  name: z.ZodString
  description: z.ZodString
  masked: z.ZodBoolean
  addressInfo: typeof AddressInfo
  type: typeof ServiceInterfaceType
  /**
   * The interface address Open UI should prefer.
   */
  preferredLauncherAddress: z.ZodDefault<z.ZodNullable<z.ZodString>>
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
  preferredLauncherAddress: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description: 'The interface address Open UI should prefer.',
    }),
}).meta({
  id: 'ExportServiceInterfaceParams',
})
export const HostnameInfo: z.ZodObject<{
  ssl: z.ZodBoolean
  public: z.ZodBoolean
  hostname: z.ZodString
  port: z.ZodDefault<z.ZodNullable<z.ZodInt>>
  metadata: typeof HostnameMetadata
}> = /* @__PURE__ */ object({
  ssl: z.boolean(),
  public: z.boolean(),
  hostname: z.string(),
  port: z
    .int()
    .nullable()
    .default(() => null),
  metadata: HostnameMetadata,
}).meta({
  id: 'HostnameInfo',
})
export const ImageConfigRepr: z.ZodObject<{
  source: typeof ImageSource
  arch: z.ZodArray<z.ZodString>
  emulateMissing: z.ZodDefault<z.ZodNullable<z.ZodBoolean>>
  emulateMissingAs: z.ZodDefault<z.ZodNullable<z.ZodString>>
  nvidiaContainer: z.ZodDefault<z.ZodBoolean>
}> = /* @__PURE__ */ object({
  source: ImageSource,
  arch: z.array(z.string()),
  emulateMissing: z
    .boolean()
    .nullable()
    .default(() => null),
  emulateMissingAs: z
    .string()
    .nullable()
    .default(() => null),
  nvidiaContainer: z.boolean().default(() => false),
}).meta({
  id: 'ImageConfigRepr',
})
export const InitProgressRes: z.ZodObject<{
  progress: typeof FullProgress
  /**
   * Present for ordinary replies; typed streams use same-ID events.
   */
  guid: z.ZodDefault<z.ZodNullable<typeof Guid>>
}> = /* @__PURE__ */ object({
  progress: FullProgress,
  /**
   * Present for ordinary replies; typed streams use same-ID events.
   */
  guid: Guid.nullable()
    .default(() => null)
    .meta({
      description:
        'Present for ordinary replies; typed streams use same-ID events.',
    }),
}).meta({
  id: 'InitProgressRes',
})
export const InstallParams: z.ZodObject<{
  acceptSigners: z.ZodDefault<z.ZodNullable<typeof AcceptSigners>>
  registry: z.ZodString
  id: typeof PackageId
  version: typeof Version
  /**
   * Accepts an untrusted signer without skipping signature or integrity verification.
   */
  trust: z.ZodDefault<z.ZodBoolean>
}> = /* @__PURE__ */ object({
  acceptSigners: AcceptSigners.nullable().default(() => null),
  registry: z.string(),
  id: PackageId,
  version: Version,
  /**
   * Accepts an untrusted signer without skipping signature or integrity verification.
   */
  trust: z
    .boolean()
    .default(() => false)
    .meta({
      description:
        'Accepts an untrusted signer without skipping signature or integrity verification.',
    }),
}).meta({
  id: 'InstallParams',
})

/**
 * Enrolled keys, including ephemeral ones, and the caller's own.
 */
export const KeyList: z.ZodObject<{
  current: z.ZodDefault<z.ZodNullable<z.ZodString>>
  keys: typeof AuthKeys
}> = /* @__PURE__ */ object({
  current: z
    .string()
    .nullable()
    .default(() => null),
  keys: AuthKeys,
}).meta({
  id: 'KeyList',
  description: "Enrolled keys, including ephemeral ones, and the caller's own.",
})
export const MetadataSrc: z.ZodUnion<
  readonly [
    z.ZodObject<{ metadata: typeof Metadata }>,
    z.ZodObject<{ s9pk: z.ZodNullable<typeof PathOrUrl> }>,
  ]
> = /* @__PURE__ */ z
  .union([
    object({ metadata: Metadata }),
    object({ s9pk: PathOrUrl.nullable() }),
  ])
  .meta({
    id: 'MetadataSrc',
  })
export const MetricsGeneral: z.ZodObject<{
  temperatures: z.ZodDefault<z.ZodArray<typeof TemperatureSensor>>
}> = /* @__PURE__ */ object({
  temperatures: z.array(TemperatureSensor).default(() => []),
}).meta({
  id: 'MetricsGeneral',
})
export const MountParams: z.ZodObject<{
  location: z.ZodString
  target: typeof MountTarget
}> = /* @__PURE__ */ object({ location: z.string(), target: MountTarget }).meta(
  {
    id: 'MountParams',
  },
)
export const NamedHealthCheckResult: z.ZodCodec<
  z.ZodIntersection<
    z.ZodObject<{ name: z.ZodString }>,
    typeof CompatibleHealthCheckResultKind
  >,
  z.ZodIntersection<
    z.ZodObject<{ name: z.ZodString }>,
    typeof NamedHealthCheckResultKind
  >
> = /* @__PURE__ */ wire(
  z.intersection(object({ name: z.string() }), CompatibleHealthCheckResultKind),
  z.intersection(object({ name: z.string() }), NamedHealthCheckResultKind),
).meta({
  id: 'NamedHealthCheckResult',
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
  enabled: z.ZodDefault<z.ZodBoolean>
  /**
   * Retained server or client settings, including while monitoring is disabled.
   */
  settings: z.ZodDefault<z.ZodNullable<typeof NutSettings>>
}> = /* @__PURE__ */ object({
  /**
   * Whether StartOS should run NUT monitoring.
   */
  enabled: z
    .boolean()
    .default(() => false)
    .meta({
      description: 'Whether StartOS should run NUT monitoring.',
    }),
  /**
   * Retained server or client settings, including while monitoring is disabled.
   */
  settings: NutSettings.nullable()
    .default(() => null)
    .meta({
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
export const PackageMetadata: z.ZodObject<{
  title: z.ZodString
  /**
   * Locale this package's `LocaleString`s fall back to when a requested one is missing.
   */
  defaultLocale: z.ZodOptional<z.ZodNullable<z.ZodString>>
  description: typeof Description
  releaseNotes: typeof LocaleString
  preDownloadAlert: z.ZodOptional<z.ZodNullable<typeof PreDownloadAlert>>
  gitHash: z.ZodDefault<z.ZodNullable<typeof GitHash>>
  license: z.ZodString
  packageRepo: z.ZodCodec<z.ZodOptional<z.ZodString>, z.ZodString>
  upstreamRepo: z.ZodString
  marketingUrl: z.ZodDefault<z.ZodNullable<z.ZodString>>
  donationUrl: z.ZodDefault<z.ZodNullable<z.ZodString>>
  osVersion: z.ZodCodec<z.ZodOptional<z.ZodString>, z.ZodString>
  sdkVersion: z.ZodDefault<z.ZodNullable<z.ZodString>>
  hardwareAcceleration: z.ZodDefault<z.ZodBoolean>
  /**
   * Grants access to `/dev/fuse`.
   */
  userspaceFilesystems: z.ZodDefault<z.ZodBoolean>
  /**
   * Grants access to `/dev/net/tun`.
   */
  virtualNetworking: z.ZodDefault<z.ZodBoolean>
  /**
   * Grants /dev/kvm when present. The opening process must run as container root.
   */
  hardwareVirtualization: z.ZodDefault<z.ZodBoolean>
  plugins: z.ZodDefault<z.ZodArray<typeof PluginId>>
  satisfies: z.ZodDefault<z.ZodArray<typeof Version>>
}> = /* @__PURE__ */ object({
  title: z.string(),
  /**
   * Locale this package's `LocaleString`s fall back to when a requested one is missing.
   */
  defaultLocale: z.string().nullable().optional().meta({
    description:
      "Locale this package's `LocaleString`s fall back to when a requested one is missing.",
  }),
  description: Description,
  releaseNotes: LocaleString,
  preDownloadAlert: PreDownloadAlert.nullable().optional(),
  gitHash: GitHash.nullable().default(() => null),
  license: z.string(),
  packageRepo: wire(z.string().optional(), z.string()),
  upstreamRepo: z.string(),
  marketingUrl: z
    .string()
    .nullable()
    .default(() => null),
  donationUrl: z
    .string()
    .nullable()
    .default(() => null),
  osVersion: wire(z.string().optional(), z.string()),
  sdkVersion: z
    .string()
    .nullable()
    .default(() => null),
  hardwareAcceleration: z.boolean().default(() => false),
  /**
   * Grants access to `/dev/fuse`.
   */
  userspaceFilesystems: z
    .boolean()
    .default(() => false)
    .meta({
      description: 'Grants access to `/dev/fuse`.',
    }),
  /**
   * Grants access to `/dev/net/tun`.
   */
  virtualNetworking: z
    .boolean()
    .default(() => false)
    .meta({
      description: 'Grants access to `/dev/net/tun`.',
    }),
  /**
   * Grants /dev/kvm when present. The opening process must run as container root.
   */
  hardwareVirtualization: z
    .boolean()
    .default(() => false)
    .meta({
      description:
        'Grants /dev/kvm when present. The opening process must run as container root.',
    }),
  plugins: z.array(PluginId).default(() => []),
  satisfies: z.array(Version).default(() => []),
}).meta({
  id: 'PackageMetadata',
})
export const PackagePlugin: z.ZodObject<{
  url: z.ZodDefault<z.ZodNullable<typeof UrlPluginRegistration>>
}> = /* @__PURE__ */ object({
  url: UrlPluginRegistration.nullable().default(() => null),
}).meta({
  id: 'PackagePlugin',
})
export const PartitionInfo: z.ZodObject<{
  logicalname: z.ZodString
  stablePath: z.ZodString
  label: z.ZodDefault<z.ZodNullable<z.ZodString>>
  capacity: z.ZodInt
  used: z.ZodDefault<z.ZodNullable<z.ZodInt>>
  available: z.ZodDefault<z.ZodNullable<z.ZodInt>>
  startOs: z.ZodRecord<z.ZodString, typeof StartOsRecoveryInfo>
  legacyBackup: z.ZodBoolean
  guid: z.ZodDefault<z.ZodNullable<z.ZodString>>
  filesystem: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  logicalname: z.string(),
  stablePath: z.string(),
  label: z
    .string()
    .nullable()
    .default(() => null),
  capacity: z.int(),
  used: z
    .int()
    .nullable()
    .default(() => null),
  available: z
    .int()
    .nullable()
    .default(() => null),
  startOs: z.record(z.string(), StartOsRecoveryInfo),
  legacyBackup: z.boolean(),
  guid: z
    .string()
    .nullable()
    .default(() => null),
  filesystem: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'PartitionInfo',
})
export const Progress: z.ZodUnion<
  readonly [
    z.ZodNull,
    z.ZodBoolean,
    z.ZodObject<{
      done: z.ZodInt
      total: z.ZodDefault<z.ZodNullable<z.ZodInt>>
      units: z.ZodDefault<z.ZodNullable<typeof ProgressUnits>>
    }>,
    typeof FullProgress,
  ]
> = /* @__PURE__ */ z
  .union([
    z.null(),
    z.boolean(),
    object({
      done: z.int(),
      total: z
        .int()
        .nullable()
        .default(() => null),
      units: ProgressUnits.nullable().default(() => null),
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
    addXForwardedHeaders: z.ZodDefault<z.ZodBoolean>
    /**
     * Validates every request before forwarding. Basic auth sets `X-Forwarded-User`.
     */
    auth: z.ZodDefault<z.ZodNullable<typeof ProxyAuth>>
  }>
> = /* @__PURE__ */ z
  .intersection(
    object({ type: z.literal('http') }),
    object({
      /**
       * Adds `X-Forwarded-For` and, on the SSL leg, `X-Forwarded-Proto: https`.
       * Client-supplied `X-Forwarded-*` headers are stripped on both legs.
       */
      addXForwardedHeaders: z
        .boolean()
        .default(() => false)
        .meta({
          description:
            'Adds `X-Forwarded-For` and, on the SSL leg, `X-Forwarded-Proto: https`.\nClient-supplied `X-Forwarded-*` headers are stripped on both legs.',
        }),
      /**
       * Validates every request before forwarding. Basic auth sets `X-Forwarded-User`.
       */
      auth: ProxyAuth.nullable()
        .default(() => null)
        .meta({
          description:
            'Validates every request before forwarding. Basic auth sets `X-Forwarded-User`.',
        }),
    }),
  )
  .meta({
    id: 'ProxyOptions',
  })
export const RegistryInfo: z.ZodObject<{
  name: z.ZodDefault<z.ZodNullable<z.ZodString>>
  icon: z.ZodDefault<z.ZodNullable<typeof DataUrl>>
  /**
   * Markdown, shown above the registry's services.
   */
  description: z.ZodDefault<z.ZodNullable<typeof LocaleString>>
  /**
   * `FullIndex::default_locale`.
   */
  defaultLocale: z.ZodDefault<z.ZodNullable<z.ZodString>>
  categories: z.ZodRecord<z.ZodString, typeof Category>
  /**
   * Registry-selected signers, not an assertion of trust.
   */
  trustSigners: z.ZodDefault<z.ZodRecord<z.ZodString, typeof SignerInfo>>
}> = /* @__PURE__ */ object({
  name: z
    .string()
    .nullable()
    .default(() => null),
  icon: DataUrl.nullable().default(() => null),
  /**
   * Markdown, shown above the registry's services.
   */
  description: LocaleString.nullable()
    .default(() => null)
    .meta({
      description: "Markdown, shown above the registry's services.",
    }),
  /**
   * `FullIndex::default_locale`.
   */
  defaultLocale: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description: '`FullIndex::default_locale`.',
    }),
  categories: z.record(z.string(), Category),
  /**
   * Registry-selected signers, not an assertion of trust.
   */
  trustSigners: z
    .record(z.string(), SignerInfo)
    .default(() => ({}))
    .meta({
      description: 'Registry-selected signers, not an assertion of trust.',
    }),
}).meta({
  id: 'RegistryInfo',
})
export const ResetPasswordParams: z.ZodObject<{
  newPassword: z.ZodDefault<z.ZodNullable<typeof PasswordType>>
}> = /* @__PURE__ */ object({
  newPassword: PasswordType.nullable().default(() => null),
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
  preferredLauncherAddress: z.ZodDefault<z.ZodNullable<z.ZodString>>
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
  preferredLauncherAddress: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
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
export const SetDependenciesParams: z.ZodObject<{
  dependencies: z.ZodArray<typeof DependencyRequirement>
}> = /* @__PURE__ */ object({
  dependencies: z.array(DependencyRequirement),
}).meta({
  id: 'SetDependenciesParams',
})
export const SetupExecuteParams: z.ZodObject<{
  guid: z.ZodString
  password: z.ZodDefault<z.ZodNullable<typeof PasswordType>>
  recoverySource: z.ZodDefault<
    z.ZodNullable<ReturnType<typeof RecoverySource<typeof PasswordType>>>
  >
  kiosk: z.ZodBoolean
  hostname: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  guid: z.string(),
  password: PasswordType.nullable().default(() => null),
  recoverySource: RecoverySource(PasswordType)
    .nullable()
    .default(() => null),
  kiosk: z.boolean(),
  hostname: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'SetupExecuteParams',
})
export const SetupProgress: z.ZodObject<{
  progress: typeof FullProgress
  /**
   * Present for ordinary replies; typed streams use same-ID events.
   */
  guid: z.ZodDefault<z.ZodNullable<typeof Guid>>
}> = /* @__PURE__ */ object({
  progress: FullProgress,
  /**
   * Present for ordinary replies; typed streams use same-ID events.
   */
  guid: Guid.nullable()
    .default(() => null)
    .meta({
      description:
        'Present for ordinary replies; typed streams use same-ID events.',
    }),
}).meta({
  id: 'SetupProgress',
})
export const SharedVolume: z.ZodIntersection<
  z.ZodObject<{
    /**
     * A unique single folder name; services mount a volume at a path matching it.
     */
    name: z.ZodString
    source: typeof SharedVolumeSource
    grants: z.ZodRecord<z.ZodString, typeof SharedVolumeGrant>
  }>,
  typeof SharedVolumeInspection
> = /* @__PURE__ */ z
  .intersection(
    object({
      /**
       * A unique single folder name; services mount a volume at a path matching it.
       */
      name: z.string().meta({
        description:
          'A unique single folder name; services mount a volume at a path matching it.',
      }),
      source: SharedVolumeSource,
      grants: z.record(z.string(), SharedVolumeGrant),
    }),
    SharedVolumeInspection,
  )
  .meta({
    id: 'SharedVolume',
  })
export const SideloadParams: z.ZodObject<{
  acceptSigners: z.ZodDefault<z.ZodNullable<typeof AcceptSigners>>
  /**
   * DER64 supplemental signatures for the uploaded archive.
   */
  signatures: z.ZodDefault<z.ZodRecord<z.ZodString, typeof AnySignature>>
  /**
   * Accepts an untrusted signer without skipping signature or integrity verification.
   */
  trust: z.ZodDefault<z.ZodBoolean>
}> = /* @__PURE__ */ object({
  acceptSigners: AcceptSigners.nullable().default(() => null),
  /**
   * DER64 supplemental signatures for the uploaded archive.
   */
  signatures: z
    .record(z.string(), AnySignature)
    .default(() => ({}))
    .meta({
      description: 'DER64 supplemental signatures for the uploaded archive.',
    }),
  /**
   * Accepts an untrusted signer without skipping signature or integrity verification.
   */
  trust: z
    .boolean()
    .default(() => false)
    .meta({
      description:
        'Accepts an untrusted signer without skipping signature or integrity verification.',
    }),
}).meta({
  id: 'SideloadParams',
})

/**
 * A `Vulnerability` as a registry publishes it.
 */
export const SignedVulnerability: z.ZodIntersection<
  z.ZodObject<{
    publishedAt: z.ZodString
    signatures: z.ZodRecord<z.ZodString, typeof AnySignature>
  }>,
  typeof Vulnerability
> = /* @__PURE__ */ z
  .intersection(
    object({
      publishedAt: z.string(),
      signatures: z.record(z.string(), AnySignature),
    }),
    Vulnerability,
  )
  .meta({
    id: 'SignedVulnerability',
    description: 'A `Vulnerability` as a registry publishes it.',
  })
export const Task: z.ZodObject<{
  packageId: typeof PackageId
  actionId: typeof ActionId
  severity: z.ZodCodec<z.ZodOptional<typeof TaskSeverity>, typeof TaskSeverity>
  reason: z.ZodDefault<z.ZodNullable<z.ZodString>>
  when: z.ZodDefault<z.ZodNullable<typeof TaskTrigger>>
  input: z.ZodDefault<z.ZodNullable<typeof TaskInput>>
}> = /* @__PURE__ */ object({
  packageId: PackageId,
  actionId: ActionId,
  severity: wire(TaskSeverity.optional(), TaskSeverity),
  reason: z
    .string()
    .nullable()
    .default(() => null),
  when: TaskTrigger.nullable().default(() => null),
  input: TaskInput.nullable().default(() => null),
}).meta({
  id: 'Task',
})

/**
 * Non-empty selected transports, serialized as a sorted, deduplicated array.
 * A protocol string is accepted as a singleton. Omission selects TCP and UDP.
 */
export const Transport: z.ZodCodec<
  typeof TransportWire,
  z.ZodArray<typeof TransportProtocol>
> = /* @__PURE__ */ wire(TransportWire, z.array(TransportProtocol)).meta({
  id: 'Transport',
  description:
    'Non-empty selected transports, serialized as a sorted, deduplicated array.\nA protocol string is accepted as a singleton. Omission selects TCP and UDP.',
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
  originRegistry: z.ZodDefault<z.ZodNullable<typeof RegistryIdentity>>
  publicKey: typeof AnyVerifyingKey
  /**
   * Original GPG exports; signing trust is determined by `public_key`.
   */
  gpgExports: z.ZodDefault<z.ZodArray<typeof GpgPublicKeyExport>>
  /**
   * Contact details registries publish for the key's signer.
   */
  contact: z.ZodDefault<z.ZodArray<typeof ContactInfo>>
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
  originRegistry: RegistryIdentity.nullable()
    .default(() => null)
    .meta({
      description:
        'Registry identity recorded when this key was first imported.',
    }),
  publicKey: AnyVerifyingKey,
  /**
   * Original GPG exports; signing trust is determined by `public_key`.
   */
  gpgExports: z
    .array(GpgPublicKeyExport)
    .default(() => [])
    .meta({
      description:
        'Original GPG exports; signing trust is determined by `public_key`.',
    }),
  /**
   * Contact details registries publish for the key's signer.
   */
  contact: z
    .array(ContactInfo)
    .default(() => [])
    .meta({
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
  /**
   * Fetches once from this registry instead of using the selected update registry's
   * synchronized releases.
   */
  registry: z.ZodDefault<z.ZodNullable<z.ZodString>>
  /**
   * Additional signer requirements checked before starting the update.
   */
  acceptSigners: z.ZodDefault<z.ZodNullable<typeof AcceptSigners>>
  /**
   * Accepts an untrusted signer without skipping signature or integrity verification.
   */
  trust: z.ZodDefault<z.ZodBoolean>
  targetVersion: z.ZodDefault<z.ZodNullable<z.ZodString>>
  /**
   * Requests a legacy progress GUID; rpcstream observes completion independently.
   */
  progress: z.ZodDefault<z.ZodBoolean>
}> = /* @__PURE__ */ object({
  /**
   * Fetches once from this registry instead of using the selected update registry's
   * synchronized releases.
   */
  registry: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description:
        "Fetches once from this registry instead of using the selected update registry's\nsynchronized releases.",
    }),
  /**
   * Additional signer requirements checked before starting the update.
   */
  acceptSigners: AcceptSigners.nullable()
    .default(() => null)
    .meta({
      description:
        'Additional signer requirements checked before starting the update.',
    }),
  /**
   * Accepts an untrusted signer without skipping signature or integrity verification.
   */
  trust: z
    .boolean()
    .default(() => false)
    .meta({
      description:
        'Accepts an untrusted signer without skipping signature or integrity verification.',
    }),
  targetVersion: z
    .string()
    .nullable()
    .default(() => null),
  /**
   * Requests a legacy progress GUID; rpcstream observes completion independently.
   */
  progress: z
    .boolean()
    .default(() => false)
    .meta({
      description:
        'Requests a legacy progress GUID; rpcstream observes completion independently.',
    }),
}).meta({
  id: 'UpdateSystemParams',
})
export const UrlPluginClearUrlsParams: z.ZodObject<{
  except: z.ZodArray<typeof PluginHostnameInfo>
}> = /* @__PURE__ */ object({ except: z.array(PluginHostnameInfo) }).meta({
  id: 'UrlPluginClearUrlsParams',
})
export const UrlPluginExportUrlParams: z.ZodObject<{
  hostnameInfo: typeof PluginHostnameInfo
  removeAction: z.ZodDefault<z.ZodNullable<typeof ActionId>>
  overflowActions: z.ZodArray<typeof ActionId>
}> = /* @__PURE__ */ object({
  hostnameInfo: PluginHostnameInfo,
  removeAction: ActionId.nullable().default(() => null),
  overflowActions: z.array(ActionId),
}).meta({
  id: 'UrlPluginExportUrlParams',
})
export const VerifyCifsParams: z.ZodObject<{
  hostname: z.ZodString
  path: z.ZodString
  username: z.ZodString
  password: z.ZodDefault<z.ZodNullable<typeof PasswordType>>
}> = /* @__PURE__ */ object({
  hostname: z.string(),
  path: z.string(),
  username: z.string(),
  password: PasswordType.nullable().default(() => null),
}).meta({
  id: 'VerifyCifsParams',
})
export const WifiListInfo: z.ZodObject<{
  ssids: z.ZodRecord<z.ZodString, typeof SignalStrength>
  connected: z.ZodDefault<z.ZodNullable<typeof Ssid>>
  country: z.ZodDefault<z.ZodNullable<z.ZodString>>
  ethernet: z.ZodBoolean
  availableWifi: z.ZodArray<typeof WifiListOut>
}> = /* @__PURE__ */ object({
  ssids: z.record(z.string(), SignalStrength),
  connected: Ssid.nullable().default(() => null),
  country: z
    .string()
    .nullable()
    .default(() => null),
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
export const BindRangeParams: z.ZodObject<{
  id: typeof HostId
  internalStartPort: z.ZodInt
  externalStartPort: z.ZodInt
  numberOfPorts: z.ZodInt
  transport: z.ZodCodec<z.ZodOptional<typeof Transport>, typeof Transport>
}> = /* @__PURE__ */ object({
  id: HostId,
  internalStartPort: z.int(),
  externalStartPort: z.int(),
  numberOfPorts: z.int(),
  transport: wire(Transport.optional(), Transport),
}).meta({
  id: 'BindRangeParams',
})
export const BindingSetAddressEnabledParams: z.ZodObject<{
  internalPort: z.ZodInt
  address: typeof HostnameInfo
  enabled: z.ZodDefault<z.ZodNullable<z.ZodBoolean>>
  /**
   * Accepts readable and alterable plaintext traffic, including passwords.
   */
  allowInsecure: z.ZodDefault<z.ZodBoolean>
}> = /* @__PURE__ */ object({
  internalPort: z.int(),
  address: HostnameInfo,
  enabled: z
    .boolean()
    .nullable()
    .default(() => null),
  /**
   * Accepts readable and alterable plaintext traffic, including passwords.
   */
  allowInsecure: z
    .boolean()
    .default(() => false)
    .meta({
      description:
        'Accepts readable and alterable plaintext traffic, including passwords.',
    }),
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
export const CreateTaskParams: z.ZodIntersection<
  z.ZodObject<{ replayId: typeof ReplayId }>,
  typeof Task
> = /* @__PURE__ */ z.intersection(object({ replayId: ReplayId }), Task).meta({
  id: 'CreateTaskParams',
})
export const CurrentDependencies: z.ZodRecord<
  z.ZodString,
  typeof CurrentDependencyInfo
> = /* @__PURE__ */ z.record(z.string(), CurrentDependencyInfo).meta({
  id: 'CurrentDependencies',
})
export const DepInfo: z.ZodIntersection<
  z.ZodIntersection<
    z.ZodObject<{
      description: z.ZodDefault<z.ZodNullable<typeof LocaleString>>
      optional: z.ZodBoolean
      versionRange: z.ZodDefault<z.ZodNullable<z.ZodString>>
    }>,
    z.ZodUnion<
      readonly [z.ZodNullable<typeof CurrentDependencyKind>, typeof empty]
    >
  >,
  z.ZodUnion<readonly [z.ZodNullable<typeof MetadataSrc>, typeof empty]>
> = /* @__PURE__ */ z
  .intersection(
    z.intersection(
      object({
        description: LocaleString.nullable().default(() => null),
        optional: z.boolean(),
        versionRange: z
          .string()
          .nullable()
          .default(() => null),
      }),
      z.union([CurrentDependencyKind.nullable(), empty]),
    ),
    z.union([MetadataSrc.nullable(), empty]),
  )
  .meta({
    id: 'DepInfo',
  })
export const DerivedAddressInfo: z.ZodObject<{
  /**
   * User override: enable these addresses (only for public IP & port)
   * An opt-in names one literal address and lapses when the gateway renumbers.
   * Only an enabled plain public domain carries its group's public IPs across.
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
  guaWan: z.ZodDefault<z.ZodArray<z.ZodString>>
  /**
   * User override: enable these private IPs regardless of their mDNS address.
   */
  lanEnabled: z.ZodDefault<
    z.ZodArray<z.ZodTuple<[z.ZodString, z.ZodInt], null>>
  >
  /**
   * Plaintext private addresses requiring consent, keyed by hostname, port and gateway.
   */
  insecure: z.ZodDefault<
    z.ZodArray<z.ZodTuple<[z.ZodString, z.ZodInt, typeof GatewayId], null>>
  >
  /**
   * Persisted consent; ordinary address changes must not grant insecure access.
   * Address removal retains consent; explicit disabling or gateway removal revokes it.
   */
  insecureEnabled: z.ZodDefault<
    z.ZodArray<z.ZodTuple<[z.ZodString, z.ZodInt, typeof GatewayId], null>>
  >
  /**
   * COMPUTED: NetServiceData::update — all possible addresses for this binding
   */
  available: z.ZodArray<typeof HostnameInfo>
  /**
   * Effective address selections shared by network enforcement and clients.
   */
  effective: z.ZodDefault<z.ZodArray<typeof HostnameInfo>>
}> = /* @__PURE__ */ object({
  /**
   * User override: enable these addresses (only for public IP & port)
   * An opt-in names one literal address and lapses when the gateway renumbers.
   * Only an enabled plain public domain carries its group's public IPs across.
   */
  enabled: z.array(z.string()).meta({
    description:
      "User override: enable these addresses (only for public IP & port)\nAn opt-in names one literal address and lapses when the gateway renumbers.\nOnly an enabled plain public domain carries its group's public IPs across.",
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
  guaWan: z
    .array(z.string())
    .default(() => [])
    .meta({
      description:
        'User override: IPv6 global-unicast addresses opted into WAN exposure.\nProjected into `HostnameInfo.public` by `update_addresses`, so `public`\nstays the single source of truth for WAN reachability (P2P address\nselection, upstream pinholes). A GUA not in this set is LAN-only.',
    }),
  /**
   * User override: enable these private IPs regardless of their mDNS address.
   */
  lanEnabled: z
    .array(z.tuple([z.string(), z.int()]))
    .default(() => [])
    .meta({
      description:
        'User override: enable these private IPs regardless of their mDNS address.',
    }),
  /**
   * Plaintext private addresses requiring consent, keyed by hostname, port and gateway.
   */
  insecure: z
    .array(z.tuple([z.string(), z.int(), GatewayId]))
    .default(() => [])
    .meta({
      description:
        'Plaintext private addresses requiring consent, keyed by hostname, port and gateway.',
    }),
  /**
   * Persisted consent; ordinary address changes must not grant insecure access.
   * Address removal retains consent; explicit disabling or gateway removal revokes it.
   */
  insecureEnabled: z
    .array(z.tuple([z.string(), z.int(), GatewayId]))
    .default(() => [])
    .meta({
      description:
        'Persisted consent; ordinary address changes must not grant insecure access.\nAddress removal retains consent; explicit disabling or gateway removal revokes it.',
    }),
  /**
   * COMPUTED: NetServiceData::update — all possible addresses for this binding
   */
  available: z.array(HostnameInfo).meta({
    description:
      'COMPUTED: NetServiceData::update — all possible addresses for this binding',
  }),
  /**
   * Effective address selections shared by network enforcement and clients.
   */
  effective: z
    .array(HostnameInfo)
    .default(() => [])
    .meta({
      description:
        'Effective address selections shared by network enforcement and clients.',
    }),
}).meta({
  id: 'DerivedAddressInfo',
})
export const DiskInfo: z.ZodObject<{
  logicalname: z.ZodString
  stablePath: z.ZodString
  partitionTable: z.ZodDefault<z.ZodNullable<typeof PartitionTable>>
  vendor: z.ZodDefault<z.ZodNullable<z.ZodString>>
  model: z.ZodDefault<z.ZodNullable<z.ZodString>>
  partitions: z.ZodArray<typeof PartitionInfo>
  capacity: z.ZodInt
  guid: z.ZodDefault<z.ZodNullable<z.ZodString>>
  filesystem: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  logicalname: z.string(),
  stablePath: z.string(),
  partitionTable: PartitionTable.nullable().default(() => null),
  vendor: z
    .string()
    .nullable()
    .default(() => null),
  model: z
    .string()
    .nullable()
    .default(() => null),
  partitions: z.array(PartitionInfo),
  capacity: z.int(),
  guid: z
    .string()
    .nullable()
    .default(() => null),
  filesystem: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'DiskInfo',
})
export const ImageConfig: z.ZodCodec<
  typeof ImageConfigRepr,
  z.ZodObject<{
    source: typeof ImageSource
    arch: z.ZodArray<z.ZodString>
    emulateMissing: z.ZodBoolean
    nvidiaContainer: z.ZodBoolean
  }>
> = /* @__PURE__ */ wire(
  ImageConfigRepr,
  object({
    source: ImageSource,
    arch: z.array(z.string()),
    emulateMissing: z.boolean(),
    nvidiaContainer: z.boolean(),
  }),
).meta({
  id: 'ImageConfig',
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
export const NetworkGatewayInfo: z.ZodObject<{
  id: typeof GatewayId
  info: typeof NetworkInterfaceInfo
}> = /* @__PURE__ */ object({ id: GatewayId, info: NetworkInterfaceInfo }).meta(
  {
    id: 'NetworkGatewayInfo',
  },
)
export const OsIndex: z.ZodObject<{
  versions: typeof OsVersionInfoMap
  vulnerabilities: z.ZodOptional<
    z.ZodRecord<z.ZodString, typeof SignedVulnerability>
  >
}> = /* @__PURE__ */ object({
  versions: OsVersionInfoMap,
  vulnerabilities: z.record(z.string(), SignedVulnerability).optional(),
}).meta({
  id: 'OsIndex',
})

/**
 * Compatible releases from the selected StartOS update source.
 */
export const OsUpdateCache: z.ZodObject<{
  registry: z.ZodString
  versions: z.ZodRecord<z.ZodString, typeof OsVersionInfo>
  /**
   * Tracks the source registry's cached `RegistryInfo::default_locale`.
   */
  defaultLocale: z.ZodDefault<z.ZodNullable<z.ZodString>>
  status: z.ZodCodec<
    z.ZodOptional<typeof RegistryStatus>,
    typeof RegistryStatus
  >
  /**
   * Failure of the latest synchronization attempt; absent while connecting or connected.
   */
  error: z.ZodOptional<z.ZodNullable<z.ZodString>>
  /**
   * StartOS vulnerabilities the source publishes, signed or not by a trusted key.
   */
  vulnerabilities: z.ZodDefault<
    z.ZodRecord<z.ZodString, typeof SignedVulnerability>
  >
  /**
   * Entries of `vulnerabilities` that a key trusted for StartOS updates signed.
   */
  trustedVulnerabilities: z.ZodDefault<z.ZodArray<z.ZodString>>
}> = /* @__PURE__ */ object({
  registry: z.string(),
  versions: z.record(z.string(), OsVersionInfo),
  /**
   * Tracks the source registry's cached `RegistryInfo::default_locale`.
   */
  defaultLocale: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description:
        "Tracks the source registry's cached `RegistryInfo::default_locale`.",
    }),
  status: wire(RegistryStatus.optional(), RegistryStatus),
  /**
   * Failure of the latest synchronization attempt; absent while connecting or connected.
   */
  error: z.string().nullable().optional().meta({
    description:
      'Failure of the latest synchronization attempt; absent while connecting or connected.',
  }),
  /**
   * StartOS vulnerabilities the source publishes, signed or not by a trusted key.
   */
  vulnerabilities: z
    .record(z.string(), SignedVulnerability)
    .default(() => ({}))
    .meta({
      description:
        'StartOS vulnerabilities the source publishes, signed or not by a trusted key.',
    }),
  /**
   * Entries of `vulnerabilities` that a key trusted for StartOS updates signed.
   */
  trustedVulnerabilities: z
    .array(z.string())
    .default(() => [])
    .meta({
      description:
        'Entries of `vulnerabilities` that a key trusted for StartOS updates signed.',
    }),
}).meta({
  id: 'OsUpdateCache',
  description: 'Compatible releases from the selected StartOS update source.',
})
export const PackageVersionInfo: z.ZodIntersection<
  z.ZodObject<{
    icon: z.ZodOptional<z.ZodNullable<typeof PackageIcon>>
    dependencyMetadata: z.ZodRecord<z.ZodString, typeof DependencyMetadata>
    sourceVersion: z.ZodDefault<z.ZodNullable<z.ZodString>>
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
      sourceVersion: z
        .string()
        .nullable()
        .default(() => null),
      s9pks: z.array(
        z.tuple([HardwareRequirements, RegistryAsset(MerkleArchiveCommitment)]),
      ),
    }),
    PackageMetadata,
  )
  .meta({
    id: 'PackageVersionInfo',
  })

/**
 * A saved registry's synchronization state and what this server can run from its index.
 */
export const RegistryCache: z.ZodObject<{
  info: typeof RegistryInfo
  /**
   * The versions of each package this server can run, in `registryIndexes`.
   */
  selections: z.ZodDefault<z.ZodRecord<z.ZodString, typeof PackageSelection>>
  /**
   * Whether the registry offers StartOS releases for this server's platform.
   */
  publishesOsUpdates: z.ZodDefault<z.ZodBoolean>
  status: z.ZodCodec<
    z.ZodOptional<typeof RegistryStatus>,
    typeof RegistryStatus
  >
  /**
   * Failure of the latest synchronization attempt; absent while connecting or connected.
   */
  error: z.ZodOptional<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  info: RegistryInfo,
  /**
   * The versions of each package this server can run, in `registryIndexes`.
   */
  selections: z
    .record(z.string(), PackageSelection)
    .default(() => ({}))
    .meta({
      description:
        'The versions of each package this server can run, in `registryIndexes`.',
    }),
  /**
   * Whether the registry offers StartOS releases for this server's platform.
   */
  publishesOsUpdates: z
    .boolean()
    .default(() => false)
    .meta({
      description:
        "Whether the registry offers StartOS releases for this server's platform.",
    }),
  status: wire(RegistryStatus.optional(), RegistryStatus),
  /**
   * Failure of the latest synchronization attempt; absent while connecting or connected.
   */
  error: z.string().nullable().optional().meta({
    description:
      'Failure of the latest synchronization attempt; absent while connecting or connected.',
  }),
}).meta({
  id: 'RegistryCache',
  description:
    "A saved registry's synchronization state and what this server can run from its index.",
})
export const ServerStatus: z.ZodObject<{
  backupProgress: z.ZodDefault<z.ZodNullable<typeof FullProgress>>
  backupConversions: z.ZodDefault<
    z.ZodRecord<z.ZodString, z.ZodRecord<z.ZodString, typeof ConversionJob>>
  >
  updateProgress: z.ZodDefault<z.ZodNullable<typeof FullProgress>>
  shuttingDown: z.ZodDefault<z.ZodBoolean>
  restarting: z.ZodDefault<z.ZodBoolean>
  restart: z.ZodDefault<z.ZodNullable<typeof RestartReason>>
}> = /* @__PURE__ */ object({
  backupProgress: FullProgress.nullable().default(() => null),
  backupConversions: z
    .record(z.string(), z.record(z.string(), ConversionJob))
    .default(() => ({})),
  updateProgress: FullProgress.nullable().default(() => null),
  shuttingDown: z.boolean().default(() => false),
  restarting: z.boolean().default(() => false),
  restart: RestartReason.nullable().default(() => null),
}).meta({
  id: 'ServerStatus',
})
export const SetBackupProgress: z.ZodObject<{ progress: typeof Progress }> =
  /* @__PURE__ */ object({ progress: Progress }).meta({
    id: 'SetBackupProgress',
  })
export const SetHealth: z.ZodIntersection<
  z.ZodObject<{ id: typeof HealthCheckId }>,
  typeof NamedHealthCheckResult
> = /* @__PURE__ */ z
  .intersection(object({ id: HealthCheckId }), NamedHealthCheckResult)
  .meta({
    id: 'SetHealth',
  })
export const SetInitProgress: z.ZodObject<{ progress: typeof Progress }> =
  /* @__PURE__ */ object({ progress: Progress }).meta({
    id: 'SetInitProgress',
  })

/**
 * Parameters for replacing the stored NUT configuration.
 */
export const SetNutParams: z.ZodObject<{ config: typeof NutConfig }> =
  /* @__PURE__ */ object({ config: NutConfig }).meta({
    id: 'SetNutParams',
    description: 'Parameters for replacing the stored NUT configuration.',
  })
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
export const StatusInfo: z.ZodObject<{
  health: z.ZodRecord<z.ZodString, typeof NamedHealthCheckResult>
  error: z.ZodDefault<z.ZodNullable<typeof ErrorData>>
  started: z.ZodDefault<z.ZodNullable<z.ZodString>>
  desired: typeof DesiredStatus
}> = /* @__PURE__ */ object({
  health: z.record(z.string(), NamedHealthCheckResult),
  error: ErrorData.nullable().default(() => null),
  started: z
    .string()
    .nullable()
    .default(() => null),
  desired: DesiredStatus,
}).meta({
  id: 'StatusInfo',
})
export const TaskEntry: z.ZodObject<{
  task: typeof Task
  active: z.ZodBoolean
}> = /* @__PURE__ */ object({ task: Task, active: z.boolean() }).meta({
  id: 'TaskEntry',
})
export const CheckDependenciesResult: z.ZodObject<{
  packageId: typeof PackageId
  title: z.ZodDefault<z.ZodNullable<z.ZodString>>
  installedVersion: z.ZodDefault<z.ZodNullable<typeof Version>>
  satisfies: z.ZodArray<typeof Version>
  isRunning: z.ZodBoolean
  tasks: z.ZodRecord<z.ZodString, typeof TaskEntry>
  healthChecks: z.ZodRecord<z.ZodString, typeof NamedHealthCheckResult>
}> = /* @__PURE__ */ object({
  packageId: PackageId,
  title: z
    .string()
    .nullable()
    .default(() => null),
  installedVersion: Version.nullable().default(() => null),
  satisfies: z.array(Version),
  isRunning: z.boolean(),
  tasks: z.record(z.string(), TaskEntry),
  healthChecks: z.record(z.string(), NamedHealthCheckResult),
}).meta({
  id: 'CheckDependenciesResult',
})
export const Dependencies: z.ZodRecord<z.ZodString, typeof DepInfo> =
  /* @__PURE__ */ z.record(z.string(), DepInfo).meta({
    id: 'Dependencies',
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
  gateway: z.ZodDefault<z.ZodNullable<typeof NetworkGatewayInfo>>
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
  gateway: NetworkGatewayInfo.nullable()
    .default(() => null)
    .meta({
      description: 'Present only before stored network policy takes authority.',
    }),
}).meta({
  id: 'DnsStatus',
  description: 'Active DNS authority and its bootstrap gateway.',
})
export const GetPackageResponse: z.ZodObject<{
  categories: z.ZodArray<z.ZodString>
  /**
   * When this registry first published any version it still carries.
   */
  firstPublishedAt: z.ZodOptional<z.ZodNullable<z.ZodString>>
  best: z.ZodRecord<z.ZodString, typeof PackageVersionInfo>
  otherVersions: z.ZodOptional<
    z.ZodNullable<z.ZodRecord<z.ZodString, typeof PackageInfoShort>>
  >
}> = /* @__PURE__ */ object({
  categories: z.array(z.string()),
  /**
   * When this registry first published any version it still carries.
   */
  firstPublishedAt: z.string().nullable().optional().meta({
    description:
      'When this registry first published any version it still carries.',
  }),
  best: z.record(z.string(), PackageVersionInfo),
  otherVersions: z.record(z.string(), PackageInfoShort).nullable().optional(),
}).meta({
  id: 'GetPackageResponse',
})
export const GetPackageResponseFull: z.ZodObject<{
  categories: z.ZodArray<z.ZodString>
  /**
   * When this registry first published any version it still carries.
   */
  firstPublishedAt: z.ZodOptional<z.ZodNullable<z.ZodString>>
  best: z.ZodRecord<z.ZodString, typeof PackageVersionInfo>
  otherVersions: z.ZodRecord<z.ZodString, typeof PackageVersionInfo>
}> = /* @__PURE__ */ object({
  categories: z.array(z.string()),
  /**
   * When this registry first published any version it still carries.
   */
  firstPublishedAt: z.string().nullable().optional().meta({
    description:
      'When this registry first published any version it still carries.',
  }),
  best: z.record(z.string(), PackageVersionInfo),
  otherVersions: z.record(z.string(), PackageVersionInfo),
}).meta({
  id: 'GetPackageResponseFull',
})
export const LegacyProxySslOptions: z.ZodIntersection<
  z.ZodObject<{
    addXForwardedHeaders: z.ZodDefault<z.ZodBoolean>
    auth: z.ZodDefault<z.ZodNullable<typeof ProxyAuth>>
  }>,
  typeof AddSslOptions
> = /* @__PURE__ */ z
  .intersection(
    object({
      addXForwardedHeaders: z.boolean().default(() => false),
      auth: ProxyAuth.nullable().default(() => null),
    }),
    AddSslOptions,
  )
  .meta({
    id: 'LegacyProxySslOptions',
  })
export const MetricsFollowResponse: z.ZodObject<{
  /**
   * Present for ordinary replies; typed observations use same-ID events.
   */
  guid: z.ZodDefault<z.ZodNullable<typeof Guid>>
  metrics: typeof Metrics
}> = /* @__PURE__ */ object({
  /**
   * Present for ordinary replies; typed observations use same-ID events.
   */
  guid: Guid.nullable()
    .default(() => null)
    .meta({
      description:
        'Present for ordinary replies; typed observations use same-ID events.',
    }),
  metrics: Metrics,
}).meta({
  id: 'MetricsFollowResponse',
})
export const PackageInfo: z.ZodObject<{
  authorized: z.ZodRecord<z.ZodString, z.ZodString>
  versions: z.ZodRecord<z.ZodString, typeof PackageVersionInfo>
  categories: z.ZodArray<z.ZodString>
  vulnerabilities: z.ZodOptional<
    z.ZodRecord<z.ZodString, typeof SignedVulnerability>
  >
}> = /* @__PURE__ */ object({
  authorized: z.record(z.string(), z.string()),
  versions: z.record(z.string(), PackageVersionInfo),
  categories: z.array(z.string()),
  vulnerabilities: z.record(z.string(), SignedVulnerability).optional(),
}).meta({
  id: 'PackageInfo',
})

/**
 * A package version as the requester sees it, for promotion to another registry.
 */
export const PackageVersionExport: z.ZodObject<{
  /**
   * The s9pks the requester can see, with their keystreams. Reader ids appear for admins and
   * the package's publishers.
   */
  info: typeof PackageVersionInfo
  /**
   * The reader directory entries the s9pks list. Null for requesters who cannot see reader
   * lists.
   */
  readers: z.ZodDefault<
    z.ZodNullable<z.ZodRecord<z.ZodString, typeof ReaderKey>>
  >
}> = /* @__PURE__ */ object({
  /**
   * The s9pks the requester can see, with their keystreams. Reader ids appear for admins and
   * the package's publishers.
   */
  info: PackageVersionInfo.meta({
    description:
      "The s9pks the requester can see, with their keystreams. Reader ids appear for admins and\nthe package's publishers.",
  }),
  /**
   * The reader directory entries the s9pks list. Null for requesters who cannot see reader
   * lists.
   */
  readers: z
    .record(z.string(), ReaderKey)
    .nullable()
    .default(() => null)
    .meta({
      description:
        'The reader directory entries the s9pks list. Null for requesters who cannot see reader\nlists.',
    }),
}).meta({
  id: 'PackageVersionExport',
  description:
    'A package version as the requester sees it, for promotion to another registry.',
})

/**
 * A contiguous binding keyed by its internal start port in [`BindingRanges`].
 * External ports map to internal ports by offset.
 */
export const RangeBindInfo: z.ZodObject<{
  enabled: z.ZodBoolean
  transport: z.ZodCodec<z.ZodOptional<typeof Transport>, typeof Transport>
  externalStartPort: z.ZodInt
  numberOfPorts: z.ZodInt
  /**
   * Reachable addresses for this range (LAN IPv4 / WAN IPv4 / mDNS /
   * domains) with per-address enabled/disabled overrides — the same model as
   * a single-port binding, but IPv4-only and non-SSL. COMPUTED by
   * `update_addresses`; every entry uses `external_start_port` as its
   * representative port. Public IPs are disabled by default (WAN is opt-in);
   * LAN/mDNS/domains are enabled by default.
   */
  addresses: z.ZodCodec<
    z.ZodOptional<typeof DerivedAddressInfo>,
    typeof DerivedAddressInfo
  >
  /**
   * The single restricted `api` interface exported from this range, if any
   * (`RangeOrigin.export`). Preserved across idempotent re-binds.
   */
  interface: z.ZodDefault<z.ZodNullable<typeof RangeServiceInterface>>
}> = /* @__PURE__ */ object({
  enabled: z.boolean(),
  transport: wire(Transport.optional(), Transport),
  externalStartPort: z.int(),
  numberOfPorts: z.int(),
  /**
   * Reachable addresses for this range (LAN IPv4 / WAN IPv4 / mDNS /
   * domains) with per-address enabled/disabled overrides — the same model as
   * a single-port binding, but IPv4-only and non-SSL. COMPUTED by
   * `update_addresses`; every entry uses `external_start_port` as its
   * representative port. Public IPs are disabled by default (WAN is opt-in);
   * LAN/mDNS/domains are enabled by default.
   */
  addresses: wire(DerivedAddressInfo.optional(), DerivedAddressInfo).meta({
    description:
      'Reachable addresses for this range (LAN IPv4 / WAN IPv4 / mDNS /\ndomains) with per-address enabled/disabled overrides — the same model as\na single-port binding, but IPv4-only and non-SSL. COMPUTED by\n`update_addresses`; every entry uses `external_start_port` as its\nrepresentative port. Public IPs are disabled by default (WAN is opt-in);\nLAN/mDNS/domains are enabled by default.',
  }),
  /**
   * The single restricted `api` interface exported from this range, if any
   * (`RangeOrigin.export`). Preserved across idempotent re-binds.
   */
  interface: RangeServiceInterface.nullable()
    .default(() => null)
    .meta({
      description:
        'The single restricted `api` interface exported from this range, if any\n(`RangeOrigin.export`). Preserved across idempotent re-binds.',
    }),
}).meta({
  id: 'RangeBindInfo',
  description:
    'A contiguous binding keyed by its internal start port in [`BindingRanges`].\nExternal ports map to internal ports by offset.',
})
export const BindingRanges: z.ZodRecord<z.ZodString, typeof RangeBindInfo> =
  /* @__PURE__ */ z.record(z.string(), RangeBindInfo).meta({
    id: 'BindingRanges',
  })
export const CompatibleBindOptions: z.ZodObject<{
  transport: z.ZodCodec<z.ZodOptional<typeof Transport>, typeof Transport>
  preferredExternalPort: z.ZodInt
  addSsl: z.ZodDefault<z.ZodNullable<typeof LegacyProxySslOptions>>
  secure: z.ZodDefault<z.ZodNullable<typeof Security>>
  proxy: z.ZodDefault<z.ZodNullable<typeof ProxyOptions>>
}> = /* @__PURE__ */ object({
  transport: wire(Transport.optional(), Transport),
  preferredExternalPort: z.int(),
  addSsl: LegacyProxySslOptions.nullable().default(() => null),
  secure: Security.nullable().default(() => null),
  proxy: ProxyOptions.nullable().default(() => null),
}).meta({
  id: 'CompatibleBindOptions',
})
export const Manifest: z.ZodIntersection<
  z.ZodObject<{
    id: typeof PackageId
    version: typeof Version
    canMigrateTo: z.ZodString
    canMigrateFrom: z.ZodString
    images: z.ZodRecord<z.ZodString, typeof ImageConfig>
    volumes: z.ZodArray<typeof VolumeId>
    /**
     * Enables user-managed grants to shared volumes; grants default to none.
     */
    sharedVolumes: z.ZodDefault<z.ZodBoolean>
    dependencies: z.ZodCodec<
      z.ZodOptional<typeof Dependencies>,
      typeof Dependencies
    >
    hardwareRequirements: z.ZodCodec<
      z.ZodOptional<typeof HardwareRequirements>,
      typeof HardwareRequirements
    >
  }>,
  typeof PackageMetadata
> = /* @__PURE__ */ z
  .intersection(
    object({
      id: PackageId,
      version: Version,
      canMigrateTo: z.string(),
      canMigrateFrom: z.string(),
      images: z.record(z.string(), ImageConfig),
      volumes: z.array(VolumeId),
      /**
       * Enables user-managed grants to shared volumes; grants default to none.
       */
      sharedVolumes: z
        .boolean()
        .default(() => false)
        .meta({
          description:
            'Enables user-managed grants to shared volumes; grants default to none.',
        }),
      dependencies: wire(Dependencies.optional(), Dependencies),
      hardwareRequirements: wire(
        HardwareRequirements.optional(),
        HardwareRequirements,
      ),
    }),
    PackageMetadata,
  )
  .meta({
    id: 'Manifest',
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
    /**
     * Selects direct-forward transports; HTTP proxies and TLS listeners use TCP.
     */
    transport: typeof Transport
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
    /**
     * Selects direct-forward transports; HTTP proxies and TLS listeners use TCP.
     */
    transport: Transport.meta({
      description:
        'Selects direct-forward transports; HTTP proxies and TLS listeners use TCP.',
    }),
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
  name: z.ZodDefault<z.ZodNullable<z.ZodString>>
  icon: z.ZodDefault<z.ZodNullable<typeof DataUrl>>
  description: z.ZodDefault<z.ZodNullable<typeof LocaleString>>
  /**
   * Locale registry-owned `LocaleString`s fall back to when a requested one is missing.
   */
  defaultLocale: z.ZodDefault<z.ZodNullable<z.ZodString>>
  package: typeof PackageIndex
  os: typeof OsIndex
  signers: z.ZodRecord<z.ZodString, typeof SignerInfo>
  trustSigners: z.ZodDefault<z.ZodArray<typeof Guid>>
  /**
   * Keys allowed to read the private s9pks that list them. Admin-only.
   */
  readers: z.ZodDefault<z.ZodRecord<z.ZodString, typeof ReaderKey>>
}> = /* @__PURE__ */ object({
  name: z
    .string()
    .nullable()
    .default(() => null),
  icon: DataUrl.nullable().default(() => null),
  description: LocaleString.nullable().default(() => null),
  /**
   * Locale registry-owned `LocaleString`s fall back to when a requested one is missing.
   */
  defaultLocale: z
    .string()
    .nullable()
    .default(() => null)
    .meta({
      description:
        'Locale registry-owned `LocaleString`s fall back to when a requested one is missing.',
    }),
  package: PackageIndex,
  os: OsIndex,
  signers: z.record(z.string(), SignerInfo),
  trustSigners: z.array(Guid).default(() => []),
  /**
   * Keys allowed to read the private s9pks that list them. Admin-only.
   */
  readers: z
    .record(z.string(), ReaderKey)
    .default(() => ({}))
    .meta({
      description:
        'Keys allowed to read the private s9pks that list them. Admin-only.',
    }),
}).meta({
  id: 'FullIndex',
})
export const InstalledState: z.ZodObject<{ manifest: typeof Manifest }> =
  /* @__PURE__ */ object({ manifest: Manifest }).meta({
    id: 'InstalledState',
  })
export const InstallingInfo: z.ZodObject<{
  newManifest: typeof Manifest
  progress: typeof FullProgress
}> = /* @__PURE__ */ object({
  newManifest: Manifest,
  progress: FullProgress,
}).meta({
  id: 'InstallingInfo',
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
  interfaces: z.ZodDefault<z.ZodRecord<z.ZodString, typeof ServiceInterface>>
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
  interfaces: z
    .record(z.string(), ServiceInterface)
    .default(() => ({}))
    .meta({
      description:
        'Service interfaces exported from this binding (`Origin.export`). A single\nbinding (host + internal port) may back several interfaces (e.g. a `ui`\nand an `api` on the same port), so this is keyed by interface id.',
    }),
}).meta({
  id: 'BindInfo',
})
export const BindParams: z.ZodIntersection<
  z.ZodObject<{ id: typeof HostId; internalPort: z.ZodInt }>,
  typeof BindOptions
> = /* @__PURE__ */ z
  .intersection(object({ id: HostId, internalPort: z.int() }), BindOptions)
  .meta({
    id: 'BindParams',
  })
export const InstallingState: z.ZodObject<{
  installingInfo: typeof InstallingInfo
}> = /* @__PURE__ */ object({ installingInfo: InstallingInfo }).meta({
  id: 'InstallingState',
})
export const UpdatingState: z.ZodObject<{
  manifest: typeof Manifest
  s9pk: z.ZodString
  installingInfo: typeof InstallingInfo
}> = /* @__PURE__ */ object({
  manifest: Manifest,
  s9pk: z.string(),
  installingInfo: InstallingInfo,
}).meta({
  id: 'UpdatingState',
})
export const Bindings: z.ZodRecord<z.ZodString, typeof BindInfo> =
  /* @__PURE__ */ z.record(z.string(), BindInfo).meta({
    id: 'Bindings',
  })
export const PackageState: z.ZodUnion<
  readonly [
    z.ZodIntersection<
      z.ZodObject<{ state: z.ZodLiteral<'installing'> }>,
      typeof InstallingState
    >,
    z.ZodIntersection<
      z.ZodObject<{ state: z.ZodLiteral<'restoring'> }>,
      typeof InstallingState
    >,
    z.ZodIntersection<
      z.ZodObject<{ state: z.ZodLiteral<'updating'> }>,
      typeof UpdatingState
    >,
    z.ZodIntersection<
      z.ZodObject<{ state: z.ZodLiteral<'installed'> }>,
      typeof InstalledState
    >,
    z.ZodIntersection<
      z.ZodObject<{ state: z.ZodLiteral<'removing'> }>,
      typeof InstalledState
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(object({ state: z.literal('installing') }), InstallingState),
    z.intersection(object({ state: z.literal('restoring') }), InstallingState),
    z.intersection(object({ state: z.literal('updating') }), UpdatingState),
    z.intersection(object({ state: z.literal('installed') }), InstalledState),
    z.intersection(object({ state: z.literal('removing') }), InstalledState),
  ])
  .meta({
    id: 'PackageState',
  })
export const Host: z.ZodObject<{
  bindings: typeof Bindings
  bindingRanges: z.ZodCodec<
    z.ZodOptional<typeof BindingRanges>,
    typeof BindingRanges
  >
  publicDomains: z.ZodRecord<z.ZodString, typeof PublicDomainConfig>
  privateDomains: z.ZodRecord<z.ZodString, z.ZodArray<typeof GatewayId>>
  /**
   * COMPUTED: port forwarding rules needed on gateways for public addresses to work.
   */
  portForwards: z.ZodDefault<z.ZodArray<typeof PortForward>>
}> = /* @__PURE__ */ object({
  bindings: Bindings,
  bindingRanges: wire(BindingRanges.optional(), BindingRanges),
  publicDomains: z.record(z.string(), PublicDomainConfig),
  privateDomains: z.record(z.string(), z.array(GatewayId)),
  /**
   * COMPUTED: port forwarding rules needed on gateways for public addresses to work.
   */
  portForwards: z
    .array(PortForward)
    .default(() => [])
    .meta({
      description:
        'COMPUTED: port forwarding rules needed on gateways for public addresses to work.',
    }),
}).meta({
  id: 'Host',
})
export const Hosts: z.ZodRecord<z.ZodString, typeof Host> = /* @__PURE__ */ z
  .record(z.string(), Host)
  .meta({
    id: 'Hosts',
  })
export const NetworkInfo: z.ZodObject<{
  wifi: typeof WifiInfo
  host: typeof Host
  gateways: z.ZodDefault<z.ZodRecord<z.ZodString, typeof NetworkInterfaceInfo>>
  acme: z.ZodDefault<z.ZodRecord<z.ZodString, typeof AcmeSettings>>
  defaultOutbound: z.ZodDefault<z.ZodNullable<typeof GatewayId>>
  outboundDns: z.ZodDefault<z.ZodNullable<z.ZodArray<z.ZodString>>>
  passthroughs: z.ZodDefault<z.ZodArray<typeof PassthroughInfo>>
}> = /* @__PURE__ */ object({
  wifi: WifiInfo,
  host: Host,
  gateways: z.record(z.string(), NetworkInterfaceInfo).default(() => ({})),
  acme: z.record(z.string(), AcmeSettings).default(() => ({})),
  defaultOutbound: GatewayId.nullable().default(() => null),
  outboundDns: z
    .array(z.string())
    .nullable()
    .default(() => null),
  passthroughs: z.array(PassthroughInfo).default(() => []),
}).meta({
  id: 'NetworkInfo',
})
export const PackageDataEntry: z.ZodObject<{
  stateInfo: typeof PackageState
  s9pk: z.ZodString
  statusInfo: typeof StatusInfo
  registry: z.ZodDefault<z.ZodNullable<z.ZodString>>
  developerKey: typeof literal_0
  lastBackup: z.ZodDefault<z.ZodNullable<z.ZodString>>
  currentDependencies: typeof CurrentDependencies
  actions: z.ZodRecord<z.ZodString, typeof ActionMetadata>
  tasks: z.ZodRecord<z.ZodString, typeof TaskEntry>
  hosts: typeof Hosts
  storeExposedDependents: z.ZodArray<z.ZodString>
  outboundGateway: z.ZodDefault<z.ZodNullable<typeof GatewayId>>
  outboundDns: z.ZodDefault<z.ZodNullable<z.ZodArray<z.ZodString>>>
  plugin: z.ZodCodec<z.ZodOptional<typeof PackagePlugin>, typeof PackagePlugin>
}> = /* @__PURE__ */ object({
  stateInfo: PackageState,
  s9pk: z.string(),
  statusInfo: StatusInfo,
  registry: z
    .string()
    .nullable()
    .default(() => null),
  developerKey: literal_0.meta({
    description:
      'Standard Base64 DER with padding. Deserialization also accepts legacy PEM armor.',
  }),
  lastBackup: z
    .string()
    .nullable()
    .default(() => null),
  currentDependencies: CurrentDependencies,
  actions: z.record(z.string(), ActionMetadata),
  tasks: z.record(z.string(), TaskEntry),
  hosts: Hosts,
  storeExposedDependents: z.array(z.string()),
  outboundGateway: GatewayId.nullable().default(() => null),
  outboundDns: z
    .array(z.string())
    .nullable()
    .default(() => null),
  plugin: wire(PackagePlugin.optional(), PackagePlugin),
}).meta({
  id: 'PackageDataEntry',
})
export const ServerInfo: z.ZodObject<{
  id: z.ZodString
  hostname: z.ZodString
  version: z.ZodString
  /**
   * Commit of the running StartOS build, rewritten on every boot.
   */
  gitHash: z.ZodCodec<z.ZodOptional<z.ZodString>, z.ZodString>
  packageVersionCompat: z.ZodString
  postInitMigrationTodos: typeof literal_1
  latestMigrationRevision: z.ZodDefault<z.ZodInt>
  lastBackup: z.ZodDefault<z.ZodNullable<z.ZodString>>
  network: typeof NetworkInfo
  statusInfo: z.ZodCodec<
    z.ZodOptional<typeof ServerStatus>,
    typeof ServerStatus
  >
  unreadNotificationCount: z.ZodInt
  trustedKeys: z.ZodCodec<
    z.ZodOptional<z.ZodRecord<z.ZodString, typeof TrustedKey>>,
    z.ZodRecord<z.ZodString, typeof TrustedKey>
  >
  /**
   * Last observed trust proposals from connected registries.
   */
  registryKeys: z.ZodDefault<
    z.ZodRecord<z.ZodString, z.ZodArray<typeof AnyVerifyingKey>>
  >
  /**
   * Connected registries; written only through `server.registries`.
   */
  registries: z.ZodDefault<z.ZodRecord<z.ZodString, z.ZodNullable<z.ZodString>>>
  /**
   * Registry that provides StartOS updates; always connected.
   */
  osRegistry: z.ZodCodec<z.ZodOptional<z.ZodString>, z.ZodString>
  pubkey: z.ZodString
  caFingerprint: z.ZodString
  ntpSynced: z.ZodDefault<z.ZodBoolean>
  zram: z.ZodDefault<z.ZodBoolean>
  governor: z.ZodDefault<z.ZodNullable<typeof Governor>>
  epp: z.ZodDefault<z.ZodNullable<typeof Epp>>
  smtp: z.ZodDefault<z.ZodNullable<typeof SmtpValue>>
  nut: z.ZodCodec<z.ZodOptional<typeof NutConfig>, typeof NutConfig>
  echoipUrls: z.ZodCodec<
    z.ZodOptional<z.ZodArray<z.ZodString>>,
    z.ZodArray<z.ZodString>
  >
  ram: z.ZodInt
  devices: z.ZodArray<typeof LshwDevice>
  kiosk: z.ZodDefault<z.ZodNullable<z.ZodBoolean>>
  language: z.ZodDefault<z.ZodNullable<z.ZodString>>
  keyboard: z.ZodDefault<z.ZodNullable<typeof KeyboardOptions>>
}> = /* @__PURE__ */ object({
  id: z.string(),
  hostname: z.string(),
  version: z.string(),
  /**
   * Commit of the running StartOS build, rewritten on every boot.
   */
  gitHash: wire(z.string().optional(), z.string()).meta({
    description:
      'Commit of the running StartOS build, rewritten on every boot.',
  }),
  packageVersionCompat: z.string(),
  postInitMigrationTodos: literal_1,
  latestMigrationRevision: z.int().default(() => 0),
  lastBackup: z
    .string()
    .nullable()
    .default(() => null),
  network: NetworkInfo,
  statusInfo: wire(ServerStatus.optional(), ServerStatus),
  unreadNotificationCount: z.int(),
  trustedKeys: wire(
    z.record(z.string(), TrustedKey).optional(),
    z.record(z.string(), TrustedKey),
  ),
  /**
   * Last observed trust proposals from connected registries.
   */
  registryKeys: z
    .record(z.string(), z.array(AnyVerifyingKey))
    .default(() => ({}))
    .meta({
      description: 'Last observed trust proposals from connected registries.',
    }),
  /**
   * Connected registries; written only through `server.registries`.
   */
  registries: z
    .record(z.string(), z.string().nullable())
    .default(() => ({}))
    .meta({
      description:
        'Connected registries; written only through `server.registries`.',
    }),
  /**
   * Registry that provides StartOS updates; always connected.
   */
  osRegistry: wire(z.string().optional(), z.string()).meta({
    description: 'Registry that provides StartOS updates; always connected.',
  }),
  pubkey: z.string(),
  caFingerprint: z.string(),
  ntpSynced: z.boolean().default(() => false),
  zram: z.boolean().default(() => false),
  governor: Governor.nullable().default(() => null),
  epp: Epp.nullable().default(() => null),
  smtp: SmtpValue.nullable().default(() => null),
  nut: wire(NutConfig.optional(), NutConfig),
  echoipUrls: wire(z.array(z.string()).optional(), z.array(z.string())),
  ram: z.int(),
  devices: z.array(LshwDevice),
  kiosk: z
    .boolean()
    .nullable()
    .default(() => null),
  language: z
    .string()
    .nullable()
    .default(() => null),
  keyboard: KeyboardOptions.nullable().default(() => null),
}).meta({
  id: 'ServerInfo',
})
export const AllPackageData: z.ZodRecord<z.ZodString, typeof PackageDataEntry> =
  /* @__PURE__ */ z.record(z.string(), PackageDataEntry).meta({
    id: 'AllPackageData',
  })
export const Public: z.ZodObject<{
  serverInfo: typeof ServerInfo
  packageData: typeof AllPackageData
  sshPubkeys: z.ZodCodec<z.ZodOptional<typeof SshKeys>, typeof SshKeys>
  /**
   * Each saved registry's last synchronized index, in the server's language.
   * Includes authorized private-asset keystreams from the server identity's registry view.
   */
  registryIndexes: z.ZodDefault<z.ZodRecord<z.ZodString, typeof FullIndex>>
  registryCache: z.ZodDefault<z.ZodRecord<z.ZodString, typeof RegistryCache>>
  osUpdateCache: z.ZodDefault<z.ZodNullable<typeof OsUpdateCache>>
  disks: z.ZodDefault<z.ZodArray<typeof DiskInfo>>
  sharedVolumes: z.ZodDefault<z.ZodRecord<z.ZodString, typeof SharedVolume>>
  backupTargets: z.ZodDefault<z.ZodRecord<z.ZodString, typeof BackupTarget>>
  /**
   * UI-owned state; backend domain logic accesses its contents only during migrations.
   */
  ui: z.ZodUnknown
}> = /* @__PURE__ */ object({
  serverInfo: ServerInfo,
  packageData: AllPackageData,
  sshPubkeys: wire(SshKeys.optional(), SshKeys),
  /**
   * Each saved registry's last synchronized index, in the server's language.
   * Includes authorized private-asset keystreams from the server identity's registry view.
   */
  registryIndexes: z
    .record(z.string(), FullIndex)
    .default(() => ({}))
    .meta({
      description:
        "Each saved registry's last synchronized index, in the server's language.\nIncludes authorized private-asset keystreams from the server identity's registry view.",
    }),
  registryCache: z.record(z.string(), RegistryCache).default(() => ({})),
  osUpdateCache: OsUpdateCache.nullable().default(() => null),
  disks: z.array(DiskInfo).default(() => []),
  sharedVolumes: z.record(z.string(), SharedVolume).default(() => ({})),
  backupTargets: z.record(z.string(), BackupTarget).default(() => ({})),
  /**
   * UI-owned state; backend domain logic accesses its contents only during migrations.
   */
  ui: z.unknown().meta({
    description:
      'UI-owned state; backend domain logic accesses its contents only during migrations.',
  }),
}).meta({
  id: 'Public',
})
