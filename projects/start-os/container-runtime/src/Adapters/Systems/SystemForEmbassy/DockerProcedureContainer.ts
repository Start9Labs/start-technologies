import * as fs from 'fs/promises'
import * as cp from 'child_process'
import { SubContainer, types as T } from '@start9labs/start-sdk'
import { promisify } from 'util'
import { DockerProcedure } from '../../../Models/DockerProcedure'
import { Volume } from './matchVolume'
import {
  CommandOptions,
  ExecOptions,
  SubContainer as SubContainerNS,
} from '@start9labs/start-sdk/lib/util/SubContainer'
import { Mounts } from '@start9labs/start-sdk/lib/mainFn/Mounts'
import { Manifest } from '@start9labs/start-core/osBindings'
import {
  BackupEffects,
  mountBackupTarget,
} from '@start9labs/start-sdk/lib/backup/Backups'
import { Drop } from '@start9labs/start-sdk/lib/util'
import { SDKManifest } from '@start9labs/start-core/types'
export const exec = promisify(cp.exec)
export const execFile = promisify(cp.execFile)

export class DockerProcedureContainer extends Drop {
  private constructor(
    private readonly subcontainer: SubContainer<SDKManifest>,
  ) {
    super()
  }

  static async of(
    effects: T.Effects,
    packageId: string,
    data: DockerProcedure,
    volumes: { [id: string]: Volume },
    name: string,
    options: { subcontainer?: SubContainer<SDKManifest> } = {},
  ) {
    const subcontainer =
      options?.subcontainer ??
      (await DockerProcedureContainer.createSubContainer(
        effects,
        packageId,
        data,
        volumes,
        name,
      ))
    return new DockerProcedureContainer(subcontainer)
  }
  static async createSubContainer(
    effects: T.Effects,
    packageId: string,
    data: DockerProcedure,
    volumes: { [id: string]: Volume },
    name: string,
  ) {
    const subcontainer = await SubContainerNS.eager(
      effects as BackupEffects,
      { imageId: data.image },
      null,
      name,
    )

    if (data.mounts) {
      const mounts = data.mounts
      for (const mount in mounts) {
        const path = mounts[mount].startsWith('/')
          ? `${subcontainer.rootfs}${mounts[mount]}`
          : `${subcontainer.rootfs}/${mounts[mount]}`
        await fs.mkdir(path, { recursive: true })
        const volumeMount: Volume = volumes[mount]
        if (volumeMount.type === 'data') {
          await subcontainer.mount(
            Mounts.of().mountVolume({
              volumeId: mount,
              subpath: null,
              mountpoint: mounts[mount],
              readonly: false,
            }),
          )
        } else if (volumeMount.type === 'assets') {
          await subcontainer.mount(
            Mounts.of().mountAssets({
              subpath: mount,
              mountpoint: mounts[mount],
            }),
          )
        } else if (volumeMount.type === 'certificate') {
          const hostInfo = await effects.getHostInfo({
            hostId: volumeMount['interface-id'],
          })
          const hostnames = [
            `${packageId}.embassy`,
            ...new Set(
              Object.values(hostInfo?.bindings || {})
                .flatMap(b => b.addresses.available)
                .map(h => h.hostname),
            ).values(),
          ]
          const certChain = await effects.getSslCertificate({
            hostnames,
          })
          const key = await effects.getSslKey({
            hostnames,
          })
          await fs.writeFile(
            `${path}/${volumeMount['interface-id']}.cert.pem`,
            certChain.join('\n'),
          )
          await fs.writeFile(
            `${path}/${volumeMount['interface-id']}.key.pem`,
            key,
          )
        } else if (volumeMount.type === 'pointer') {
          await effects.mount({
            location: path,
            target: {
              packageId: volumeMount['package-id'],
              subpath: volumeMount.path,
              readonly: volumeMount.readonly,
              volumeId: volumeMount['volume-id'],
              idmap: [],
            },
          })
        } else if (volumeMount.type === 'backup') {
          // Mount the backup target via the same rbind native 0.4 packages
          // use (Backups.ts), at the manifest-declared mountpoint the legacy
          // docker backup/restore procedure expects.
          await mountBackupTarget(subcontainer.rootfs, mounts[mount])
        }
      }
    }
    return subcontainer
  }

  async exec(
    commands: string[],
    options?: CommandOptions & ExecOptions,
    timeoutMs?: number | null,
  ) {
    try {
      return await this.subcontainer.exec(commands, options, timeoutMs)
    } finally {
      await this.subcontainer.destroy?.()
    }
  }

  async execFail(
    commands: string[],
    timeoutMs: number | null,
    options?: CommandOptions & ExecOptions,
  ) {
    try {
      const res = await this.subcontainer.exec(commands, options, timeoutMs)
      if (res.exitCode !== 0) {
        const codeOrSignal =
          res.exitCode !== null
            ? `code ${res.exitCode}`
            : `signal ${res.exitSignal}`
        throw new Error(
          `Process exited with ${codeOrSignal}: ${res.stderr.toString()}`,
        )
      }
      return res
    } finally {
      await this.subcontainer.destroy?.()
    }
  }

  // async spawn(commands: string[]): Promise<cp.ChildProcess> {
  //   return await this.subcontainer.spawn(commands)
  // }

  onDrop(): void {
    this.subcontainer.destroy?.()
  }
}
