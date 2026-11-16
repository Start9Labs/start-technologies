import * as cp from 'node:child_process'
import * as fs from 'node:fs'
import * as os from 'node:os'
import * as path from 'node:path'
import { once } from 'node:events'
import * as T from '@start9labs/start-core/types'
import { CommandController } from '../mainFn/CommandController'
import { SubContainer } from '../util/SubContainer'

describe('CommandController output capture', () => {
  it.each([
    { stdout: true, stderr: false },
    { stdout: false, stderr: true },
    { stdout: true, stderr: true },
    { stdout: false, stderr: false },
  ])(
    'finishes 1 MiB writes with callbacks %j and no viewer',
    async callbacks => {
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'daemon-output-'))
      const stdoutPath = path.join(dir, 'stdout')
      const stderrPath = path.join(dir, 'stderr')
      const stdoutFd = fs.openSync(stdoutPath, 'w')
      const stderrFd = fs.openSync(stderrPath, 'w')
      const stdout: Buffer[] = []
      const stderr: Buffer[] = []
      let child: cp.ChildProcess | undefined
      let closed: Promise<unknown> | undefined
      let timer: NodeJS.Timeout | undefined
      const subcontainer = {
        spawn: async (command: string[], options: cp.SpawnOptions) => {
          const stdio = options.stdio ?? 'inherit'
          const streams = Array.isArray(stdio) ? stdio : [stdio, stdio, stdio]
          child = cp.spawn(command[0], command.slice(1), {
            ...options,
            stdio: streams.map((stream, fd) =>
              stream === 'inherit'
                ? ['ignore', stdoutFd, stderrFd][fd]
                : stream,
            ) as cp.StdioOptions,
          })
          closed = once(child, 'close')
          return child
        },
      } as unknown as SubContainer<T.SDKManifest>
      try {
        const controller = (
          await CommandController.of<T.SDKManifest, typeof subcontainer>()(
            {} as T.Effects,
            subcontainer,
            {
              command: [
                process.execPath,
                '-e',
                `const fs = require('node:fs');
               fs.writeFileSync(2, Buffer.alloc(1024 * 1024, 'e'));
               fs.writeFileSync(1, Buffer.alloc(1024 * 1024, 'o'));`,
              ],
              ...(callbacks.stdout
                ? { onStdout: (chunk: Buffer) => stdout.push(chunk) }
                : {}),
              ...(callbacks.stderr
                ? { onStderr: (chunk: Buffer) => stderr.push(chunk) }
                : {}),
            },
          )
        ).leak()
        await Promise.race([
          controller.wait(),
          new Promise((_, reject) => {
            timer = setTimeout(
              () => reject(new Error('Output blocked with no viewer')),
              2000,
            )
          }),
        ])
        await closed
        expect(
          (callbacks.stdout
            ? Buffer.concat(stdout)
            : fs.readFileSync(stdoutPath)
          ).equals(Buffer.alloc(1024 * 1024, 'o')),
        ).toBe(true)
        expect(
          (callbacks.stderr
            ? Buffer.concat(stderr)
            : fs.readFileSync(stderrPath)
          ).equals(Buffer.alloc(1024 * 1024, 'e')),
        ).toBe(true)
      } finally {
        clearTimeout(timer)
        child?.kill('SIGKILL')
        await closed
        fs.closeSync(stdoutFd)
        fs.closeSync(stderrFd)
        fs.rmSync(dir, { recursive: true, force: true })
      }
    },
  )
})
