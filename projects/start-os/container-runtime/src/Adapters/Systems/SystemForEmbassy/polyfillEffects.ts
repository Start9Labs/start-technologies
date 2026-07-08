import * as fs from 'fs/promises'
import * as oet from './oldEmbassyTypes'
import { Volume } from '../../../Models/Volume'
import * as child_process from 'child_process'
import { promisify } from 'util'
import { daemons, startSdk, T, utils } from '@start9labs/start-sdk'
import 'isomorphic-fetch'
import { Manifest } from './matchManifest'
import { DockerProcedureContainer } from './DockerProcedureContainer'
import * as cp from 'child_process'
import { Effects } from '../../../Models/Effects'
import { Mounts } from '@start9labs/start-sdk/lib/mainFn/Mounts'
export const execFile = promisify(cp.execFile)
export const polyfillEffects = (
  effects: Effects,
  manifest: Manifest,
): oet.Effects => {
  const self = {
    effects,
    manifest,
    async writeFile(input: {
      path: string
      volumeId: string
      toWrite: string
    }): Promise<void> {
      await fs.writeFile(
        new Volume(input.volumeId, input.path).path,
        input.toWrite,
      )
    },
    async readFile(input: { volumeId: string; path: string }): Promise<string> {
      return (
        await fs.readFile(new Volume(input.volumeId, input.path).path)
      ).toString()
    },
    async metadata(input: {
      volumeId: string
      path: string
    }): Promise<oet.Metadata> {
      const stats = await fs.stat(new Volume(input.volumeId, input.path).path)
      return {
        fileType: stats.isFile() ? 'file' : 'directory',
        gid: stats.gid,
        uid: stats.uid,
        mode: stats.mode,
        isDir: stats.isDirectory(),
        isFile: stats.isFile(),
        isSymlink: stats.isSymbolicLink(),
        len: stats.size,
        readonly: (stats.mode & 0o200) > 0,
      }
    },
    async createDir(input: {
      volumeId: string
      path: string
    }): Promise<string> {
      const path = new Volume(input.volumeId, input.path).path
      await fs.mkdir(path, { recursive: true })
      return path
    },
    async readDir(input: {
      volumeId: string
      path: string
    }): Promise<string[]> {
      return fs.readdir(new Volume(input.volumeId, input.path).path)
    },
    async removeDir(input: {
      volumeId: string
      path: string
    }): Promise<string> {
      const path = new Volume(input.volumeId, input.path).path
      await fs.rmdir(new Volume(input.volumeId, input.path).path, {
        recursive: true,
      })
      return path
    },
    removeFile(input: { volumeId: string; path: string }): Promise<void> {
      return fs.rm(new Volume(input.volumeId, input.path).path)
    },
    async writeJsonFile(input: {
      volumeId: string
      path: string
      toWrite: Record<string, unknown>
    }): Promise<void> {
      await fs.writeFile(
        new Volume(input.volumeId, input.path).path,
        JSON.stringify(input.toWrite),
      )
    },
    async readJsonFile(input: {
      volumeId: string
      path: string
    }): Promise<Record<string, unknown>> {
      return JSON.parse(
        (
          await fs.readFile(new Volume(input.volumeId, input.path).path)
        ).toString(),
      )
    },
    runCommand({
      command,
      args,
      timeoutMillis,
    }: {
      command: string
      args?: string[] | undefined
      timeoutMillis?: number | undefined
    }): Promise<oet.ResultType<string>> {
      const commands: [string, ...string[]] = [command, ...(args || [])]
      return startSdk
        .runCommand(
          effects,
          { imageId: manifest.main.image },
          commands,
          { mounts: Mounts.of() },
          commands.join(' '),
        )
        .then((x: any) => ({
          stderr: x.stderr.toString(),
          stdout: x.stdout.toString(),
        }))
        .then((x: any) =>
          !!x.stderr ? { error: x.stderr } : { result: x.stdout },
        )
    },
    runDaemon(input: { command: string; args?: string[] | undefined }): {
      wait(): Promise<oet.ResultType<string>>
      term(): Promise<void>
    } {
      const promiseSubcontainer = DockerProcedureContainer.createSubContainer(
        effects,
        manifest.id,
        manifest.main,
        manifest.volumes,
        [input.command, ...(input.args || [])].join(' '),
      )
      const daemon = promiseSubcontainer.then(subcontainer =>
        daemons.runCommand()(effects, subcontainer, {
          command: [input.command, ...(input.args || [])],
        }),
      )
      return {
        wait: () =>
          daemon.then(daemon =>
            daemon.wait().then(() => {
              return { result: '' }
            }),
          ),
        term: () => daemon.then(daemon => daemon.term()),
      }
    },
    async chown(input: {
      volumeId: string
      path: string
      uid: string
    }): Promise<null> {
      const commands: [string, ...string[]] = [
        'chown',
        '--recursive',
        input.uid,
        `/drive/${input.path}`,
      ]
      await startSdk
        .runCommand(
          effects,
          { imageId: manifest.main.image },
          commands,
          {
            mounts: Mounts.of().mountVolume({
              volumeId: input.volumeId,
              subpath: null,
              mountpoint: '/drive',
              readonly: false,
            }),
          },
          commands.join(' '),
        )
        .then((x: any) => ({
          stderr: x.stderr.toString(),
          stdout: x.stdout.toString(),
        }))
        .then((x: any) => {
          if (!!x.stderr) {
            throw new Error(x.stderr)
          }
        })
      return null
    },
    async chmod(input: {
      volumeId: string
      path: string
      mode: string
    }): Promise<null> {
      const commands: [string, ...string[]] = [
        'chmod',
        '--recursive',
        input.mode,
        `/drive/${input.path}`,
      ]
      await startSdk
        .runCommand(
          effects,
          { imageId: manifest.main.image },
          commands,
          {
            mounts: Mounts.of().mountVolume({
              volumeId: input.volumeId,
              subpath: null,
              mountpoint: '/drive',
              readonly: false,
            }),
          },
          commands.join(' '),
        )
        .then((x: any) => ({
          stderr: x.stderr.toString(),
          stdout: x.stdout.toString(),
        }))
        .then((x: any) => {
          if (!!x.stderr) {
            throw new Error(x.stderr)
          }
        })
      return null
    },
    sleep(timeMs: number): Promise<null> {
      return new Promise(resolve => setTimeout(resolve, timeMs))
    },
    trace(whatToPrint: string): void {
      console.trace(utils.asError(whatToPrint))
    },
    warn(whatToPrint: string): void {
      console.warn(utils.asError(whatToPrint))
    },
    error(whatToPrint: string): void {
      console.error(utils.asError(whatToPrint))
    },
    debug(whatToPrint: string): void {
      console.debug(utils.asError(whatToPrint))
    },
    info(whatToPrint: string): void {
      console.log(false)
    },
    is_sandboxed(): boolean {
      return false
    },
    exists(input: { volumeId: string; path: string }): Promise<boolean> {
      return self
        .metadata(input)
        .then(() => true)
        .catch(() => false)
    },
    async fetch(
      url: string,
      options?:
        | {
            method?:
              | 'GET'
              | 'POST'
              | 'PUT'
              | 'DELETE'
              | 'HEAD'
              | 'PATCH'
              | undefined
            headers?: Record<string, string> | undefined
            body?: string | undefined
          }
        | undefined,
    ): Promise<{
      method: string
      ok: boolean
      status: number
      headers: Record<string, string>
      body?: string | null | undefined
      text(): Promise<string>
      json(): Promise<unknown>
    }> {
      const fetched = await fetch(url, options)
      return {
        method: fetched.type,
        ok: fetched.ok,
        status: fetched.status,
        headers: Object.fromEntries(fetched.headers.entries()),
        body: await fetched.text(),
        text: () => fetched.text(),
        json: () => fetched.json(),
      }
    },

    runRsync(rsyncOptions: {
      srcVolume: string
      dstVolume: string
      srcPath: string
      dstPath: string
      options: oet.BackupOptions
    }): {
      id: () => Promise<string>
      wait: () => Promise<null>
      progress: () => Promise<number>
    } {
      let secondRun: ReturnType<typeof self._runRsync> | undefined
      let firstRun = self._runRsync(rsyncOptions)
      let waitValue = firstRun.wait().then(x => {
        secondRun = self._runRsync(rsyncOptions)
        return secondRun.wait()
      })
      const id = async () => {
        return secondRun?.id?.() ?? firstRun.id()
      }
      const wait = () => waitValue
      const progress = async () => {
        const secondProgress = secondRun?.progress?.()
        if (secondProgress) {
          return (await secondProgress) / 2.0 + 0.5
        }
        return (await firstRun.progress()) / 2.0
      }
      return { id, wait, progress }
    },
    _runRsync(rsyncOptions: {
      srcVolume: string
      dstVolume: string
      srcPath: string
      dstPath: string
      options: oet.BackupOptions
    }): {
      id: () => Promise<string>
      wait: () => Promise<null>
      progress: () => Promise<number>
    } {
      const { srcVolume, dstVolume, srcPath, dstPath, options } = rsyncOptions
      const command = 'rsync'
      const args: string[] = []
      if (options.delete) {
        args.push('--delete')
      }
      if (options.force) {
        args.push('--force')
      }
      if (options.ignoreExisting) {
        args.push('--ignore-existing')
      }
      for (const exclude of options.exclude) {
        args.push(`--exclude=${exclude}`)
      }
      args.push('-actAXH')
      args.push('--info=progress2')
      // --no-inc-recursive would give accurate progress percentages (since
      // rsync knows the full file list up front), but it forces a full
      // pre-scan that causes timeouts on large transfers. If we start
      // surfacing progress to users, do a raw file count up front and compute
      // percentage from bytes/files seen instead of relying on rsync's own
      // percentage.
      args.push(new Volume(srcVolume, srcPath).path)
      args.push(new Volume(dstVolume, dstPath).path)
      const spawned = child_process.spawn(command, args, { detached: true })
      let percentage = 0.0
      spawned.stdout.on('data', (data: unknown) => {
        const lines = String(data).replace('\r', '\n').split('\n')
        for (const line of lines) {
          const parsed = /$([0-9.]+)%/.exec(line)?.[1]
          if (!parsed) continue
          percentage = Number.parseFloat(parsed)
        }
      })

      spawned.stderr.on('data', (data: unknown) => {
        console.error(`polyfill.runAsync`, utils.asError(data))
      })

      const id = async () => {
        const pid = spawned.pid
        if (pid === undefined) {
          throw new Error('rsync process has no pid')
        }
        return String(pid)
      }
      const waitPromise = new Promise<null>((resolve, reject) => {
        spawned.on('exit', (code: any) => {
          if (code === 0) {
            resolve(null)
          } else {
            reject(new Error(`rsync exited with code ${code}`))
          }
        })
      })
      const wait = () => waitPromise
      const progress = () => Promise.resolve(percentage)
      return { id, wait, progress }
    },
    async diskUsage(
      options?: { volumeId: string; path: string } | undefined,
    ): Promise<{ used: number; total: number }> {
      const output = await execFile('df', ['--block-size=1', '-P', '/'])
        .then((x: any) => ({
          stderr: x.stderr.toString(),
          stdout: x.stdout.toString(),
        }))
        .then((x: any) => {
          if (!!x.stderr) {
            throw new Error(x.stderr)
          }
          return parseDfOutput(x.stdout)
        })
      if (!!options) {
        const used = await execFile('du', [
          '-s',
          '--block-size=1',
          '-P',
          new Volume(options.volumeId, options.path).path,
        ])
          .then((x: any) => ({
            stderr: x.stderr.toString(),
            stdout: x.stdout.toString(),
          }))
          .then((x: any) => {
            if (!!x.stderr) {
              throw new Error(x.stderr)
            }
            return Number.parseInt(x.stdout.split(/\s+/)[0])
          })
        return {
          ...output,
          used,
        }
      }
      return output
    },
  }
  return self
}

function parseDfOutput(output: string): { used: number; total: number } {
  const lines = output
    .split('\n')
    .filter(x => x.length)
    .map(x => x.split(/\s+/))
  const index = lines.splice(0, 1)[0].map(x => x.toLowerCase())
  const usedIndex = index.indexOf('used')
  const sizeIndex = index.indexOf('size')
  const used = lines.map(x => Number.parseInt(x[usedIndex]))[0] || 0
  const total = lines.map(x => Number.parseInt(x[sizeIndex]))[0] || 0
  return { used, total }
}
