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
export const ActionResultValue: z.ZodUnion<
  readonly [
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
export const BackupTargetId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'BackupTargetId',
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
export const ClearCallbacksParams: typeof literal_7 =
  /* @__PURE__ */ literal_7.meta({
    id: 'ClearCallbacksParams',
  })
export const ClearTasksParams: typeof literal_8 =
  /* @__PURE__ */ literal_8.meta({
    id: 'ClearTasksParams',
  })
export const CliSetIconParams: z.ZodObject<{
  icon: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  clear: z.ZodBoolean
}> = /* @__PURE__ */ object({
  icon: wire(z.string().nullable().optional(), z.string().nullable()),
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
export const FileType: z.ZodEnum<{
  file: 'file'
  directory: 'directory'
  infer: 'infer'
}> = /* @__PURE__ */ z.enum(['file', 'directory', 'infer']).meta({
  id: 'FileType',
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
export const GigaBytes: typeof literal_10 = /* @__PURE__ */ literal_10.meta({
  id: 'GigaBytes',
})
export const GitHash: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'GitHash',
})
export const Governor: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'Governor',
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
  entrypoint: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<z.ZodString>>>,
    z.ZodNullable<z.ZodArray<z.ZodString>>
  >
  cmd: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<z.ZodString>>>,
    z.ZodNullable<z.ZodArray<z.ZodString>>
  >
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
  entrypoint: wire(
    z.array(z.string()).nullable().optional(),
    z.array(z.string()).nullable(),
  ),
  cmd: wire(
    z.array(z.string()).nullable().optional(),
    z.array(z.string()).nullable(),
  ),
}).meta({
  id: 'ImageMetadata',
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
export const ListVersionSignersParams: z.ZodObject<{ version: z.ZodString }> =
  /* @__PURE__ */ object({ version: z.string() }).meta({
    id: 'ListVersionSignersParams',
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
    limit: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<z.ZodInt>>,
      z.ZodNullable<z.ZodInt>
    >
    cursor: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<z.ZodString>>,
      z.ZodNullable<z.ZodString>
    >
    boot: z.ZodCodec<
      z.ZodOptional<
        z.ZodNullable<z.ZodUnion<readonly [z.ZodNumber, z.ZodString]>>
      >,
      z.ZodNullable<z.ZodUnion<readonly [z.ZodNumber, z.ZodString]>>
    >
    before: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  }>,
  typeof empty
> = /* @__PURE__ */ z
  .intersection(
    object({
      limit: wire(z.int().nullable().optional(), z.int().nullable()),
      cursor: wire(z.string().nullable().optional(), z.string().nullable()),
      boot: wire(
        z.union([z.number(), z.string()]).nullable().optional(),
        z.union([z.number(), z.string()]).nullable(),
      ),
      before: wire(z.boolean().optional(), z.boolean()),
    }),
    empty,
  )
  .meta({
    id: 'LogsParams',
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
      z.ZodObject<{
        message: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodString>>,
          z.ZodNullable<z.ZodString>
        >
      }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ result: z.ZodLiteral<'disabled'> }>,
      z.ZodObject<{
        message: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodString>>,
          z.ZodNullable<z.ZodString>
        >
      }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ result: z.ZodLiteral<'starting'> }>,
      z.ZodObject<{
        message: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodString>>,
          z.ZodNullable<z.ZodString>
        >
      }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ result: z.ZodLiteral<'waiting'> }>,
      z.ZodObject<{
        message: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodString>>,
          z.ZodNullable<z.ZodString>
        >
      }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ result: z.ZodLiteral<'loading'> }>,
      z.ZodObject<{ message: z.ZodString }>
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
        message: wire(z.string().nullable().optional(), z.string().nullable()),
      }),
    ),
    z.intersection(
      object({ result: z.literal('disabled') }),
      object({
        message: wire(z.string().nullable().optional(), z.string().nullable()),
      }),
    ),
    z.intersection(
      object({ result: z.literal('starting') }),
      object({
        message: wire(z.string().nullable().optional(), z.string().nullable()),
      }),
    ),
    z.intersection(
      object({ result: z.literal('waiting') }),
      object({
        message: wire(z.string().nullable().optional(), z.string().nullable()),
      }),
    ),
    z.intersection(
      object({ result: z.literal('loading') }),
      object({ message: z.string() }),
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
export const RemoveDomainParams: z.ZodObject<{ fqdn: z.ZodString }> =
  /* @__PURE__ */ object({ fqdn: z.string() }).meta({
    id: 'RemoveDomainParams',
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
export const SetDataVersionParams: z.ZodObject<{
  version: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  version: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'SetDataVersionParams',
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
export const SideloadParams: typeof empty = /* @__PURE__ */ empty.meta({
  id: 'SideloadParams',
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
export const TaskCondition: z.ZodEnum<{
  'input-not-matches': 'input-not-matches'
}> = /* @__PURE__ */ z.enum(['input-not-matches']).meta({
  id: 'TaskCondition',
})
export const TaskInputRepr: z.ZodIntersection<
  z.ZodObject<{ kind: z.ZodLiteral<'partial'> }>,
  z.ZodObject<{
    accept: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<z.ZodArray<z.ZodUnknown>>>,
      z.ZodNullable<z.ZodArray<z.ZodUnknown>>
    >
    set: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<z.ZodUnknown>>,
      z.ZodNullable<z.ZodUnknown>
    >
    value: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<z.ZodUnknown>>,
      z.ZodNullable<z.ZodUnknown>
    >
  }>
> = /* @__PURE__ */ z
  .intersection(
    object({ kind: z.literal('partial') }),
    object({
      accept: wire(
        z.array(z.unknown()).nullable().optional(),
        z.array(z.unknown()).nullable(),
      ),
      set: wire(z.unknown().nullable().optional(), z.unknown().nullable()),
      value: wire(z.unknown().nullable().optional(), z.unknown().nullable()),
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
export const TrustedOsUpdateSigner: z.ZodObject<{ label: z.ZodString }> =
  /* @__PURE__ */ object({ label: z.string() }).meta({
    id: 'TrustedOsUpdateSigner',
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
  warning: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
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
   * One of: "only-stopped", "only-running", "all"
   *   - "only-stopped" - the action can only be run when the service is stopped
   *   - "only-running" - the action can only be run when the service is running
   *   - "any" - the action can only be run regardless of the service's status
   */
  allowedStatuses: typeof AllowedStatuses
  hasInput: z.ZodBoolean
  /**
   * If provided, this action will be nested under a header of this value, along with other actions of the same group
   */
  group: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  /**
   * Who is allowed to invoke this action directly via `effects.action.run`.
   *   - "public" — any installed package
   *   - "dependent" — only services that declare this package as a current dependency
   *   - "user" — only the user (other services must create a task). Default when omitted.
   * Services that lack direct access can always queue a task with `effects.action.createTask`.
   */
  access: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof ActionAccess>>,
    z.ZodNullable<typeof ActionAccess>
  >
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
  warning: wire(z.string().nullable().optional(), z.string().nullable()).meta({
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
   * One of: "only-stopped", "only-running", "all"
   *   - "only-stopped" - the action can only be run when the service is stopped
   *   - "only-running" - the action can only be run when the service is running
   *   - "any" - the action can only be run regardless of the service's status
   */
  allowedStatuses: AllowedStatuses.meta({
    description:
      'One of: "only-stopped", "only-running", "all"\n  - "only-stopped" - the action can only be run when the service is stopped\n  - "only-running" - the action can only be run when the service is running\n  - "any" - the action can only be run regardless of the service\'s status',
  }),
  hasInput: z.boolean(),
  /**
   * If provided, this action will be nested under a header of this value, along with other actions of the same group
   */
  group: wire(z.string().nullable().optional(), z.string().nullable()).meta({
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
  access: wire(
    ActionAccess.nullable().optional(),
    ActionAccess.nullable(),
  ).meta({
    description:
      'Who is allowed to invoke this action directly via `effects.action.run`.\n  - "public" — any installed package\n  - "dependent" — only services that declare this package as a current dependency\n  - "user" — only the user (other services must create a task). Default when omitted.\nServices that lack direct access can always queue a task with `effects.action.createTask`.',
  }),
}).meta({
  id: 'ActionMetadata',
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
}> = /* @__PURE__ */ object({
  version: z.string(),
  headline: z.string(),
  releaseNotes: LocaleString,
  sourceVersion: z.string(),
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

/**
 * Enrolled sign-ins indexed by canonical DER64 SPKI.
 */
export const AuthKeys: z.ZodRecord<z.ZodString, typeof Session> =
  /* @__PURE__ */ z.record(z.string(), Session).meta({
    id: 'AuthKeys',
    description: 'Enrolled sign-ins indexed by canonical DER64 SPKI.',
  })
export const BackupReport: z.ZodObject<{
  server: typeof ServerBackupReport
  packages: z.ZodRecord<z.ZodString, typeof PackageBackupReport>
}> = /* @__PURE__ */ object({
  server: ServerBackupReport,
  packages: z.record(z.string(), PackageBackupReport),
}).meta({
  id: 'BackupReport',
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
export const BindRangeParams: z.ZodObject<{
  id: typeof HostId
  internalStartPort: z.ZodInt
  externalStartPort: z.ZodInt
  numberOfPorts: z.ZodInt
}> = /* @__PURE__ */ object({
  id: HostId,
  internalStartPort: z.int(),
  externalStartPort: z.int(),
  numberOfPorts: z.int(),
}).meta({
  id: 'BindRangeParams',
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
export const CheckDependenciesParam: z.ZodObject<{
  packageIds: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<typeof PackageId>>>,
    z.ZodNullable<z.ZodArray<typeof PackageId>>
  >
}> = /* @__PURE__ */ object({
  packageIds: wire(
    z.array(PackageId).nullable().optional(),
    z.array(PackageId).nullable(),
  ),
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
  force: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  packageId: PackageId,
  replayId: ReplayId,
  force: wire(z.boolean().optional(), z.boolean()),
}).meta({
  id: 'ClearTaskParams',
})
export const CliSetDescriptionParams: z.ZodObject<{
  description: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof LocaleString>>,
    z.ZodNullable<typeof LocaleString>
  >
  clear: z.ZodBoolean
}> = /* @__PURE__ */ object({
  description: wire(
    LocaleString.nullable().optional(),
    LocaleString.nullable(),
  ),
  clear: z.boolean(),
}).meta({
  id: 'CliSetDescriptionParams',
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
  data: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
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
  data: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      'Optional long-form, markdown-formatted body that the UI renders in a\n"View Details" modal when present (release notes, post-update\nchangelogs, structured error reports). When omitted, the notification\nhas no extra payload.',
  }),
}).meta({
  id: 'CreateNotificationParams',
})
export const CreateSubcontainerFsParams: z.ZodObject<{
  imageId: typeof ImageId
  name: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  imageId: ImageId,
  name: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'CreateSubcontainerFsParams',
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
      z.ZodObject<{
        restartAgain: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
      }>
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
      object({ restartAgain: wire(z.boolean().optional(), z.boolean()) }),
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
export const EffectsRunActionParams: z.ZodObject<{
  packageId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof PackageId>>,
    z.ZodNullable<typeof PackageId>
  >
  actionId: typeof ActionId
  input: z.ZodUnknown
}> = /* @__PURE__ */ object({
  packageId: wire(PackageId.nullable().optional(), PackageId.nullable()),
  actionId: ActionId,
  input: z.unknown(),
}).meta({
  id: 'EffectsRunActionParams',
})
export const ExportRangeServiceInterfaceParams: z.ZodObject<{
  hostId: typeof HostId
  internalStartPort: z.ZodInt
  id: typeof ServiceInterfaceId
  name: z.ZodString
  description: z.ZodString
  scheme: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  hostId: HostId,
  internalStartPort: z.int(),
  id: ServiceInterfaceId,
  name: z.string(),
  description: z.string(),
  scheme: wire(z.string().nullable().optional(), z.string().nullable()),
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
  packageId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof PackageId>>,
    z.ZodNullable<typeof PackageId>
  >
  actionId: typeof ActionId
  prefill: z.ZodCodec<z.ZodOptional<typeof literal_2>, typeof literal_2>
}> = /* @__PURE__ */ object({
  packageId: wire(PackageId.nullable().optional(), PackageId.nullable()),
  actionId: ActionId,
  prefill: wire(literal_2.optional(), literal_2),
}).meta({
  id: 'GetActionInputParams',
})
export const GetContainerIpParams: z.ZodObject<{
  packageId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof PackageId>>,
    z.ZodNullable<typeof PackageId>
  >
  callback: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof CallbackId>>,
    z.ZodNullable<typeof CallbackId>
  >
}> = /* @__PURE__ */ object({
  packageId: wire(PackageId.nullable().optional(), PackageId.nullable()),
  callback: wire(CallbackId.nullable().optional(), CallbackId.nullable()),
}).meta({
  id: 'GetContainerIpParams',
})
export const GetHostInfoParams: z.ZodObject<{
  hostId: typeof HostId
  packageId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof PackageId>>,
    z.ZodNullable<typeof PackageId>
  >
  callback: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof CallbackId>>,
    z.ZodNullable<typeof CallbackId>
  >
}> = /* @__PURE__ */ object({
  hostId: HostId,
  packageId: wire(PackageId.nullable().optional(), PackageId.nullable()),
  callback: wire(CallbackId.nullable().optional(), CallbackId.nullable()),
}).meta({
  id: 'GetHostInfoParams',
})
export const GetOutboundGatewayParams: z.ZodObject<{
  callback: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof CallbackId>>,
    z.ZodNullable<typeof CallbackId>
  >
}> = /* @__PURE__ */ object({
  callback: wire(CallbackId.nullable().optional(), CallbackId.nullable()),
}).meta({
  id: 'GetOutboundGatewayParams',
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
export const GetServiceInterfaceParams: z.ZodObject<{
  packageId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof PackageId>>,
    z.ZodNullable<typeof PackageId>
  >
  serviceInterfaceId: typeof ServiceInterfaceId
  callback: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof CallbackId>>,
    z.ZodNullable<typeof CallbackId>
  >
}> = /* @__PURE__ */ object({
  packageId: wire(PackageId.nullable().optional(), PackageId.nullable()),
  serviceInterfaceId: ServiceInterfaceId,
  callback: wire(CallbackId.nullable().optional(), CallbackId.nullable()),
}).meta({
  id: 'GetServiceInterfaceParams',
})
export const GetServiceManifestParams: z.ZodObject<{
  packageId: typeof PackageId
  callback: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof CallbackId>>,
    z.ZodNullable<typeof CallbackId>
  >
}> = /* @__PURE__ */ object({
  packageId: PackageId,
  callback: wire(CallbackId.nullable().optional(), CallbackId.nullable()),
}).meta({
  id: 'GetServiceManifestParams',
})
export const GetServicePortForwardParams: z.ZodObject<{
  packageId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof PackageId>>,
    z.ZodNullable<typeof PackageId>
  >
  hostId: typeof HostId
  internalPort: z.ZodInt
}> = /* @__PURE__ */ object({
  packageId: wire(PackageId.nullable().optional(), PackageId.nullable()),
  hostId: HostId,
  internalPort: z.int(),
}).meta({
  id: 'GetServicePortForwardParams',
})
export const GetSslCertificateParams: z.ZodObject<{
  hostnames: z.ZodArray<z.ZodString>
  algorithm: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Algorithm>>,
    z.ZodNullable<typeof Algorithm>
  >
  callback: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof CallbackId>>,
    z.ZodNullable<typeof CallbackId>
  >
}> = /* @__PURE__ */ object({
  hostnames: z.array(z.string()),
  algorithm: wire(Algorithm.nullable().optional(), Algorithm.nullable()),
  callback: wire(CallbackId.nullable().optional(), CallbackId.nullable()),
}).meta({
  id: 'GetSslCertificateParams',
})
export const GetSslKeyParams: z.ZodObject<{
  hostnames: z.ZodArray<z.ZodString>
  algorithm: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Algorithm>>,
    z.ZodNullable<typeof Algorithm>
  >
}> = /* @__PURE__ */ object({
  hostnames: z.array(z.string()),
  algorithm: wire(Algorithm.nullable().optional(), Algorithm.nullable()),
}).meta({
  id: 'GetSslKeyParams',
})
export const GetStatusParams: z.ZodObject<{
  packageId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof PackageId>>,
    z.ZodNullable<typeof PackageId>
  >
  callback: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof CallbackId>>,
    z.ZodNullable<typeof CallbackId>
  >
}> = /* @__PURE__ */ object({
  packageId: wire(PackageId.nullable().optional(), PackageId.nullable()),
  callback: wire(CallbackId.nullable().optional(), CallbackId.nullable()),
}).meta({
  id: 'GetStatusParams',
})
export const GetSystemSmtpParams: z.ZodObject<{
  callback: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof CallbackId>>,
    z.ZodNullable<typeof CallbackId>
  >
}> = /* @__PURE__ */ object({
  callback: wire(CallbackId.nullable().optional(), CallbackId.nullable()),
}).meta({
  id: 'GetSystemSmtpParams',
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
export const ImageSource: z.ZodUnion<
  readonly [
    z.ZodLiteral<'packed'>,
    z.ZodObject<{
      dockerBuild: z.ZodObject<{
        workdir: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodString>>,
          z.ZodNullable<z.ZodString>
        >
        dockerfile: z.ZodCodec<
          z.ZodOptional<z.ZodNullable<z.ZodString>>,
          z.ZodNullable<z.ZodString>
        >
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
        workdir: wire(z.string().nullable().optional(), z.string().nullable()),
        dockerfile: wire(
          z.string().nullable().optional(),
          z.string().nullable(),
        ),
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
export const InstallParams: z.ZodObject<{
  registry: z.ZodString
  id: typeof PackageId
  version: typeof Version
}> = /* @__PURE__ */ object({
  registry: z.string(),
  id: PackageId,
  version: Version,
}).meta({
  id: 'InstallParams',
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
export const ListPackageSignersParams: z.ZodObject<{ id: typeof PackageId }> =
  /* @__PURE__ */ object({ id: PackageId }).meta({
    id: 'ListPackageSignersParams',
  })
export const ListServiceInterfacesParams: z.ZodObject<{
  packageId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof PackageId>>,
    z.ZodNullable<typeof PackageId>
  >
  callback: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof CallbackId>>,
    z.ZodNullable<typeof CallbackId>
  >
}> = /* @__PURE__ */ object({
  packageId: wire(PackageId.nullable().optional(), PackageId.nullable()),
  callback: wire(CallbackId.nullable().optional(), CallbackId.nullable()),
}).meta({
  id: 'ListServiceInterfacesParams',
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
export const NamedHealthCheckResult: z.ZodIntersection<
  z.ZodObject<{ name: z.ZodString }>,
  typeof NamedHealthCheckResultKind
> = /* @__PURE__ */ z
  .intersection(object({ name: z.string() }), NamedHealthCheckResultKind)
  .meta({
    id: 'NamedHealthCheckResult',
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
export const OsUpdateSignerKeyParams: z.ZodObject<{
  key: typeof AnyVerifyingKey
}> = /* @__PURE__ */ object({ key: AnyVerifyingKey }).meta({
  id: 'OsUpdateSignerKeyParams',
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
}> = /* @__PURE__ */ object({ releaseNotes: LocaleString }).meta({
  id: 'PackageInfoShort',
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
  scheme: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  id: ServiceInterfaceId,
  name: z.string(),
  description: z.string(),
  scheme: wire(z.string().nullable().optional(), z.string().nullable()),
}).meta({
  id: 'RangeServiceInterface',
  description:
    "The single restricted service interface a port-range binding may export.\n\nUnlike [`ServiceInterface`], a range interface is always `api`-typed and\ncarries no `masked` / `username` / `path` / `query` and no per-address\n[`AddressInfo`] — its address is the host plus the range's external port\nspan, taken from the [`RangeBindInfo`](crate::net::host::binding::RangeBindInfo)\nit lives under. `scheme` is an optional transport prefix (e.g. `tcp` for\nbitcoin ZMQ endpoints); most ranges (coturn RTP, FTP data) omit it.",
})
export const RebuildParams: z.ZodObject<{ id: typeof PackageId }> =
  /* @__PURE__ */ object({ id: PackageId }).meta({
    id: 'RebuildParams',
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
export const ServiceDependencyMetadata: z.ZodObject<{
  title: typeof LocaleString
}> = /* @__PURE__ */ object({ title: LocaleString }).meta({
  id: 'ServiceDependencyMetadata',
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
export const SetMainStatus: z.ZodObject<{
  status: typeof SetMainStatusStatus
}> = /* @__PURE__ */ object({ status: SetMainStatusStatus }).meta({
  id: 'SetMainStatus',
})
export const SetOsUpdateSignerParams: z.ZodObject<{
  key: typeof AnyVerifyingKey
  label: z.ZodString
}> = /* @__PURE__ */ object({ key: AnyVerifyingKey, label: z.string() }).meta({
  id: 'SetOsUpdateSignerParams',
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
export const SideloadResponse: z.ZodObject<{
  upload: typeof Guid
  progress: typeof Guid
}> = /* @__PURE__ */ object({ upload: Guid, progress: Guid }).meta({
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
}> = /* @__PURE__ */ object({
  hostname: ServerHostname,
  version: z.string(),
  timestamp: z.string(),
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
  once: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  condition: typeof TaskCondition
}> = /* @__PURE__ */ object({
  once: wire(z.boolean().optional(), z.boolean()),
  condition: TaskCondition,
}).meta({
  id: 'TaskTrigger',
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
export const UpdateSystemRes: z.ZodObject<{
  target: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  progress: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Guid>>,
    z.ZodNullable<typeof Guid>
  >
}> = /* @__PURE__ */ object({
  target: wire(z.string().nullable().optional(), z.string().nullable()),
  progress: wire(Guid.nullable().optional(), Guid.nullable()),
}).meta({
  id: 'UpdateSystemRes',
})
export const UpdateTunnelParams: z.ZodObject<{
  id: typeof GatewayId
  config: z.ZodString
}> = /* @__PURE__ */ object({ id: GatewayId, config: z.string() }).meta({
  id: 'UpdateTunnelParams',
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
export const VersionSignerParams: z.ZodObject<{
  version: z.ZodString
  signer: typeof Guid
}> = /* @__PURE__ */ object({ version: z.string(), signer: Guid }).meta({
  id: 'VersionSignerParams',
})
export const WifiInfo: z.ZodObject<{
  enabled: z.ZodBoolean
  interface: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof GatewayId>>,
    z.ZodNullable<typeof GatewayId>
  >
  ssids: z.ZodArray<z.ZodString>
  selected: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  lastRegion: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  enabled: z.boolean(),
  interface: wire(GatewayId.nullable().optional(), GatewayId.nullable()),
  ssids: z.array(z.string()),
  selected: wire(z.string().nullable().optional(), z.string().nullable()),
  lastRegion: wire(z.string().nullable().optional(), z.string().nullable()),
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
}> = /* @__PURE__ */ object({
  version: z.string(),
  timestamp: wire(z.string().nullable().optional(), z.string().nullable()),
  packageBackups: z.record(z.string(), PackageBackupInfo),
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
  password: PasswordType,
}).meta({
  id: 'BackupParams',
})
export const CifsBackupTarget: z.ZodObject<{
  hostname: z.ZodString
  path: z.ZodString
  username: z.ZodString
  mountable: z.ZodBoolean
  available: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodInt>>,
    z.ZodNullable<z.ZodInt>
  >
  startOs: z.ZodRecord<z.ZodString, typeof StartOsRecoveryInfo>
  legacyBackup: z.ZodBoolean
}> = /* @__PURE__ */ object({
  hostname: z.string(),
  path: z.string(),
  username: z.string(),
  mountable: z.boolean(),
  available: wire(z.int().nullable().optional(), z.int().nullable()),
  startOs: z.record(z.string(), StartOsRecoveryInfo),
  legacyBackup: z.boolean(),
}).meta({
  id: 'CifsBackupTarget',
})
export const ClearBindingsParams: z.ZodObject<{
  except: z.ZodCodec<
    z.ZodOptional<z.ZodArray<typeof BindId>>,
    z.ZodArray<typeof BindId>
  >
}> = /* @__PURE__ */ object({
  except: wire(z.array(BindId).optional(), z.array(BindId)),
}).meta({
  id: 'ClearBindingsParams',
})
export const CompatibleAlpnInfo: z.ZodUnion<
  readonly [z.ZodArray<typeof MaybeUtf8String>, typeof LegacyAlpnInfo]
> = /* @__PURE__ */ z.union([z.array(MaybeUtf8String), LegacyAlpnInfo]).meta({
  id: 'CompatibleAlpnInfo',
})
export const CurrentDependencyInfo: z.ZodIntersection<
  z.ZodObject<{
    title: z.ZodCodec<
      z.ZodOptional<z.ZodNullable<typeof LocaleString>>,
      z.ZodNullable<typeof LocaleString>
    >
    versionRange: z.ZodString
  }>,
  typeof CurrentDependencyKind
> = /* @__PURE__ */ z
  .intersection(
    object({
      title: wire(LocaleString.nullable().optional(), LocaleString.nullable()),
      versionRange: z.string(),
    }),
    CurrentDependencyKind,
  )
  .meta({
    id: 'CurrentDependencyInfo',
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
  id: 'ExportServiceInterfaceParams',
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
export const ImageConfigRepr: z.ZodObject<{
  source: typeof ImageSource
  arch: z.ZodArray<z.ZodString>
  emulateMissing: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodBoolean>>,
    z.ZodNullable<z.ZodBoolean>
  >
  emulateMissingAs: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  nvidiaContainer: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  source: ImageSource,
  arch: z.array(z.string()),
  emulateMissing: wire(
    z.boolean().nullable().optional(),
    z.boolean().nullable(),
  ),
  emulateMissingAs: wire(
    z.string().nullable().optional(),
    z.string().nullable(),
  ),
  nvidiaContainer: wire(z.boolean().optional(), z.boolean()),
}).meta({
  id: 'ImageConfigRepr',
})
export const InitProgressRes: z.ZodObject<{
  progress: typeof FullProgress
  guid: typeof Guid
}> = /* @__PURE__ */ object({ progress: FullProgress, guid: Guid }).meta({
  id: 'InitProgressRes',
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
export const MountParams: z.ZodObject<{
  location: z.ZodString
  target: typeof MountTarget
}> = /* @__PURE__ */ object({ location: z.string(), target: MountTarget }).meta(
  {
    id: 'MountParams',
  },
)
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
export const PackagePlugin: z.ZodObject<{
  url: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof UrlPluginRegistration>>,
    z.ZodNullable<typeof UrlPluginRegistration>
  >
}> = /* @__PURE__ */ object({
  url: wire(
    UrlPluginRegistration.nullable().optional(),
    UrlPluginRegistration.nullable(),
  ),
}).meta({
  id: 'PackagePlugin',
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
export const ServerStatus: z.ZodObject<{
  backupProgress: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof FullProgress>>,
    z.ZodNullable<typeof FullProgress>
  >
  updateProgress: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof FullProgress>>,
    z.ZodNullable<typeof FullProgress>
  >
  shuttingDown: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  restarting: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  restart: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof RestartReason>>,
    z.ZodNullable<typeof RestartReason>
  >
}> = /* @__PURE__ */ object({
  backupProgress: wire(
    FullProgress.nullable().optional(),
    FullProgress.nullable(),
  ),
  updateProgress: wire(
    FullProgress.nullable().optional(),
    FullProgress.nullable(),
  ),
  shuttingDown: wire(z.boolean().optional(), z.boolean()),
  restarting: wire(z.boolean().optional(), z.boolean()),
  restart: wire(RestartReason.nullable().optional(), RestartReason.nullable()),
}).meta({
  id: 'ServerStatus',
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
export const SetDependenciesParams: z.ZodObject<{
  dependencies: z.ZodArray<typeof DependencyRequirement>
}> = /* @__PURE__ */ object({
  dependencies: z.array(DependencyRequirement),
}).meta({
  id: 'SetDependenciesParams',
})
export const SetHealth: z.ZodIntersection<
  z.ZodObject<{ id: typeof HealthCheckId }>,
  typeof NamedHealthCheckResult
> = /* @__PURE__ */ z
  .intersection(object({ id: HealthCheckId }), NamedHealthCheckResult)
  .meta({
    id: 'SetHealth',
  })
export const SetupProgress: z.ZodObject<{
  progress: typeof FullProgress
  guid: typeof Guid
}> = /* @__PURE__ */ object({ progress: FullProgress, guid: Guid }).meta({
  id: 'SetupProgress',
})
export const StatusInfo: z.ZodObject<{
  health: z.ZodRecord<z.ZodString, typeof NamedHealthCheckResult>
  error: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof ErrorData>>,
    z.ZodNullable<typeof ErrorData>
  >
  started: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  desired: typeof DesiredStatus
}> = /* @__PURE__ */ object({
  health: z.record(z.string(), NamedHealthCheckResult),
  error: wire(ErrorData.nullable().optional(), ErrorData.nullable()),
  started: wire(z.string().nullable().optional(), z.string().nullable()),
  desired: DesiredStatus,
}).meta({
  id: 'StatusInfo',
})
export const Task: z.ZodObject<{
  packageId: typeof PackageId
  actionId: typeof ActionId
  severity: z.ZodCodec<z.ZodOptional<typeof TaskSeverity>, typeof TaskSeverity>
  reason: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  when: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof TaskTrigger>>,
    z.ZodNullable<typeof TaskTrigger>
  >
  input: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof TaskInput>>,
    z.ZodNullable<typeof TaskInput>
  >
}> = /* @__PURE__ */ object({
  packageId: PackageId,
  actionId: ActionId,
  severity: wire(TaskSeverity.optional(), TaskSeverity),
  reason: wire(z.string().nullable().optional(), z.string().nullable()),
  when: wire(TaskTrigger.nullable().optional(), TaskTrigger.nullable()),
  input: wire(TaskInput.nullable().optional(), TaskInput.nullable()),
}).meta({
  id: 'Task',
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
  targetVersion: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
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
  targetVersion: wire(z.string().nullable().optional(), z.string().nullable()),
  progress: wire(z.boolean().optional(), z.boolean()),
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
  removeAction: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof ActionId>>,
    z.ZodNullable<typeof ActionId>
  >
  overflowActions: z.ZodArray<typeof ActionId>
}> = /* @__PURE__ */ object({
  hostnameInfo: PluginHostnameInfo,
  removeAction: wire(ActionId.nullable().optional(), ActionId.nullable()),
  overflowActions: z.array(ActionId),
}).meta({
  id: 'UrlPluginExportUrlParams',
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
export const AddSslOptions: z.ZodCodec<
  z.ZodObject<{
    preferredExternalPort: z.ZodInt
    /**
     * When `true`, the OS reverse proxy adds `X-Forwarded-Proto: https`
     * and `X-Forwarded-For: <client-ip>` to incoming HTTP requests before
     * forwarding them upstream. Setting this implies HTTP-aware proxying.
     */
    addXForwardedHeaders: z.ZodOptional<z.ZodBoolean>
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
    /**
     * Optional reverse-proxy auth gate. When set, the OS reverse proxy
     * will validate the `Authorization` header on incoming HTTP requests
     * against this configuration before forwarding them upstream.
     * Unauthenticated requests get `401 Unauthorized` with an appropriate
     * `WWW-Authenticate` challenge. For `Basic`, the authenticated
     * username is forwarded to the upstream service as `X-Forwarded-User`.
     * Setting this implies HTTP-aware proxying.
     */
    auth: z.ZodOptional<z.ZodNullable<typeof ProxyAuth>>
  }>,
  z.ZodObject<{
    preferredExternalPort: z.ZodInt
    /**
     * When `true`, the OS reverse proxy adds `X-Forwarded-Proto: https`
     * and `X-Forwarded-For: <client-ip>` to incoming HTTP requests before
     * forwarding them upstream. Setting this implies HTTP-aware proxying.
     */
    addXForwardedHeaders: z.ZodBoolean
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
    /**
     * Optional reverse-proxy auth gate. When set, the OS reverse proxy
     * will validate the `Authorization` header on incoming HTTP requests
     * against this configuration before forwarding them upstream.
     * Unauthenticated requests get `401 Unauthorized` with an appropriate
     * `WWW-Authenticate` challenge. For `Basic`, the authenticated
     * username is forwarded to the upstream service as `X-Forwarded-User`.
     * Setting this implies HTTP-aware proxying.
     */
    auth: z.ZodNullable<typeof ProxyAuth>
  }>
> = /* @__PURE__ */ wire(
  object({
    preferredExternalPort: z.int(),
    /**
     * When `true`, the OS reverse proxy adds `X-Forwarded-Proto: https`
     * and `X-Forwarded-For: <client-ip>` to incoming HTTP requests before
     * forwarding them upstream. Setting this implies HTTP-aware proxying.
     */
    addXForwardedHeaders: z.boolean().optional().meta({
      description:
        'When `true`, the OS reverse proxy adds `X-Forwarded-Proto: https`\nand `X-Forwarded-For: <client-ip>` to incoming HTTP requests before\nforwarding them upstream. Setting this implies HTTP-aware proxying.',
    }),
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
    /**
     * Optional reverse-proxy auth gate. When set, the OS reverse proxy
     * will validate the `Authorization` header on incoming HTTP requests
     * against this configuration before forwarding them upstream.
     * Unauthenticated requests get `401 Unauthorized` with an appropriate
     * `WWW-Authenticate` challenge. For `Basic`, the authenticated
     * username is forwarded to the upstream service as `X-Forwarded-User`.
     * Setting this implies HTTP-aware proxying.
     */
    auth: ProxyAuth.nullable().optional().meta({
      description:
        'Optional reverse-proxy auth gate. When set, the OS reverse proxy\nwill validate the `Authorization` header on incoming HTTP requests\nagainst this configuration before forwarding them upstream.\nUnauthenticated requests get `401 Unauthorized` with an appropriate\n`WWW-Authenticate` challenge. For `Basic`, the authenticated\nusername is forwarded to the upstream service as `X-Forwarded-User`.\nSetting this implies HTTP-aware proxying.',
    }),
  }),
  object({
    preferredExternalPort: z.int(),
    /**
     * When `true`, the OS reverse proxy adds `X-Forwarded-Proto: https`
     * and `X-Forwarded-For: <client-ip>` to incoming HTTP requests before
     * forwarding them upstream. Setting this implies HTTP-aware proxying.
     */
    addXForwardedHeaders: z.boolean().meta({
      description:
        'When `true`, the OS reverse proxy adds `X-Forwarded-Proto: https`\nand `X-Forwarded-For: <client-ip>` to incoming HTTP requests before\nforwarding them upstream. Setting this implies HTTP-aware proxying.',
    }),
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
    /**
     * Optional reverse-proxy auth gate. When set, the OS reverse proxy
     * will validate the `Authorization` header on incoming HTTP requests
     * against this configuration before forwarding them upstream.
     * Unauthenticated requests get `401 Unauthorized` with an appropriate
     * `WWW-Authenticate` challenge. For `Basic`, the authenticated
     * username is forwarded to the upstream service as `X-Forwarded-User`.
     * Setting this implies HTTP-aware proxying.
     */
    auth: ProxyAuth.nullable().meta({
      description:
        'Optional reverse-proxy auth gate. When set, the OS reverse proxy\nwill validate the `Authorization` header on incoming HTTP requests\nagainst this configuration before forwarding them upstream.\nUnauthenticated requests get `401 Unauthorized` with an appropriate\n`WWW-Authenticate` challenge. For `Basic`, the authenticated\nusername is forwarded to the upstream service as `X-Forwarded-User`.\nSetting this implies HTTP-aware proxying.',
    }),
  }),
).meta({
  id: 'AddSslOptions',
})
export const BackupTarget: z.ZodUnion<
  readonly [
    z.ZodIntersection<
      z.ZodObject<{ type: z.ZodLiteral<'disk'> }>,
      z.ZodIntersection<
        z.ZodObject<{
          vendor: z.ZodCodec<
            z.ZodOptional<z.ZodNullable<z.ZodString>>,
            z.ZodNullable<z.ZodString>
          >
          model: z.ZodCodec<
            z.ZodOptional<z.ZodNullable<z.ZodString>>,
            z.ZodNullable<z.ZodString>
          >
        }>,
        typeof PartitionInfo
      >
    >,
    z.ZodIntersection<
      z.ZodObject<{ type: z.ZodLiteral<'cifs'> }>,
      typeof CifsBackupTarget
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(
      object({ type: z.literal('disk') }),
      z.intersection(
        object({
          vendor: wire(z.string().nullable().optional(), z.string().nullable()),
          model: wire(z.string().nullable().optional(), z.string().nullable()),
        }),
        PartitionInfo,
      ),
    ),
    z.intersection(object({ type: z.literal('cifs') }), CifsBackupTarget),
  ])
  .meta({
    id: 'BackupTarget',
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
  >,
  z.ZodUnion<readonly [z.ZodNullable<typeof MetadataSrc>, typeof empty]>
> = /* @__PURE__ */ z
  .intersection(
    z.intersection(
      object({
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
    ),
    z.union([MetadataSrc.nullable(), empty]),
  )
  .meta({
    id: 'DepInfo',
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
export const MetricsFollowResponse: z.ZodObject<{
  guid: typeof Guid
  metrics: typeof Metrics
}> = /* @__PURE__ */ object({ guid: Guid, metrics: Metrics }).meta({
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
export const SetBackupProgress: z.ZodObject<{ progress: typeof Progress }> =
  /* @__PURE__ */ object({ progress: Progress }).meta({
    id: 'SetBackupProgress',
  })
export const SetInitProgress: z.ZodObject<{ progress: typeof Progress }> =
  /* @__PURE__ */ object({ progress: Progress }).meta({
    id: 'SetInitProgress',
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
export const TaskEntry: z.ZodObject<{
  task: typeof Task
  active: z.ZodBoolean
}> = /* @__PURE__ */ object({ task: Task, active: z.boolean() }).meta({
  id: 'TaskEntry',
})
export const BindOptions: z.ZodObject<{
  preferredExternalPort: z.ZodInt
  addSsl: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof AddSslOptions>>,
    z.ZodNullable<typeof AddSslOptions>
  >
  secure: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Security>>,
    z.ZodNullable<typeof Security>
  >
}> = /* @__PURE__ */ object({
  preferredExternalPort: z.int(),
  addSsl: wire(AddSslOptions.nullable().optional(), AddSslOptions.nullable()),
  secure: wire(Security.nullable().optional(), Security.nullable()),
}).meta({
  id: 'BindOptions',
})
export const CheckDependenciesResult: z.ZodObject<{
  packageId: typeof PackageId
  title: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  installedVersion: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Version>>,
    z.ZodNullable<typeof Version>
  >
  satisfies: z.ZodArray<typeof Version>
  isRunning: z.ZodBoolean
  tasks: z.ZodRecord<z.ZodString, typeof TaskEntry>
  healthChecks: z.ZodRecord<z.ZodString, typeof NamedHealthCheckResult>
}> = /* @__PURE__ */ object({
  packageId: PackageId,
  title: wire(z.string().nullable().optional(), z.string().nullable()),
  installedVersion: wire(Version.nullable().optional(), Version.nullable()),
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
 * Contiguous port-range binding (e.g. WebRTC/STUN/TURN RTP ranges).
 *
 * Keyed by `internal_start_port` in [`BindingRanges`]. The range covers
 * `internal_start_port..(internal_start_port + number_of_ports)` and is
 * forwarded through a single iptables rule per protocol per gateway,
 * preserving the destination port number.
 */
export const RangeBindInfo: z.ZodObject<{
  enabled: z.ZodBoolean
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
  interface: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof RangeServiceInterface>>,
    z.ZodNullable<typeof RangeServiceInterface>
  >
}> = /* @__PURE__ */ object({
  enabled: z.boolean(),
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
  interface: wire(
    RangeServiceInterface.nullable().optional(),
    RangeServiceInterface.nullable(),
  ).meta({
    description:
      'The single restricted `api` interface exported from this range, if any\n(`RangeOrigin.export`). Preserved across idempotent re-binds.',
  }),
}).meta({
  id: 'RangeBindInfo',
  description:
    'Contiguous port-range binding (e.g. WebRTC/STUN/TURN RTP ranges).\n\nKeyed by `internal_start_port` in [`BindingRanges`]. The range covers\n`internal_start_port..(internal_start_port + number_of_ports)` and is\nforwarded through a single iptables rule per protocol per gateway,\npreserving the destination port number.',
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
export const BindParams: z.ZodIntersection<
  z.ZodObject<{ id: typeof HostId; internalPort: z.ZodInt }>,
  typeof BindOptions
> = /* @__PURE__ */ z
  .intersection(object({ id: HostId, internalPort: z.int() }), BindOptions)
  .meta({
    id: 'BindParams',
  })
export const BindingRanges: z.ZodRecord<z.ZodString, typeof RangeBindInfo> =
  /* @__PURE__ */ z.record(z.string(), RangeBindInfo).meta({
    id: 'BindingRanges',
  })
export const Manifest: z.ZodIntersection<
  z.ZodObject<{
    id: typeof PackageId
    version: typeof Version
    canMigrateTo: z.ZodString
    canMigrateFrom: z.ZodString
    images: z.ZodRecord<z.ZodString, typeof ImageConfig>
    volumes: z.ZodArray<typeof VolumeId>
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
export const Bindings: z.ZodRecord<z.ZodString, typeof BindInfo> =
  /* @__PURE__ */ z.record(z.string(), BindInfo).meta({
    id: 'Bindings',
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
  portForwards: z.ZodCodec<
    z.ZodOptional<z.ZodArray<typeof PortForward>>,
    z.ZodArray<typeof PortForward>
  >
}> = /* @__PURE__ */ object({
  bindings: Bindings,
  bindingRanges: wire(BindingRanges.optional(), BindingRanges),
  publicDomains: z.record(z.string(), PublicDomainConfig),
  privateDomains: z.record(z.string(), z.array(GatewayId)),
  /**
   * COMPUTED: port forwarding rules needed on gateways for public addresses to work.
   */
  portForwards: wire(
    z.array(PortForward).optional(),
    z.array(PortForward),
  ).meta({
    description:
      'COMPUTED: port forwarding rules needed on gateways for public addresses to work.',
  }),
}).meta({
  id: 'Host',
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
export const Hosts: z.ZodRecord<z.ZodString, typeof Host> = /* @__PURE__ */ z
  .record(z.string(), Host)
  .meta({
    id: 'Hosts',
  })
export const NetworkInfo: z.ZodObject<{
  wifi: typeof WifiInfo
  host: typeof Host
  gateways: z.ZodCodec<
    z.ZodOptional<z.ZodRecord<z.ZodString, typeof NetworkInterfaceInfo>>,
    z.ZodRecord<z.ZodString, typeof NetworkInterfaceInfo>
  >
  acme: z.ZodCodec<
    z.ZodOptional<z.ZodRecord<z.ZodString, typeof AcmeSettings>>,
    z.ZodRecord<z.ZodString, typeof AcmeSettings>
  >
  defaultOutbound: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof GatewayId>>,
    z.ZodNullable<typeof GatewayId>
  >
  outboundDns: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<z.ZodString>>>,
    z.ZodNullable<z.ZodArray<z.ZodString>>
  >
  passthroughs: z.ZodCodec<
    z.ZodOptional<z.ZodArray<typeof PassthroughInfo>>,
    z.ZodArray<typeof PassthroughInfo>
  >
}> = /* @__PURE__ */ object({
  wifi: WifiInfo,
  host: Host,
  gateways: wire(
    z.record(z.string(), NetworkInterfaceInfo).optional(),
    z.record(z.string(), NetworkInterfaceInfo),
  ),
  acme: wire(
    z.record(z.string(), AcmeSettings).optional(),
    z.record(z.string(), AcmeSettings),
  ),
  defaultOutbound: wire(GatewayId.nullable().optional(), GatewayId.nullable()),
  outboundDns: wire(
    z.array(z.string()).nullable().optional(),
    z.array(z.string()).nullable(),
  ),
  passthroughs: wire(
    z.array(PassthroughInfo).optional(),
    z.array(PassthroughInfo),
  ),
}).meta({
  id: 'NetworkInfo',
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
export const PackageDataEntry: z.ZodObject<{
  stateInfo: typeof PackageState
  s9pk: z.ZodString
  statusInfo: typeof StatusInfo
  registry: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  developerKey: typeof literal_0
  lastBackup: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  currentDependencies: typeof CurrentDependencies
  actions: z.ZodRecord<z.ZodString, typeof ActionMetadata>
  tasks: z.ZodRecord<z.ZodString, typeof TaskEntry>
  hosts: typeof Hosts
  storeExposedDependents: z.ZodArray<z.ZodString>
  outboundGateway: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof GatewayId>>,
    z.ZodNullable<typeof GatewayId>
  >
  outboundDns: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodArray<z.ZodString>>>,
    z.ZodNullable<z.ZodArray<z.ZodString>>
  >
  plugin: z.ZodCodec<z.ZodOptional<typeof PackagePlugin>, typeof PackagePlugin>
}> = /* @__PURE__ */ object({
  stateInfo: PackageState,
  s9pk: z.string(),
  statusInfo: StatusInfo,
  registry: wire(z.string().nullable().optional(), z.string().nullable()),
  developerKey: literal_0.meta({
    description:
      'Standard Base64 DER with padding. Deserialization also accepts legacy PEM armor.',
  }),
  lastBackup: wire(z.string().nullable().optional(), z.string().nullable()),
  currentDependencies: CurrentDependencies,
  actions: z.record(z.string(), ActionMetadata),
  tasks: z.record(z.string(), TaskEntry),
  hosts: Hosts,
  storeExposedDependents: z.array(z.string()),
  outboundGateway: wire(GatewayId.nullable().optional(), GatewayId.nullable()),
  outboundDns: wire(
    z.array(z.string()).nullable().optional(),
    z.array(z.string()).nullable(),
  ),
  plugin: wire(PackagePlugin.optional(), PackagePlugin),
}).meta({
  id: 'PackageDataEntry',
})
export const ServerInfo: z.ZodObject<{
  id: z.ZodString
  hostname: z.ZodString
  version: z.ZodString
  packageVersionCompat: z.ZodString
  postInitMigrationTodos: typeof literal_1
  latestMigrationRevision: z.ZodCodec<z.ZodOptional<z.ZodInt>, z.ZodInt>
  lastBackup: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  network: typeof NetworkInfo
  statusInfo: z.ZodCodec<
    z.ZodOptional<typeof ServerStatus>,
    typeof ServerStatus
  >
  unreadNotificationCount: z.ZodInt
  trustedOsUpdateSigners: z.ZodCodec<
    z.ZodOptional<z.ZodRecord<z.ZodString, typeof TrustedOsUpdateSigner>>,
    z.ZodRecord<z.ZodString, typeof TrustedOsUpdateSigner>
  >
  pubkey: z.ZodString
  caFingerprint: z.ZodString
  ntpSynced: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  zram: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
  governor: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Governor>>,
    z.ZodNullable<typeof Governor>
  >
  epp: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof Epp>>,
    z.ZodNullable<typeof Epp>
  >
  smtp: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof SmtpValue>>,
    z.ZodNullable<typeof SmtpValue>
  >
  echoipUrls: z.ZodCodec<
    z.ZodOptional<z.ZodArray<z.ZodString>>,
    z.ZodArray<z.ZodString>
  >
  ram: z.ZodInt
  devices: z.ZodArray<typeof LshwDevice>
  kiosk: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodBoolean>>,
    z.ZodNullable<z.ZodBoolean>
  >
  language: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
  keyboard: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof KeyboardOptions>>,
    z.ZodNullable<typeof KeyboardOptions>
  >
}> = /* @__PURE__ */ object({
  id: z.string(),
  hostname: z.string(),
  version: z.string(),
  packageVersionCompat: z.string(),
  postInitMigrationTodos: literal_1,
  latestMigrationRevision: wire(z.int().optional(), z.int()),
  lastBackup: wire(z.string().nullable().optional(), z.string().nullable()),
  network: NetworkInfo,
  statusInfo: wire(ServerStatus.optional(), ServerStatus),
  unreadNotificationCount: z.int(),
  trustedOsUpdateSigners: wire(
    z.record(z.string(), TrustedOsUpdateSigner).optional(),
    z.record(z.string(), TrustedOsUpdateSigner),
  ),
  pubkey: z.string(),
  caFingerprint: z.string(),
  ntpSynced: wire(z.boolean().optional(), z.boolean()),
  zram: wire(z.boolean().optional(), z.boolean()),
  governor: wire(Governor.nullable().optional(), Governor.nullable()),
  epp: wire(Epp.nullable().optional(), Epp.nullable()),
  smtp: wire(SmtpValue.nullable().optional(), SmtpValue.nullable()),
  echoipUrls: wire(z.array(z.string()).optional(), z.array(z.string())),
  ram: z.int(),
  devices: z.array(LshwDevice),
  kiosk: wire(z.boolean().nullable().optional(), z.boolean().nullable()),
  language: wire(z.string().nullable().optional(), z.string().nullable()),
  keyboard: wire(
    KeyboardOptions.nullable().optional(),
    KeyboardOptions.nullable(),
  ),
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
  ui: z.ZodUnknown
}> = /* @__PURE__ */ object({
  serverInfo: ServerInfo,
  packageData: AllPackageData,
  ui: z.unknown(),
}).meta({
  id: 'Public',
})
