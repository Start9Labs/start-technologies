import type { T } from '@start9labs/start-core'

export function getPartitionName(
  partition: Pick<T.PartitionInfo, 'label' | 'logicalname'>,
): string {
  return partition.label || partition.logicalname
}
