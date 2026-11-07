import type { T } from '@start9labs/start-core'
import type { DiskBackupTarget } from 'src/app/services/api/api.types'

export function getDiskBackupTargets(
  disks: readonly T.DiskInfo[],
): Record<string, DiskBackupTarget> {
  return Object.fromEntries(
    disks.flatMap(disk =>
      disk.partitions.map(partition => [
        `disk-${partition.logicalname}`,
        {
          type: 'disk' as const,
          vendor: disk.vendor,
          model: disk.model,
          ...partition,
        },
      ]),
    ),
  )
}
