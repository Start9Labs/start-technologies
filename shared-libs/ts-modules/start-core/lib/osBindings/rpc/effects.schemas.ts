import { z } from 'zod'
const wire = <A extends z.ZodType, B extends z.ZodType>(input: A, output: B) =>
  z.codec(input, output, { decode: v => v as never, encode: v => v as never })
const object = <Shape extends z.core.$ZodLooseShape>(shape: Shape) =>
  z.object(shape).meta({ additionalProperties: true })
const empty: z.ZodType<{}, {}> = /* @__PURE__ */ object({})
const literal_0 = /* @__PURE__ */ z.string()
const literal_1 = /* @__PURE__ */ z.union([
  z.object({ only: z.array(z.string()) }),
  z.object({ except: z.array(z.string()) }),
])
const literal_2 = /* @__PURE__ */ z.array(z.record(z.string(), z.unknown()))
const literal_3 = /* @__PURE__ */ z.record(z.string(), z.unknown())
const literal_4 = /* @__PURE__ */ z.record(z.string(), z.unknown()).nullable()
const literal_5 = /* @__PURE__ */ z.union([z.string(), z.array(z.number())])
const literal_6 = /* @__PURE__ */ z.union([
  z.object({ only: z.array(z.number()) }),
  z.object({ except: z.array(z.number()) }),
])
const literal_7 = /* @__PURE__ */ z.number()
const literal_8 = /* @__PURE__ */ z.union([
  z.string(),
  z.record(z.string(), z.string()),
])
const literal_9 = /* @__PURE__ */ z.enum(['processor', 'display'])
export const AcmeProvider: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'AcmeProvider',
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
export const BasicCredential: z.ZodObject<{
  username: z.ZodString
  password: z.ZodString
}> = /* @__PURE__ */ object({
  username: z.string(),
  password: z.string(),
}).meta({
  id: 'BasicCredential',
})
export const BuildArg: z.ZodUnion<
  readonly [z.ZodString, z.ZodObject<{ env: z.ZodString }>]
> = /* @__PURE__ */ z.union([z.string(), object({ env: z.string() })]).meta({
  id: 'BuildArg',
})
export const CallbackId: typeof literal_7 = /* @__PURE__ */ literal_7.meta({
  id: 'CallbackId',
})
export const ClearCallbacksParams: typeof literal_6 =
  /* @__PURE__ */ literal_6.meta({
    id: 'ClearCallbacksParams',
  })
export const ClearTasksParams: typeof literal_1 =
  /* @__PURE__ */ literal_1.meta({
    id: 'ClearTasksParams',
  })
export const DeviceFilter: z.ZodObject<{
  description: z.ZodString
  class: typeof literal_9
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
  class: literal_9,
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
export const EchoParams: z.ZodObject<{ message: z.ZodString }> =
  /* @__PURE__ */ object({ message: z.string() }).meta({
    id: 'EchoParams',
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
export const GatewayId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'GatewayId',
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
export const LocaleString: typeof literal_8 = /* @__PURE__ */ literal_8.meta({
  id: 'LocaleString',
})
export const MaybeUtf8String: typeof literal_5 = /* @__PURE__ */ literal_5.meta(
  {
    id: 'MaybeUtf8String',
  },
)
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
export const NotificationLevel: z.ZodEnum<{
  success: 'success'
  info: 'info'
  warning: 'warning'
  error: 'error'
}> = /* @__PURE__ */ z.enum(['success', 'info', 'warning', 'error']).meta({
  id: 'NotificationLevel',
})
export const PackageId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'PackageId',
})
export const PathOrUrl: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'PathOrUrl',
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
export const ReplayId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'ReplayId',
})
export const Security: z.ZodObject<{ ssl: z.ZodBoolean }> =
  /* @__PURE__ */ object({ ssl: z.boolean() }).meta({
    id: 'Security',
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
export const SetMainStatusStatus: z.ZodEnum<{
  running: 'running'
  stopped: 'stopped'
}> = /* @__PURE__ */ z.enum(['running', 'stopped']).meta({
  id: 'SetMainStatusStatus',
})
export const SmtpSecurity: z.ZodEnum<{ starttls: 'starttls'; tls: 'tls' }> =
  /* @__PURE__ */ z.enum(['starttls', 'tls']).meta({
    id: 'SmtpSecurity',
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
export const Version: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'Version',
})
export const VolumeId: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'VolumeId',
})
export const ActionInput: z.ZodObject<{
  eventId: z.ZodCodec<z.ZodOptional<typeof Guid>, typeof Guid>
  spec: typeof literal_3
  value: z.ZodCodec<z.ZodOptional<typeof literal_4>, typeof literal_4>
}> = /* @__PURE__ */ object({
  eventId: wire(Guid.optional(), Guid),
  spec: literal_3,
  value: wire(literal_4.optional(), literal_4),
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
export const GetActionInputParams: z.ZodObject<{
  packageId: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<typeof PackageId>>,
    z.ZodNullable<typeof PackageId>
  >
  actionId: typeof ActionId
  prefill: z.ZodCodec<z.ZodOptional<typeof literal_4>, typeof literal_4>
}> = /* @__PURE__ */ object({
  packageId: wire(PackageId.nullable().optional(), PackageId.nullable()),
  actionId: ActionId,
  prefill: wire(literal_4.optional(), literal_4),
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
export const Metadata: z.ZodObject<{
  title: typeof LocaleString
  icon: typeof PathOrUrl
}> = /* @__PURE__ */ object({ title: LocaleString, icon: PathOrUrl }).meta({
  id: 'Metadata',
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
export const SetMainStatus: z.ZodObject<{
  status: typeof SetMainStatusStatus
}> = /* @__PURE__ */ object({ status: SetMainStatusStatus }).meta({
  id: 'SetMainStatus',
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
export const TaskInput: z.ZodCodec<
  typeof TaskInputRepr,
  z.ZodIntersection<
    z.ZodObject<{ kind: z.ZodLiteral<'partial'> }>,
    z.ZodObject<{ accept: typeof literal_2; set: typeof literal_3 }>
  >
> = /* @__PURE__ */ wire(
  TaskInputRepr,
  z.intersection(
    object({ kind: z.literal('partial') }),
    object({ accept: literal_2, set: literal_3 }),
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
export const UrlPluginRegisterParams: z.ZodObject<{
  tableAction: typeof ActionId
}> = /* @__PURE__ */ object({ tableAction: ActionId }).meta({
  id: 'UrlPluginRegisterParams',
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
export const MountParams: z.ZodObject<{
  location: z.ZodString
  target: typeof MountTarget
}> = /* @__PURE__ */ object({ location: z.string(), target: MountTarget }).meta(
  {
    id: 'MountParams',
  },
)
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
export const CreateTaskParams: z.ZodIntersection<
  z.ZodObject<{ replayId: typeof ReplayId }>,
  typeof Task
> = /* @__PURE__ */ z.intersection(object({ replayId: ReplayId }), Task).meta({
  id: 'CreateTaskParams',
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
export const SetBackupProgress: z.ZodObject<{ progress: typeof Progress }> =
  /* @__PURE__ */ object({ progress: Progress }).meta({
    id: 'SetBackupProgress',
  })
export const SetInitProgress: z.ZodObject<{ progress: typeof Progress }> =
  /* @__PURE__ */ object({ progress: Progress }).meta({
    id: 'SetInitProgress',
  })
export const TaskEntry: z.ZodObject<{
  task: typeof Task
  active: z.ZodBoolean
}> = /* @__PURE__ */ object({ task: Task, active: z.boolean() }).meta({
  id: 'TaskEntry',
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
export const BindingRanges: z.ZodRecord<z.ZodString, typeof RangeBindInfo> =
  /* @__PURE__ */ z.record(z.string(), RangeBindInfo).meta({
    id: 'BindingRanges',
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
export const Bindings: z.ZodRecord<z.ZodString, typeof BindInfo> =
  /* @__PURE__ */ z.record(z.string(), BindInfo).meta({
    id: 'Bindings',
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
export const Api = {
  _PARAMS: /* @__PURE__ */ empty,
  _CHILDREN: {
    action: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        clear: {
          _PARAMS: /* @__PURE__ */ ClearActionsParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'clear-tasks': {
          _PARAMS: /* @__PURE__ */ ClearTasksParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        'create-task': {
          _PARAMS: /* @__PURE__ */ CreateTaskParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
        export: {
          _PARAMS: /* @__PURE__ */ ExportActionParams,
          _RETURN: /* @__PURE__ */ z.null(),
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
            EffectsRunActionParams,
          ),
          _RETURN: /* @__PURE__ */ ActionResult.nullable(),
        },
      },
    },
    bind: {
      _PARAMS: /* @__PURE__ */ BindParams,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    'bind-range': {
      _PARAMS: /* @__PURE__ */ BindRangeParams,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    'check-dependencies': {
      _PARAMS: /* @__PURE__ */ CheckDependenciesParam,
      _RETURN: /* @__PURE__ */ z.array(CheckDependenciesResult),
    },
    'clear-bindings': {
      _PARAMS: /* @__PURE__ */ ClearBindingsParams,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    'clear-callbacks': {
      _PARAMS: /* @__PURE__ */ ClearCallbacksParams,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    'clear-service-interfaces': {
      _PARAMS: /* @__PURE__ */ ClearServiceInterfacesParams,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    echo: {
      _PARAMS: /* @__PURE__ */ EchoParams,
      _RETURN: /* @__PURE__ */ z.string(),
    },
    'export-range-service-interface': {
      _PARAMS: /* @__PURE__ */ ExportRangeServiceInterfaceParams,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    'export-service-interface': {
      _PARAMS: /* @__PURE__ */ ExportServiceInterfaceParams,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    'get-container-ip': {
      _PARAMS: /* @__PURE__ */ GetContainerIpParams,
      _RETURN: /* @__PURE__ */ z.string().nullable(),
    },
    'get-data-version': {
      _PARAMS: /* @__PURE__ */ empty,
      _RETURN: /* @__PURE__ */ z.string().nullable(),
    },
    'get-dependencies': {
      _PARAMS: /* @__PURE__ */ empty,
      _RETURN: /* @__PURE__ */ z.array(DependencyRequirement),
    },
    'get-host-info': {
      _PARAMS: /* @__PURE__ */ GetHostInfoParams,
      _RETURN: /* @__PURE__ */ Host.nullable(),
    },
    'get-installed-packages': {
      _PARAMS: /* @__PURE__ */ empty,
      _RETURN: /* @__PURE__ */ z.array(PackageId),
    },
    'get-os-ip': {
      _PARAMS: /* @__PURE__ */ empty,
      _RETURN: /* @__PURE__ */ z.string(),
    },
    'get-outbound-gateway': {
      _PARAMS: /* @__PURE__ */ GetOutboundGatewayParams,
      _RETURN: /* @__PURE__ */ GatewayId,
    },
    'get-service-interface': {
      _PARAMS: /* @__PURE__ */ GetServiceInterfaceParams,
      _RETURN: /* @__PURE__ */ ServiceInterface.nullable(),
    },
    'get-service-manifest': {
      _PARAMS: /* @__PURE__ */ GetServiceManifestParams,
      _RETURN: /* @__PURE__ */ Manifest.nullable(),
    },
    'get-service-port-forward': {
      _PARAMS: /* @__PURE__ */ GetServicePortForwardParams,
      _RETURN: /* @__PURE__ */ NetInfo.nullable(),
    },
    'get-ssl-certificate': {
      _PARAMS: /* @__PURE__ */ GetSslCertificateParams,
      _RETURN: /* @__PURE__ */ z.array(z.string()),
    },
    'get-ssl-key': {
      _PARAMS: /* @__PURE__ */ GetSslKeyParams,
      _RETURN: /* @__PURE__ */ literal_0,
    },
    'get-status': {
      _PARAMS: /* @__PURE__ */ GetStatusParams,
      _RETURN: /* @__PURE__ */ StatusInfo.nullable(),
    },
    'get-system-smtp': {
      _PARAMS: /* @__PURE__ */ GetSystemSmtpParams,
      _RETURN: /* @__PURE__ */ SmtpValue.nullable(),
    },
    'git-info': {
      _PARAMS: /* @__PURE__ */ empty,
      _RETURN: /* @__PURE__ */ z.string(),
    },
    'list-service-interfaces': {
      _PARAMS: /* @__PURE__ */ ListServiceInterfacesParams,
      _RETURN: /* @__PURE__ */ z.record(z.string(), ServiceInterface),
    },
    mount: {
      _PARAMS: /* @__PURE__ */ MountParams,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    notification: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        create: {
          _PARAMS: /* @__PURE__ */ CreateNotificationParams,
          _RETURN: /* @__PURE__ */ z.null(),
        },
      },
    },
    plugin: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        url: {
          _PARAMS: /* @__PURE__ */ empty,
          _CHILDREN: {
            'clear-urls': {
              _PARAMS: /* @__PURE__ */ UrlPluginClearUrlsParams,
              _RETURN: /* @__PURE__ */ z.null(),
            },
            'export-url': {
              _PARAMS: /* @__PURE__ */ UrlPluginExportUrlParams,
              _RETURN: /* @__PURE__ */ z.null(),
            },
            register: {
              _PARAMS: /* @__PURE__ */ UrlPluginRegisterParams,
              _RETURN: /* @__PURE__ */ z.null(),
            },
          },
        },
      },
    },
    rebuild: {
      _PARAMS: /* @__PURE__ */ empty,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    restart: {
      _PARAMS: /* @__PURE__ */ empty,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    'retire-binding': {
      _PARAMS: /* @__PURE__ */ RetireBindingParams,
      _RETURN: /* @__PURE__ */ z.boolean(),
    },
    'retire-host': {
      _PARAMS: /* @__PURE__ */ RetireHostParams,
      _RETURN: /* @__PURE__ */ z.boolean(),
    },
    'set-backup-progress': {
      _PARAMS: /* @__PURE__ */ SetBackupProgress,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    'set-data-version': {
      _PARAMS: /* @__PURE__ */ SetDataVersionParams,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    'set-dependencies': {
      _PARAMS: /* @__PURE__ */ SetDependenciesParams,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    'set-health': {
      _PARAMS: /* @__PURE__ */ SetHealth,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    'set-init-progress': {
      _PARAMS: /* @__PURE__ */ SetInitProgress,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    'set-main-status': {
      _PARAMS: /* @__PURE__ */ SetMainStatus,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    shutdown: {
      _PARAMS: /* @__PURE__ */ empty,
      _RETURN: /* @__PURE__ */ z.null(),
    },
    subcontainer: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        'create-fs': {
          _PARAMS: /* @__PURE__ */ CreateSubcontainerFsParams,
          _RETURN: /* @__PURE__ */ z.tuple([z.string(), Guid]),
        },
        'destroy-fs': {
          _PARAMS: /* @__PURE__ */ DestroySubcontainerFsParams,
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
