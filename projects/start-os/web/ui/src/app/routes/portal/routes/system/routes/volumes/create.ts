import { convertBytes, i18nPipe } from '@start9labs/shared'
import { ISB } from '@start9labs/start-core'

import { Params } from 'src/app/services/api/api.types'
import { getPartitionName } from 'src/app/utils/get-partition-name'
import { DataModel } from 'src/app/services/patch-db/data-model'

export type Source = Params<'volumes.create'>['source']

export type Creator = (params: {
  name: string
  source: Source
}) => Promise<unknown>

const nameSpec = (i18n: i18nPipe) =>
  ISB.Value.text({
    name: i18n.transform('Name'),
    description: i18n.transform('You can rename it later.'),
    required: true,
    default: null,
  })

export function localSpec(i18n: i18nPipe) {
  return ISB.InputSpec.of({ name: nameSpec(i18n) })
}

export type Disk = DataModel['disks'][number]

export const FORMAT = 'format'

/** Drives a volume may use: not the OS drive or a StartOS data drive. */
export const usableDrives = (disks: Disk[]) =>
  disks.filter(
    disk =>
      !/-part\d+$/.test(disk.stablePath) &&
      !disk.guid &&
      !disk.partitions.some(partition => partition.guid),
  )

export const driveName = (i18n: i18nPipe, drive: Disk) =>
  [drive.vendor, drive.model].filter(Boolean).join(' ') ||
  i18n.transform('Unknown Drive')

export function diskSpec(i18n: i18nPipe, drives: Disk[]) {
  return ISB.InputSpec.of({
    drive: ISB.Value.union({
      name: i18n.transform('Drive'),
      default: null,
      variants: ISB.Variants.of(
        Object.fromEntries(
          drives.map(drive => [
            drive.logicalname,
            {
              name: `${driveName(i18n, drive)} · ${convertBytes(drive.capacity)} · ${drive.logicalname}`,
              spec: ISB.InputSpec.of({
                partition: ISB.Value.select({
                  name: i18n.transform('Partition'),
                  description: i18n.transform(
                    'Use an existing partition, or erase the drive and create a new one.',
                  ),
                  default: drive.partitions.length ? null : FORMAT,
                  values: {
                    ...Object.fromEntries(
                      drive.partitions.map(p => [
                        p.logicalname,
                        `${getPartitionName(p)} · ${convertBytes(p.capacity)}`,
                      ]),
                    ),
                    [FORMAT]: i18n.transform('Erase and format this drive'),
                  },
                }),
              }),
            },
          ]),
        ),
      ),
    }),
    name: nameSpec(i18n),
    path: ISB.Value.text({
      name: i18n.transform('Path'),
      description: i18n.transform(
        'Optional. The folder on the drive to use, such as media/photos. Leave it blank to use the whole drive, which is usually best for a freshly formatted drive. Missing folders are created.',
      ),
      placeholder: 'e.g. media/photos',
      required: false,
      default: null,
    }),
  })
}

export function cifsSpec(i18n: i18nPipe) {
  return ISB.InputSpec.of({
    hostname: ISB.Value.text({
      name: i18n.transform('Hostname'),
      description: i18n.transform(
        'The hostname of your target device on the Local Area Network.',
      ),
      placeholder: `e.g. 'My Computer' OR 'my-computer.local'`,
      required: true,
      default: null,
    }),
    path: ISB.Value.text({
      name: i18n.transform('Path'),
      description: i18n.transform(
        "The shared folder's name, optionally followed by a folder inside it, such as Photos/albums. A leading / is optional.",
      ),
      placeholder: 'e.g. Photos/albums',
      required: true,
      default: null,
    }),
    username: ISB.Value.text({
      name: i18n.transform('Username'),
      description: i18n.transform(
        'On Linux, this is the samba username you created when sharing the folder. On Mac and Windows, this is the username of the user who is sharing the folder.',
      ),
      required: false,
      default: null,
    }),
    password: ISB.Value.text({
      name: i18n.transform('Password'),
      description: i18n.transform(
        'On Linux, this is the samba password you created when sharing the folder. On Mac and Windows, this is the password of the user who is sharing the folder.',
      ),
      required: false,
      default: null,
      masked: true,
    }),
  })
}

export const diskSource = (
  logicalname: string,
  path: string | null,
): Source => ({ kind: 'disk', logicalname, path: path || '' })

export const cifsSource = ({
  hostname,
  path,
  username,
  password,
}: ReturnType<typeof cifsSpec>['_TYPE']): Source => ({
  kind: 'cifs',
  hostname,
  path,
  username: username || '',
  password,
})
