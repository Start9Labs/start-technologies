import { z } from 'zod'
const wire = <A extends z.ZodType, B extends z.ZodType>(input: A, output: B) =>
  z.codec(input, output, { decode: v => v as never, encode: v => v as never })
const object = <Shape extends z.core.$ZodLooseShape>(shape: Shape) =>
  z.object(shape).meta({ additionalProperties: true })
const empty: z.ZodType<{}, {}> = /* @__PURE__ */ object({})
const literal_0 = /* @__PURE__ */ z.union([z.number(), z.string()]).nullable()
const literal_1 = /* @__PURE__ */ z.string()
const literal_2 = /* @__PURE__ */ z.string().nullable()
export const ApiState: z.ZodEnum<{
  error: 'error'
  initializing: 'initializing'
  running: 'running'
}> = /* @__PURE__ */ z.enum(['error', 'initializing', 'running']).meta({
  id: 'ApiState',
})
export const Guid: typeof literal_1 = /* @__PURE__ */ literal_1.meta({
  id: 'Guid',
})
export const HostId: typeof literal_1 = /* @__PURE__ */ literal_1.meta({
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
export const LogEntry: z.ZodObject<{
  timestamp: typeof literal_1
  message: z.ZodString
  bootId: z.ZodString
}> = /* @__PURE__ */ object({
  timestamp: literal_1,
  message: z.string(),
  bootId: z.string(),
}).meta({
  id: 'LogEntry',
})
export const PackageId: typeof literal_1 = /* @__PURE__ */ literal_1.meta({
  id: 'PackageId',
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
  ipv4: z.ZodCodec<z.ZodOptional<typeof literal_2>, typeof literal_2>
  ipv6: z.ZodCodec<z.ZodOptional<typeof literal_2>, typeof literal_2>
}> = /* @__PURE__ */ object({
  ipv4: wire(literal_2.optional(), literal_2),
  ipv6: wire(literal_2.optional(), literal_2),
}).meta({
  id: 'QueryDnsRes',
  description:
    "What a public domain currently resolves to, per address family. A public\ndomain is DualStack: the operator points an `A` record at the gateway's WAN\nIPv4 and an `AAAA` at the box's IPv6 GUA. Either may be absent.",
})
export const UpdateSystemParams: z.ZodObject<{
  registry: z.ZodString
  targetVersion: z.ZodCodec<z.ZodOptional<typeof literal_2>, typeof literal_2>
  progress: z.ZodCodec<z.ZodOptional<z.ZodBoolean>, z.ZodBoolean>
}> = /* @__PURE__ */ object({
  registry: z.string(),
  targetVersion: wire(literal_2.optional(), literal_2),
  progress: wire(z.boolean().optional(), z.boolean()),
}).meta({
  id: 'UpdateSystemParams',
})
export const UpdateSystemRes: z.ZodObject<{
  target: z.ZodCodec<z.ZodOptional<typeof literal_2>, typeof literal_2>
  progress: z.ZodCodec<z.ZodOptional<typeof literal_2>, typeof literal_2>
}> = /* @__PURE__ */ object({
  target: wire(literal_2.optional(), literal_2),
  progress: wire(literal_2.optional(), literal_2),
}).meta({
  id: 'UpdateSystemRes',
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
        error: {
          _PARAMS: /* @__PURE__ */ empty,
          _RETURN: /* @__PURE__ */ object({
            code: z.number(),
            message: z.string(),
            data: z.unknown().optional(),
          }),
        },
        'kernel-logs': {
          _PARAMS: /* @__PURE__ */ z.intersection(
            object({
              limit: wire(z.int().nullable().optional(), z.int().nullable()),
              cursor: wire(
                z.string().nullable().optional(),
                z.string().nullable(),
              ),
              boot: wire(literal_0.optional(), literal_0),
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
                  boot: wire(literal_0.optional(), literal_0),
                  before: wire(z.boolean().optional(), z.boolean()),
                }),
                empty,
              ),
              _RETURN: /* @__PURE__ */ LogFollowResponse,
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
              boot: wire(literal_0.optional(), literal_0),
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
                  boot: wire(literal_0.optional(), literal_0),
                  before: wire(z.boolean().optional(), z.boolean()),
                }),
                empty,
              ),
              _RETURN: /* @__PURE__ */ LogFollowResponse,
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
        update: {
          _PARAMS: /* @__PURE__ */ UpdateSystemParams,
          _RETURN: /* @__PURE__ */ UpdateSystemRes,
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
          },
        },
        forward: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
        gateway: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
        ssl: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
        tunnel: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
        vhost: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
      },
    },
    notification: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
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
      },
    },
    setup: {
      _PARAMS: /* @__PURE__ */ empty,
      _CHILDREN: {
        cifs: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
        disk: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
      },
    },
    ssh: { _PARAMS: /* @__PURE__ */ empty, _CHILDREN: {} },
    state: {
      _PARAMS: /* @__PURE__ */ empty,
      _RETURN: /* @__PURE__ */ ApiState,
    },
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
