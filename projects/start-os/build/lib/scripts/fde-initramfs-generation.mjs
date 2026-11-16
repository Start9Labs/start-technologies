import {
  cp,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  writeFile,
} from 'node:fs/promises'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { verifyGeneratedArchive } from './fde-initramfs-archive.mjs'
import { admitInitramfsSources, bootModules } from './fde-initramfs-control.mjs'
import { prepareNativeHooks } from './fde-initramfs-hooks.mjs'
import { kernelDirectory } from './fde-initramfs-paths.mjs'
import {
  capturedStreams,
  instrumentProducer,
  runNativeProducer,
} from './fde-initramfs-producer.mjs'

/**
 * Assembles an unsigned candidate from stable admitted public sources.
 * The caller owns the read-only source namespace and consumes the archive before return.
 * Complete runtime/plugin/configuration/EARLY admission remains a signing obligation.
 */
export async function withAssembledInitramfs(parent, consume) {
  return withPrivateInitramfs(
    parent,
    async ({ directory, archive, captures }) => {
      const configuration = join(directory, 'configuration')
      await admitInitramfsSources('/', '/etc/initramfs-tools')
      try {
        const hooks = '/etc/initramfs-tools/hooks'
        if (
          !(await lstat(hooks)).isDirectory() ||
          (await readdir(hooks)).length
        )
          throw Error('Unsupported additional native source hooks')
      } catch (error) {
        if (error.code !== 'ENOENT') throw error
      }
      await cp('/etc/initramfs-tools', configuration, {
        recursive: true,
        preserveTimestamps: true,
      })
      const hooks = join(directory, 'hooks')
      const profile = await prepareNativeHooks('/', hooks)
      const aliases = join(directory, 'provider-aliases.json')
      await writeFile(
        aliases,
        JSON.stringify(await profile.providerAliases()),
        { flag: 'wx', mode: 0o600 },
      )
      await mkdir(join(configuration, 'hooks'), { recursive: true })
      await writeFile(
        join(configuration, 'hooks/fde-required-modules'),
        `#!/bin/sh
case "\${1-}" in prereqs) exit 0 ;; esac
set -eu
. /usr/share/initramfs-tools/hook-functions
manual_add_modules ${bootModules.join(' ')}
`,
        { flag: 'wx', mode: 0o700 },
      )
      await cp(
        new URL('./fde-initramfs-tools-hook', import.meta.url),
        join(configuration, 'hooks/fde-tools'),
      )
      const producer = join(directory, 'mkinitramfs')
      await writeFile(
        producer,
        instrumentProducer(
          await readFile('/usr/sbin/mkinitramfs'),
          await readFile('/usr/share/initramfs-tools/hook-functions'),
        ),
        { flag: 'wx', mode: 0o700 },
      )
      const { version } = await kernelDirectory('/')
      await runNativeProducer(
        producer,
        ['-c', 'gzip', '-d', configuration, '-o', archive, version],
        {
          PATH: '/usr/sbin:/usr/bin:/sbin:/bin',
          LC_ALL: 'C',
          HOME: directory,
          TMPDIR: directory,
          FDE_NATIVE_HOOKS: hooks,
          FDE_PROVIDER_ALIASES: aliases,
          FDE_ADMIT_SOURCES: fileURLToPath(
            new URL('./fde-admit-initramfs-sources.mjs', import.meta.url),
          ),
          FDE_PREPARE: fileURLToPath(
            new URL('./fde-prepare-initramfs.mjs', import.meta.url),
          ),
          FDE_CAPTURE_DIR: captures,
        },
      )
    },
    consume,
  )
}

/**
 * The producer admits its sources/closure and joins all writers before returning.
 * The consumer finishes archive use before returning; neither callback retains writers.
 */
export async function withPrivateInitramfs(parent, produce, consume) {
  const directory = await mkdtemp(join(parent, '.fde-initramfs-'))
  try {
    const archive = join(directory, 'initrd')
    const captures = join(directory, 'captures')
    await mkdir(captures, { mode: 0o700 })
    await produce({ directory, archive, captures })
    const streams = capturedStreams(captures)
    await verifyGeneratedArchive(archive, streams.prefixes, streams.mainCpio)
    return await consume(archive)
  } finally {
    await rm(directory, { recursive: true })
  }
}
