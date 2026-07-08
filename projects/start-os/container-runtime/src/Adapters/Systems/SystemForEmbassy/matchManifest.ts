import { z } from '@start9labs/start-sdk'
import { matchVolume } from './matchVolume'
import { matchDockerProcedure } from '../../../Models/DockerProcedure'

const matchJsProcedure = z.object({
  type: z.literal('script'),
  args: z.array(z.unknown()).nullable().optional().default([]),
})

const matchProcedure = z.union([matchDockerProcedure, matchJsProcedure])
export type Procedure = z.infer<typeof matchProcedure>

const healthCheckFields = {
  name: z.string(),
  'success-message': z.string().nullable().optional(),
}

const matchAction = z.object({
  name: z.string(),
  description: z.string(),
  warning: z.string().nullable().optional(),
  implementation: matchProcedure,
  'allowed-statuses': z.array(z.enum(['running', 'stopped'])),
  'input-spec': z.unknown().nullable().optional(),
})
export const matchManifest = z.object({
  id: z.string(),
  title: z.string(),
  version: z.string(),
  main: matchDockerProcedure,
  assets: z
    .object({
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
    .object({
      get: matchProcedure,
      set: matchProcedure,
    })
    .nullable()
    .optional(),
  properties: matchProcedure.nullable().optional(),
  volumes: z.record(z.string(), matchVolume),
  interfaces: z.record(
    z.string(),
    z.object({
      name: z.string(),
      description: z.string(),
      'tor-config': z
        .object({
          'port-mapping': z.record(z.string(), z.string()),
        })
        .nullable()
        .optional(),
      'lan-config': z
        .record(
          z.string(),
          z.object({
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
  backup: z.object({
    create: matchProcedure,
    restore: matchProcedure,
  }),
  migrations: z
    .object({
      to: z.record(z.string(), matchProcedure),
      from: z.record(z.string(), matchProcedure),
    })
    .nullable()
    .optional(),
  dependencies: z.record(
    z.string(),
    z
      .object({
        version: z.string(),
        requirement: z.union([
          z.object({
            type: z.literal('opt-in'),
            how: z.string(),
          }),
          z.object({
            type: z.literal('opt-out'),
            how: z.string(),
          }),
          z.object({
            type: z.literal('required'),
          }),
        ]),
        description: z.string().nullable().optional(),
        config: z
          .object({
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
