import { TuiComparator } from '@taiga-ui/addon-table'
import { getInstalledPrimaryStatus } from 'src/app/services/pkg-status-rendering.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import { byTitle } from 'src/app/utils/get-package-data'

export const byName: TuiComparator<PackageDataEntry> = byTitle

export const byStatus: TuiComparator<PackageDataEntry> = (a, b) =>
  getInstalledPrimaryStatus(b) > getInstalledPrimaryStatus(a) ? -1 : 1
