import { z } from '@start9labs/start-sdk'

const matchDataVolume = z.looseObject({
  type: z.literal('data'),
  readonly: z.boolean().optional(),
})
const matchAssetVolume = z.looseObject({
  type: z.literal('assets'),
})
const matchPointerVolume = z.looseObject({
  type: z.literal('pointer'),
  'package-id': z.string(),
  'volume-id': z.string(),
  path: z.string(),
  readonly: z.boolean(),
})
const matchCertificateVolume = z.looseObject({
  type: z.literal('certificate'),
  'interface-id': z.string(),
})
const matchBackupVolume = z.looseObject({
  type: z.literal('backup'),
  readonly: z.boolean(),
})
export const matchVolume = z.union([
  matchDataVolume,
  matchAssetVolume,
  matchPointerVolume,
  matchCertificateVolume,
  matchBackupVolume,
])
export type Volume = z.infer<typeof matchVolume>
