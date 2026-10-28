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
  class: literal_2,
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
export const GitHash: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'GitHash',
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
export const RemoveVersionParams: z.ZodObject<{ version: z.ZodString }> =
  /* @__PURE__ */ object({ version: z.string() }).meta({
    id: 'RemoveVersionParams',
  })
export const SetNameParams: z.ZodObject<{ name: z.ZodString }> =
  /* @__PURE__ */ object({ name: z.string() }).meta({
    id: 'SetNameParams',
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
  commitment: typeof Blake3Commitment
}> = /* @__PURE__ */ object({
  version: z.string(),
  platform: z.string(),
  url: z.string(),
  signature: AnySignature,
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
}> = /* @__PURE__ */ object({
  url: z.string(),
  commitment: MerkleArchiveCommitment,
  signature: AnySignature,
}).meta({
  id: 'AddMirrorParams',
})
export const AddPackageParams: z.ZodObject<{
  urls: z.ZodArray<z.ZodString>
  commitment: typeof MerkleArchiveCommitment
  signature: typeof AnySignature
}> = /* @__PURE__ */ object({
  urls: z.array(z.string()),
  commitment: MerkleArchiveCommitment,
  signature: AnySignature,
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
export const AddVersionParams: z.ZodObject<{
  version: z.ZodString
  headline: z.ZodString
  releaseNotes: typeof LocaleString
  sourceVersion: z.ZodString
}> = /* @__PURE__ */ object({
  version: z.string(),
  headline: z.string(),
  releaseNotes: LocaleString,
  sourceVersion: z.string(),
}).meta({
  id: 'AddVersionParams',
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
export const EditSignerParams: z.ZodObject<{
  id: typeof Guid
  setName: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  addContact: z.ZodArray<typeof ContactInfo>
  addKey: z.ZodArray<typeof AnyVerifyingKey>
  removeContact: z.ZodArray<typeof ContactInfo>
  removeKey: z.ZodArray<typeof AnyVerifyingKey>
}> = /* @__PURE__ */ object({
  id: Guid,
  setName: wire(z.string().nullable().optional(), z.string().nullable()),
  addContact: z.array(ContactInfo),
  addKey: z.array(AnyVerifyingKey),
  removeContact: z.array(ContactInfo),
  removeKey: z.array(AnyVerifyingKey),
}).meta({
  id: 'EditSignerParams',
})
export const GetPackageParams: z.ZodObject<{
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
}> = /* @__PURE__ */ object({
  id: wire(PackageId.nullable().optional(), PackageId.nullable()),
  targetVersion: wire(z.string().nullable().optional(), z.string().nullable()),
  sourceVersion: wire(Version.nullable().optional(), Version.nullable()),
  otherVersions: wire(
    PackageDetailLevel.nullable().optional(),
    PackageDetailLevel.nullable(),
  ),
  allRevisions: wire(z.boolean().optional(), z.boolean()),
}).meta({
  id: 'GetPackageParams',
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
export const ListPackageSignersParams: z.ZodObject<{ id: typeof PackageId }> =
  /* @__PURE__ */ object({ id: PackageId }).meta({
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
  headline: z.ZodString
  releaseNotes: typeof LocaleString
  sourceVersion: z.ZodString
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
}> = /* @__PURE__ */ object({ releaseNotes: LocaleString }).meta({
  id: 'PackageInfoShort',
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
export const SignerInfo: z.ZodObject<{
  name: z.ZodString
  contact: z.ZodArray<typeof ContactInfo>
  keys: z.ZodArray<typeof AnyVerifyingKey>
}> = /* @__PURE__ */ object({
  name: z.string(),
  contact: z.array(ContactInfo),
  keys: z.array(AnyVerifyingKey),
}).meta({
  id: 'SignerInfo',
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
export const OsVersionInfoMap: z.ZodRecord<z.ZodString, typeof OsVersionInfo> =
  /* @__PURE__ */ z.record(z.string(), OsVersionInfo).meta({
    id: 'OsVersionInfoMap',
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
}).meta({
  id: 'RegistryInfo',
})
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
          _RETURN: /* @__PURE__ */ z.record(z.string(), SignerInfo),
        },
        remove: {
          _PARAMS: /* @__PURE__ */ RemoveAdminParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        signer: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            add: {
              _PARAMS: /* @__PURE__ */ SignerInfo,
              _RETURN: /* @__PURE__ */ Guid,
            },
            edit: {
              _PARAMS: /* @__PURE__ */ EditSignerParams,
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
              _RETURN: /* @__PURE__ */ z.record(z.string(), SignerInfo),
            },
            remove: {
              _PARAMS: /* @__PURE__ */ RemoveSignerParams,
              _RETURN: /* @__PURE__ */ z.null(),
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
        },
        dump: {
          _PARAMS: /* @__PURE__ */ DumpParams,
          _RETURN: /* @__PURE__ */ Dump,
        },
        subscribe: {
          _PARAMS: /* @__PURE__ */ DumpParams,
          _RETURN: /* @__PURE__ */ SubscribeRes,
        },
      },
    },
    index: {
      _PARAMS: /* @__PURE__ */ z.intersection(
        z.intersection(
          object({
            format: wire(IoFormat.nullable().optional(), IoFormat.nullable()),
          }),
          empty,
        ),
        empty,
      ),
      _RETURN: /* @__PURE__ */ FullIndex,
    },
    info: {
      _PARAMS: /* @__PURE__ */ z.intersection(
        object({
          format: wire(IoFormat.nullable().optional(), IoFormat.nullable()),
        }),
        empty,
      ),
      _RETURN: /* @__PURE__ */ RegistryInfo,
      _CHILDREN: {
        'set-description': {
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
            SetDescriptionParams,
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-icon': {
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
            SetIconParams,
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'set-name': {
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
            SetNameParams,
          ),
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
    metrics: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        downloads: {
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
            GetDownloadsParams,
          ),
          _RETURN: /* @__PURE__ */ DownloadsResponse,
        },
        summary: {
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
          _RETURN: /* @__PURE__ */ MetricsSummary,
        },
        users: {
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
            GetUsersParams,
          ),
          _RETURN: /* @__PURE__ */ UsersResponse,
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
                },
                iso: {
                  _PARAMS: /* @__PURE__ */ AddAssetParams,
                  _RETURN: /* @__PURE__ */ z.null(),
                },
                squashfs: {
                  _PARAMS: /* @__PURE__ */ AddAssetParams,
                  _RETURN: /* @__PURE__ */ z.null(),
                },
              },
            },
            get: {
              _PARAMS: /* @__PURE__ */ empty,
              _CHILDREN: {
                img: {
                  _PARAMS: /* @__PURE__ */ GetOsAssetParams,
                  _RETURN: /* @__PURE__ */ RegistryAsset(Blake3Commitment),
                },
                iso: {
                  _PARAMS: /* @__PURE__ */ GetOsAssetParams,
                  _RETURN: /* @__PURE__ */ RegistryAsset(Blake3Commitment),
                },
                squashfs: {
                  _PARAMS: /* @__PURE__ */ GetOsAssetParams,
                  _RETURN: /* @__PURE__ */ RegistryAsset(Blake3Commitment),
                },
              },
            },
            remove: {
              _PARAMS: /* @__PURE__ */ empty,
              _CHILDREN: {
                img: {
                  _PARAMS: /* @__PURE__ */ RemoveAssetParams,
                  _RETURN: /* @__PURE__ */ z.boolean(),
                },
                iso: {
                  _PARAMS: /* @__PURE__ */ RemoveAssetParams,
                  _RETURN: /* @__PURE__ */ z.boolean(),
                },
                squashfs: {
                  _PARAMS: /* @__PURE__ */ RemoveAssetParams,
                  _RETURN: /* @__PURE__ */ z.boolean(),
                },
              },
            },
            sign: {
              _PARAMS: /* @__PURE__ */ empty,
              _CHILDREN: {
                img: {
                  _PARAMS: /* @__PURE__ */ SignAssetParams,
                  _RETURN: /* @__PURE__ */ z.null(),
                },
                iso: {
                  _PARAMS: /* @__PURE__ */ SignAssetParams,
                  _RETURN: /* @__PURE__ */ z.null(),
                },
                squashfs: {
                  _PARAMS: /* @__PURE__ */ SignAssetParams,
                  _RETURN: /* @__PURE__ */ z.null(),
                },
              },
            },
          },
        },
        index: {
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
          _RETURN: /* @__PURE__ */ OsIndex,
        },
        version: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            add: {
              _PARAMS: /* @__PURE__ */ AddVersionParams,
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
                GetOsVersionParams,
              ),
              _RETURN: /* @__PURE__ */ OsVersionInfoMap,
            },
            remove: {
              _PARAMS: /* @__PURE__ */ RemoveVersionParams,
              _RETURN: /* @__PURE__ */ z.null(),
            },
            signer: {
              _PARAMS: /* @__PURE__ */ empty,
              _CHILDREN: {
                add: {
                  _PARAMS: /* @__PURE__ */ VersionSignerParams,
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
                    ListVersionSignersParams,
                  ),
                  _RETURN: /* @__PURE__ */ z.record(z.string(), SignerInfo),
                },
                remove: {
                  _PARAMS: /* @__PURE__ */ VersionSignerParams,
                  _RETURN: /* @__PURE__ */ z.null(),
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
        },
        'add-mirror': {
          _PARAMS: /* @__PURE__ */ AddMirrorParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        category: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            add: {
              _PARAMS: /* @__PURE__ */ AddCategoryParams,
              _RETURN: /* @__PURE__ */ z.null(),
            },
            'add-package': {
              _PARAMS: /* @__PURE__ */ AddPackageToCategoryParams,
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
              _RETURN: /* @__PURE__ */ z.record(z.string(), Category),
            },
            remove: {
              _PARAMS: /* @__PURE__ */ RemoveCategoryParams,
              _RETURN: /* @__PURE__ */ z.null(),
            },
            'remove-package': {
              _PARAMS: /* @__PURE__ */ RemovePackageFromCategoryParams,
              _RETURN: /* @__PURE__ */ z.null(),
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
            GetPackageParams,
          ),
          _RETURN: /* @__PURE__ */ GetPackageResult,
        },
        index: {
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
          _RETURN: /* @__PURE__ */ PackageIndex,
        },
        remove: {
          _PARAMS: /* @__PURE__ */ RemovePackageParams,
          _RETURN: /* @__PURE__ */ z.boolean(),
        },
        'remove-mirror': {
          _PARAMS: /* @__PURE__ */ RemoveMirrorParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        signer: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            add: {
              _PARAMS: /* @__PURE__ */ AddPackageSignerParams,
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
                ListPackageSignersParams,
              ),
              _RETURN: /* @__PURE__ */ z.record(
                z.string(),
                z.tuple([SignerInfo, z.string()]),
              ),
            },
            remove: {
              _PARAMS: /* @__PURE__ */ RemovePackageSignerParams,
              _RETURN: /* @__PURE__ */ z.null(),
            },
          },
        },
      },
    },
  },
}
