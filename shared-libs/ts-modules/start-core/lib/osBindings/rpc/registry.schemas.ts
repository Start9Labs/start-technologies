import { z } from 'zod'
const wire = <A extends z.ZodType, B extends z.ZodType>(input: A, output: B) =>
  z.codec(input, output, { decode: v => v as never, encode: v => v as never })
const object = <Shape extends z.core.$ZodLooseShape>(shape: Shape) =>
  z.object(shape).meta({ additionalProperties: true })
const empty: z.ZodType<{}, {}> = /* @__PURE__ */ object({})
const literal_0 = /* @__PURE__ */ z.string()
const literal_1 = /* @__PURE__ */ z.union([
  z.string(),
  z.record(z.string(), z.string()),
])
const literal_2 = /* @__PURE__ */ z.enum(['processor', 'display'])

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
export const AnySignature: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'AnySignature',
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
export const DataUrl: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'DataUrl',
})
export const DeviceFilter: z.ZodObject<{
  description: z.ZodString
  class: typeof literal_2
  product: z.ZodDefault<z.ZodNullable<z.ZodString>>
  vendor: z.ZodDefault<z.ZodNullable<z.ZodString>>
  capabilities: z.ZodDefault<z.ZodNullable<z.ZodArray<z.ZodString>>>
  driver: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  description: z.string(),
  class: literal_2,
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
export const Dump: z.ZodObject<{ id: z.ZodInt; value: z.ZodUnknown }> =
  /* @__PURE__ */ object({ id: z.int(), value: z.unknown() }).meta({
    id: 'Dump',
  })
export const DumpParams: z.ZodObject<{
  pointer: z.ZodDefault<z.ZodNullable<z.ZodString>>
}> = /* @__PURE__ */ object({
  pointer: z
    .string()
    .nullable()
    .default(() => null),
}).meta({
  id: 'DumpParams',
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
export const GitHash: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'GitHash',
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
export const ListVersionSignersParams: z.ZodObject<{ version: z.ZodString }> =
  /* @__PURE__ */ object({ version: z.string() }).meta({
    id: 'ListVersionSignersParams',
  })
export const LocaleString: typeof literal_1 = /* @__PURE__ */ literal_1.meta({
  id: 'LocaleString',
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
export const PluginId: z.ZodEnum<{ 'url-v0': 'url-v0' }> = /* @__PURE__ */ z
  .enum(['url-v0'])
  .meta({
    id: 'PluginId',
  })
export const PreDownloadAlertWhen: z.ZodObject<{ sourceVersion: z.ZodString }> =
  /* @__PURE__ */ object({ sourceVersion: z.string() }).meta({
    id: 'PreDownloadAlertWhen',
  })
export const ReaderParams: typeof empty = /* @__PURE__ */ empty.meta({
  id: 'ReaderParams',
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
export const SetDefaultLocaleParams: z.ZodObject<{
  defaultLocale: z.ZodNullable<z.ZodString>
}> = /* @__PURE__ */ object({ defaultLocale: z.string().nullable() }).meta({
  id: 'SetDefaultLocaleParams',
})
export const SetNameParams: z.ZodObject<{ name: z.ZodString }> =
  /* @__PURE__ */ object({ name: z.string() }).meta({
    id: 'SetNameParams',
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
export const AddReaderParams: z.ZodObject<{
  label: z.ZodString
  key: typeof AnyVerifyingKey
}> = /* @__PURE__ */ object({ label: z.string(), key: AnyVerifyingKey }).meta({
  id: 'AddReaderParams',
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
  gpgExports: z.ZodDefault<z.ZodArray<typeof GpgPublicKeyExport>>
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
  gpgExports: z
    .array(GpgPublicKeyExport)
    .default(() => [])
    .meta({
      description:
        'Retained certificates; `keys` determines the selected signing keys.',
    }),
}).meta({
  id: 'AddSignerParams',
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
export const ApplyWithPathParams: z.ZodIntersection<
  z.ZodObject<{ path: z.ZodDefault<z.ZodNullable<z.ZodString>> }>,
  typeof ApplyParams
> = /* @__PURE__ */ z
  .intersection(
    object({
      path: z
        .string()
        .nullable()
        .default(() => null),
    }),
    ApplyParams,
  )
  .meta({
    id: 'ApplyWithPathParams',
  })
export const Category: z.ZodObject<{ name: typeof LocaleString }> =
  /* @__PURE__ */ object({ name: LocaleString }).meta({
    id: 'Category',
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
export const Description: z.ZodObject<{
  short: typeof LocaleString
  long: typeof LocaleString
}> = /* @__PURE__ */ object({ short: LocaleString, long: LocaleString }).meta({
  id: 'Description',
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
export const ExportPackageVersionParams: z.ZodIntersection<
  z.ZodObject<{ id: typeof PackageId; version: typeof Version }>,
  typeof ReaderParams
> = /* @__PURE__ */ z
  .intersection(object({ id: PackageId, version: Version }), ReaderParams)
  .meta({
    id: 'ExportPackageVersionParams',
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
export const ListPackageSignersParams: z.ZodIntersection<
  z.ZodObject<{ id: typeof PackageId }>,
  typeof ReaderParams
> = /* @__PURE__ */ z
  .intersection(object({ id: PackageId }), ReaderParams)
  .meta({
    id: 'ListPackageSignersParams',
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
}> = /* @__PURE__ */ object({
  releaseNotes: LocaleString,
  /**
   * `PackageMetadata::default_locale`.
   */
  defaultLocale: z.string().nullable().optional().meta({
    description: '`PackageMetadata::default_locale`.',
  }),
}).meta({
  id: 'PackageInfoShort',
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
 * A key allowed to read the private s9pks that list it.
 */
export const ReaderKey: z.ZodObject<{
  label: z.ZodString
  key: typeof AnyVerifyingKey
}> = /* @__PURE__ */ object({ label: z.string(), key: AnyVerifyingKey }).meta({
  id: 'ReaderKey',
  description: 'A key allowed to read the private s9pks that list it.',
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
export const RemoveReaderParams: z.ZodObject<{ id: typeof Guid }> =
  /* @__PURE__ */ object({ id: Guid }).meta({
    id: 'RemoveReaderParams',
  })
export const RemoveSignerParams: z.ZodObject<{ id: typeof Guid }> =
  /* @__PURE__ */ object({ id: Guid }).meta({
    id: 'RemoveSignerParams',
  })
export const SetDescriptionParams: z.ZodObject<{
  description: z.ZodNullable<typeof LocaleString>
}> = /* @__PURE__ */ object({ description: LocaleString.nullable() }).meta({
  id: 'SetDescriptionParams',
})
export const SetIconParams: z.ZodObject<{
  icon: z.ZodNullable<typeof DataUrl>
}> = /* @__PURE__ */ object({ icon: DataUrl.nullable() }).meta({
  id: 'SetIconParams',
})
export const SetTrustSignersParams: z.ZodObject<{
  signers: z.ZodArray<typeof Guid>
}> = /* @__PURE__ */ object({ signers: z.array(Guid) }).meta({
  id: 'SetTrustSignersParams',
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
export const SubscribeIndexParams: z.ZodIntersection<
  z.ZodObject<{
    pointer: z.ZodDefault<z.ZodNullable<z.ZodString>>
    /**
     * Client StartOS version. Omission preserves the canonical index format.
     */
    osVersion: z.ZodDefault<z.ZodNullable<z.ZodString>>
    /**
     * Locale to resolve localized text to; one the index lacks gets the fallback. Omission keeps every translation.
     */
    language: z.ZodDefault<z.ZodNullable<z.ZodString>>
  }>,
  typeof ReaderParams
> = /* @__PURE__ */ z
  .intersection(
    object({
      pointer: z
        .string()
        .nullable()
        .default(() => null),
      /**
       * Client StartOS version. Omission preserves the canonical index format.
       */
      osVersion: z
        .string()
        .nullable()
        .default(() => null)
        .meta({
          description:
            'Client StartOS version. Omission preserves the canonical index format.',
        }),
      /**
       * Locale to resolve localized text to; one the index lacks gets the fallback. Omission keeps every translation.
       */
      language: z
        .string()
        .nullable()
        .default(() => null)
        .meta({
          description:
            'Locale to resolve localized text to; one the index lacks gets the fallback. Omission keeps every translation.',
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
export const VersionSignerParams: z.ZodObject<{
  version: z.ZodString
  signer: typeof Guid
}> = /* @__PURE__ */ object({ version: z.string(), signer: Guid }).meta({
  id: 'VersionSignerParams',
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

/**
 * Representation of JSON Patch (list of patch operations)
 */
export const Patch: z.ZodArray<typeof PatchOperation> = /* @__PURE__ */ z
  .array(PatchOperation)
  .meta({
    id: 'Patch',
    description: 'Representation of JSON Patch (list of patch operations)',
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
 * Database-generated patches contain only add, remove and replace operations.
 */
export const DiffPatch: typeof Patch = /* @__PURE__ */ Patch.meta({
  id: 'DiffPatch',
  description:
    'Database-generated patches contain only add, remove and replace operations.',
})
export const OsIndex: z.ZodObject<{ versions: typeof OsVersionInfoMap }> =
  /* @__PURE__ */ object({ versions: OsVersionInfoMap }).meta({
    id: 'OsIndex',
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
export const PackageInfo: z.ZodObject<{
  authorized: z.ZodRecord<z.ZodString, z.ZodString>
  versions: z.ZodRecord<z.ZodString, typeof PackageVersionInfo>
  categories: z.ZodArray<z.ZodString>
}> = /* @__PURE__ */ object({
  authorized: z.record(z.string(), z.string()),
  versions: z.record(z.string(), PackageVersionInfo),
  categories: z.array(z.string()),
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
export const Revision: z.ZodObject<{ id: z.ZodInt; patch: typeof DiffPatch }> =
  /* @__PURE__ */ object({ id: z.int(), patch: DiffPatch }).meta({
    id: 'Revision',
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
export const Api = {
  _PARAMS: /* @__PURE__ */ empty,
  _CHILDREN: {
    admin: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        add: {
          _PARAMS: /* @__PURE__ */ AddAdminParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        list: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(
              object({ format: IoFormat.nullable().default(() => null) }),
              empty,
            ),
            empty,
          ),
          _RETURN: /* @__PURE__ */ z.record(z.string(), SignerInfo),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        reader: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            add: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(
                  object({ format: IoFormat.nullable().default(() => null) }),
                  empty,
                ),
                AddReaderParams,
              ),
              _RETURN: /* @__PURE__ */ Guid,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            edit: {
              _PARAMS: /* @__PURE__ */ EditReaderParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            list: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(
                  object({ format: IoFormat.nullable().default(() => null) }),
                  empty,
                ),
                empty,
              ),
              _RETURN: /* @__PURE__ */ z.record(z.string(), ReaderKey),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            remove: {
              _PARAMS: /* @__PURE__ */ RemoveReaderParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        remove: {
          _PARAMS: /* @__PURE__ */ RemoveAdminParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        signer: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            add: {
              _PARAMS: /* @__PURE__ */ AddSignerParams,
              _RETURN: /* @__PURE__ */ Guid,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            edit: {
              _PARAMS: /* @__PURE__ */ EditSignerParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            list: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(
                  object({ format: IoFormat.nullable().default(() => null) }),
                  empty,
                ),
                empty,
              ),
              _RETURN: /* @__PURE__ */ z.record(z.string(), SignerInfo),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            remove: {
              _PARAMS: /* @__PURE__ */ RemoveSignerParams,
              _RETURN: /* @__PURE__ */ z.null(),
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
          _PARAMS: /* @__PURE__ */ ApplyWithPathParams,
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
        subscribe: {
          _PARAMS: /* @__PURE__ */ SubscribeIndexParams,
          _RETURN: /* @__PURE__ */ SubscribeResult,
          _ITEM: /* @__PURE__ */ Revision,
          _FINAL: /* @__PURE__ */ z.null(),
        },
      },
    },
    index: {
      _PARAMS: /* @__PURE__ */ z.intersection(
        z.intersection(
          object({ format: IoFormat.nullable().default(() => null) }),
          empty,
        ),
        ReaderParams,
      ),
      _RETURN: /* @__PURE__ */ FullIndex,
      _ITEM: /* @__PURE__ */ z.never(),
      _FINAL: /* @__PURE__ */ z.null(),
    },
    info: {
      _PARAMS: /* @__PURE__ */ z.intersection(
        object({ format: IoFormat.nullable().default(() => null) }),
        empty,
      ),
      _RETURN: /* @__PURE__ */ RegistryInfo,
      _ITEM: /* @__PURE__ */ z.never(),
      _FINAL: /* @__PURE__ */ z.null(),
      _CHILDREN: {
        'set-default-locale': {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(
              object({ format: IoFormat.nullable().default(() => null) }),
              empty,
            ),
            SetDefaultLocaleParams,
          ),
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'set-description': {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(
              object({ format: IoFormat.nullable().default(() => null) }),
              empty,
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
              object({ format: IoFormat.nullable().default(() => null) }),
              empty,
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
              object({ format: IoFormat.nullable().default(() => null) }),
              empty,
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
              object({ format: IoFormat.nullable().default(() => null) }),
              empty,
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
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        downloads: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(
              object({ format: IoFormat.nullable().default(() => null) }),
              empty,
            ),
            GetDownloadsParams,
          ),
          _RETURN: /* @__PURE__ */ DownloadsResponse,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        summary: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(
              object({ format: IoFormat.nullable().default(() => null) }),
              empty,
            ),
            empty,
          ),
          _RETURN: /* @__PURE__ */ MetricsSummary,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        users: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(
              object({ format: IoFormat.nullable().default(() => null) }),
              empty,
            ),
            GetUsersParams,
          ),
          _RETURN: /* @__PURE__ */ UsersResponse,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
      },
    },
    os: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        asset: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            add: {
              _PARAMS: /* @__PURE__ */ empty,
              _CHILDREN: {
                img: {
                  _PARAMS: /* @__PURE__ */ AddAssetParams,
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                iso: {
                  _PARAMS: /* @__PURE__ */ AddAssetParams,
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                squashfs: {
                  _PARAMS: /* @__PURE__ */ AddAssetParams,
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
              },
            },
            get: {
              _PARAMS: /* @__PURE__ */ empty,
              _CHILDREN: {
                img: {
                  _PARAMS: /* @__PURE__ */ GetOsAssetParams,
                  _RETURN: /* @__PURE__ */ RegistryAsset(Blake3Commitment),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                iso: {
                  _PARAMS: /* @__PURE__ */ GetOsAssetParams,
                  _RETURN: /* @__PURE__ */ RegistryAsset(Blake3Commitment),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                squashfs: {
                  _PARAMS: /* @__PURE__ */ GetOsAssetParams,
                  _RETURN: /* @__PURE__ */ RegistryAsset(Blake3Commitment),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
              },
            },
            remove: {
              _PARAMS: /* @__PURE__ */ empty,
              _CHILDREN: {
                img: {
                  _PARAMS: /* @__PURE__ */ RemoveAssetParams,
                  _RETURN: /* @__PURE__ */ z.boolean(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                iso: {
                  _PARAMS: /* @__PURE__ */ RemoveAssetParams,
                  _RETURN: /* @__PURE__ */ z.boolean(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                squashfs: {
                  _PARAMS: /* @__PURE__ */ RemoveAssetParams,
                  _RETURN: /* @__PURE__ */ z.boolean(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
              },
            },
            sign: {
              _PARAMS: /* @__PURE__ */ empty,
              _CHILDREN: {
                img: {
                  _PARAMS: /* @__PURE__ */ SignAssetParams,
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                iso: {
                  _PARAMS: /* @__PURE__ */ SignAssetParams,
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                squashfs: {
                  _PARAMS: /* @__PURE__ */ SignAssetParams,
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
            z.intersection(
              object({ format: IoFormat.nullable().default(() => null) }),
              empty,
            ),
            empty,
          ),
          _RETURN: /* @__PURE__ */ OsIndex,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        version: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            add: {
              _PARAMS: /* @__PURE__ */ AddVersionParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            edit: {
              _PARAMS: /* @__PURE__ */ EditVersionParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            get: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(
                  object({ format: IoFormat.nullable().default(() => null) }),
                  empty,
                ),
                GetOsVersionParams,
              ),
              _RETURN: /* @__PURE__ */ OsVersionInfoMap,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            remove: {
              _PARAMS: /* @__PURE__ */ RemoveVersionParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            signer: {
              _PARAMS: /* @__PURE__ */ empty,
              _CHILDREN: {
                add: {
                  _PARAMS: /* @__PURE__ */ VersionSignerParams,
                  _RETURN: /* @__PURE__ */ z.null(),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                list: {
                  _PARAMS: /* @__PURE__ */ z.intersection(
                    z.intersection(
                      object({
                        format: IoFormat.nullable().default(() => null),
                      }),
                      empty,
                    ),
                    ListVersionSignersParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.record(z.string(), SignerInfo),
                  _ITEM: /* @__PURE__ */ z.never(),
                  _FINAL: /* @__PURE__ */ z.null(),
                },
                remove: {
                  _PARAMS: /* @__PURE__ */ VersionSignerParams,
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
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        add: {
          _PARAMS: /* @__PURE__ */ AddPackageParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'add-mirror': {
          _PARAMS: /* @__PURE__ */ AddMirrorParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        category: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            add: {
              _PARAMS: /* @__PURE__ */ AddCategoryParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'add-package': {
              _PARAMS: /* @__PURE__ */ AddPackageToCategoryParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            list: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(
                  object({ format: IoFormat.nullable().default(() => null) }),
                  empty,
                ),
                empty,
              ),
              _RETURN: /* @__PURE__ */ z.record(z.string(), Category),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            remove: {
              _PARAMS: /* @__PURE__ */ RemoveCategoryParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            'remove-package': {
              _PARAMS: /* @__PURE__ */ RemovePackageFromCategoryParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        get: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(
              object({ format: IoFormat.nullable().default(() => null) }),
              empty,
            ),
            GetPackageParams,
          ),
          _RETURN: /* @__PURE__ */ GetPackageResult,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        index: {
          _PARAMS: /* @__PURE__ */ z.intersection(
            z.intersection(
              object({ format: IoFormat.nullable().default(() => null) }),
              empty,
            ),
            ReaderParams,
          ),
          _RETURN: /* @__PURE__ */ PackageIndex,
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        remove: {
          _PARAMS: /* @__PURE__ */ RemovePackageParams,
          _RETURN: /* @__PURE__ */ z.boolean(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        'remove-mirror': {
          _PARAMS: /* @__PURE__ */ RemoveMirrorParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        sign: {
          _PARAMS: /* @__PURE__ */ SignPackageParams,
          _RETURN: /* @__PURE__ */ z.null(),
          _ITEM: /* @__PURE__ */ z.never(),
          _FINAL: /* @__PURE__ */ z.null(),
        },
        signer: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            add: {
              _PARAMS: /* @__PURE__ */ AddPackageSignerParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            list: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(
                  object({ format: IoFormat.nullable().default(() => null) }),
                  empty,
                ),
                ListPackageSignersParams,
              ),
              _RETURN: /* @__PURE__ */ z.record(
                z.string(),
                z.tuple([SignerInfo, z.string()]),
              ),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
            remove: {
              _PARAMS: /* @__PURE__ */ RemovePackageSignerParams,
              _RETURN: /* @__PURE__ */ z.null(),
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
        },
        version: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            export: {
              _PARAMS: /* @__PURE__ */ z.intersection(
                z.intersection(
                  object({ format: IoFormat.nullable().default(() => null) }),
                  empty,
                ),
                ExportPackageVersionParams,
              ),
              _RETURN: /* @__PURE__ */ PackageVersionExport,
              _ITEM: /* @__PURE__ */ z.never(),
              _FINAL: /* @__PURE__ */ z.null(),
            },
          },
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
