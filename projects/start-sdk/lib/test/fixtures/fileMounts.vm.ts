import * as fs from 'node:fs/promises'
import { strict as assert } from 'node:assert'
import { execFileSync } from 'node:child_process'
import { SubContainer } from '../../util/SubContainer'
import { Mounts } from '../../mainFn/Mounts'
import { FileHelper } from '../../util/fileHelper'

async function run() {
  const rootfs = process.argv[2]
  const source = '/media/startos/volumes/file-watch-test/config.txt'
  await fs.mkdir('/media/startos/volumes/file-watch-test', { recursive: true })
  await fs.mkdir('/media/startos/images', { recursive: true })
  const asset = '/media/startos/assets/file-watch-test.txt'
  await fs.mkdir('/media/startos/assets', { recursive: true })
  await fs.writeFile(asset, 'asset\n')
  await fs.writeFile('/media/startos/images/file-watch-test.json', '{}')
  await fs.writeFile('/media/startos/images/file-watch-test.env', '')
  await fs.writeFile(source, 'old\n', { mode: 0o640 })
  execFileSync('python3', [
    '-c',
    'import os,sys; os.setxattr(sys.argv[1], "user.test", b"keep")',
    source,
  ])
  const effects = {
    onLeaveContext: () => {},
    subcontainer: {
      createFs: async () => [rootfs, 'file-watch-test'],
      destroyFs: async () => null,
    },
  } as any
  const sub = await SubContainer.eager<any>(
    effects,
    { imageId: 'file-watch-test' },
    Mounts.of<any>()
      .mountVolume({
        volumeId: 'file-watch-test',
        subpath: 'config.txt',
        mountpoint: '/etc/watched.conf',
        type: 'infer',
        readonly: true,
        idmap: [{ fromId: 0, toId: 1000 }],
      })
      .mountAssets({
        subpath: 'file-watch-test.txt',
        mountpoint: '/etc/asset.conf',
        type: 'file',
      }),
    'file-watch-test',
  )
  let child: Awaited<ReturnType<typeof sub.spawn>> | undefined
  try {
    child = await sub.spawn(
      ['sh', '-c', 'while :; do cat /etc/watched.conf; sleep 0.05; done'],
      { stdio: 'pipe' },
    )
    let output = ''
    child.stdout!.on('data', data => {
      output += data
    })
    let errors = ''
    child.stderr!.on('data', data => {
      errors += data
    })
    const seen = async (value: string) => {
      for (let i = 0; i < 200 && !output.includes(value); i++)
        await new Promise(resolve => setTimeout(resolve, 25))
      assert(
        output.includes(value),
        `consumer missed ${value}: ${output}; ${errors}`,
      )
    }
    await seen('old\n')
    const file = FileHelper.string(source)
    const countMounts = async () =>
      (await fs.readFile('/proc/self/mountinfo', 'utf8'))
        .split('\n')
        .filter(line => line.split(' ')[4]?.startsWith(rootfs)).length
    const mountCount = await countMounts()
    const values = Array.from({ length: 20 }, (_, i) => `value-${i}\n`)
    for (const value of values) {
      await file.write(effects, value)
      assert.equal(
        await fs.readFile(sub.subpath('/etc/watched.conf'), 'utf8'),
        value,
      )
      assert.equal((await fs.stat(sub.subpath('/etc/watched.conf'))).uid, 1000)
      assert.equal(
        execFileSync(
          'python3',
          [
            '-c',
            'import os,sys; print(os.getxattr(sys.argv[1], "user.test").decode())',
            source,
          ],
          { encoding: 'utf8' },
        ).trim(),
        'keep',
      )
      await seen(value)
    }
    const nativeFs: typeof fs = require('node:fs/promises')
    const stat = nativeFs.stat
    let sourceStats = 0
    nativeFs.stat = (async (...args: Parameters<typeof fs.stat>) => {
      const result = await stat(...args)
      if (args[0] === source && ++sourceStats === 2) await fs.unlink(source)
      return result
    }) as typeof fs.stat
    try {
      await assert.rejects(file.write(effects, 'deleted-before-rebind\n'))
    } finally {
      nativeFs.stat = stat
    }
    await assert.rejects(fs.stat(source), { code: 'ENOENT' })
    const deleted = await sub.exec(['cat', '/etc/watched.conf'])
    assert.equal(deleted.stdout, values[values.length - 1])
    assert.equal(deleted.exitCode, 0)
    await fs.writeFile(`${source}.new`, 'external\n')
    await fs.rename(`${source}.new`, source)
    await seen('external\n')
    assert.equal(await file.read().once(), 'external\n')
    assert.equal(await countMounts(), mountCount)
    assert.equal(errors, '')
    assert(
      output
        .split('\n')
        .slice(0, -1)
        .every(value =>
          ['old\n', 'external\n', ...values].includes(`${value}\n`),
        ),
      'consumer saw partial contents',
    )
    const readonly = await sub.exec([
      'sh',
      '-c',
      'echo corrupt >/etc/watched.conf',
    ])
    assert.notEqual(readonly.exitCode, 0)
    assert.equal(await fs.readFile(source, 'utf8'), 'external\n')
    const readonlyAsset = await sub.exec([
      'sh',
      '-c',
      'echo corrupt >/etc/asset.conf',
    ])
    assert.notEqual(readonlyAsset.exitCode, 0)
    assert.equal(await fs.readFile(asset, 'utf8'), 'asset\n')
    console.log(
      'PASS: repeated own-file rebinds, running exec namespace, deletion race, external watch, idmap and readonly volume/assets',
    )
  } finally {
    child?.kill('SIGTERM')
    await sub.destroy()
    await fs.rm(source, { force: true })
    await fs.rm(asset, { force: true })
    await fs.rm('/media/startos/volumes/file-watch-test/.config.txt.tmp', {
      force: true,
    })
    await fs.rm('/media/startos/images/file-watch-test.json', { force: true })
    await fs.rm('/media/startos/images/file-watch-test.env', { force: true })
  }
}
run().catch(error => {
  console.error(error)
  process.exitCode = 1
})
