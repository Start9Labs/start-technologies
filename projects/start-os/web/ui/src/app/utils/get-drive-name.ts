import { T } from '@start9labs/start-core'

export const getDriveName = (
  drive: Pick<T.Public['disks'][number], 'vendor' | 'model'> | null,
) => [drive?.vendor, drive?.model].filter(Boolean).join(' ')
