import { z } from '@start9labs/start-sdk'
import { matchDuration } from './Duration'

export const matchDockerProcedure = z.object({
  type: z.literal('docker'),
  image: z.string(),
  system: z.boolean().optional(),
  entrypoint: z.string(),
  args: z.array(z.string()).default([]),
  mounts: z.record(z.string(), z.string()).optional(),
  'io-format': z
    .enum(['json', 'json-pretty', 'yaml', 'cbor', 'toml', 'toml-pretty'])
    .nullable()
    .optional(),
  'sigterm-timeout': z.union([z.number(), matchDuration]).catch(30),
  inject: z.boolean().default(false),
})

export type DockerProcedure = z.infer<typeof matchDockerProcedure>
