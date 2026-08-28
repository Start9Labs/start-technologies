import { TuiComparator } from '@taiga-ui/addon-table'
import { getInstalledPrimaryStatus } from 'src/app/services/pkg-status-rendering.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import { getManifest } from 'src/app/utils/get-package-data'

export const byName: TuiComparator<PackageDataEntry> = (a, b) =>
  getManifest(b).title.toLowerCase() > getManifest(a).title.toLowerCase()
    ? -1
    : 1

export const byStatus: TuiComparator<PackageDataEntry> = (a, b) =>
  getInstalledPrimaryStatus(b) > getInstalledPrimaryStatus(a) ? -1 : 1
