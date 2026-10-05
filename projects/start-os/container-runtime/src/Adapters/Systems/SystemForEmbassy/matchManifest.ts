import { z } from '@start9labs/start-sdk'
import { matchVolume } from './matchVolume'
import { matchDockerProcedure } from '../../../Models/DockerProcedure'

const matchJsProcedure = z.looseObject({
  type: z.literal('script'),
  args: z.array(z.unknown()).nullable().optional().default([]),
})

const matchProcedure = z.union([matchDockerProcedure, matchJsProcedure])
export type Procedure = z.infer<typeof matchProcedure>

const healthCheckFields = {
  name: z.string(),
  'success-message': z.string().nullable().optional(),
}

const matchAction = z.looseObject({
  name: z.string(),
  description: z.string(),
  warning: z.string().nullable().optional(),
  implementation: matchProcedure,
  'allowed-statuses': z.array(z.enum(['running', 'stopped'])),
  'input-spec': z.unknown().nullable().optional(),
})
export const matchManifest = z.looseObject({
  id: z.string(),
  title: z.string(),
  version: z.string(),
  main: matchDockerProcedure,
  assets: z
    .looseObject({
      assets: z.string().nullable().optional(),
      scripts: z.string().nullable().optional(),
    })
    .nullable()
    .optional(),
  'health-checks': z.record(
    z.string(),
    z.union([
      matchDockerProcedure.extend(healthCheckFields),
      matchJsProcedure.extend(healthCheckFields),
    ]),
  ),
  config: z
    .looseObject({
      get: matchProcedure,
      set: matchProcedure,
    })
    .nullable()
    .optional(),
  properties: matchProcedure.nullable().optional(),
  volumes: z.record(z.string(), matchVolume),
  interfaces: z.record(
    z.string(),
    z.looseObject({
      name: z.string(),
      description: z.string(),
      'tor-config': z
        .looseObject({
          'port-mapping': z.record(z.string(), z.string()),
        })
        .nullable()
        .optional(),
      'lan-config': z
        .record(
          z.string(),
          z.looseObject({
            ssl: z.boolean(),
            internal: z.number(),
          }),
        )
        .nullable()
        .optional(),
      ui: z.boolean(),
      protocols: z.array(z.string()),
    }),
  ),
  backup: z.looseObject({
    create: matchProcedure,
    restore: matchProcedure,
  }),
  migrations: z
    .looseObject({
      to: z.record(z.string(), matchProcedure),
      from: z.record(z.string(), matchProcedure),
    })
    .nullable()
    .optional(),
  dependencies: z.record(
    z.string(),
    z
      .looseObject({
        version: z.string(),
        requirement: z.union([
          z.looseObject({
            type: z.literal('opt-in'),
            how: z.string(),
          }),
          z.looseObject({
            type: z.literal('opt-out'),
            how: z.string(),
          }),
          z.looseObject({
            type: z.literal('required'),
          }),
        ]),
        description: z.string().nullable().optional(),
        config: z
          .looseObject({
            check: matchProcedure,
            'auto-configure': matchProcedure,
          })
          .nullable()
          .optional(),
      })
      .nullable()
      .optional(),
  ),

  actions: z.record(z.string(), matchAction),
})
export type Manifest = z.infer<typeof matchManifest>
