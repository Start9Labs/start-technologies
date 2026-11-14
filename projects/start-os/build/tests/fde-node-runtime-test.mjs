import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { cp, mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

assert.ok(
  process.argv[2] && process.argv[3] && process.argv[4],
  'Provide genuine reviewed providers, Node 20 executable and its native library directory',
)
const providers = resolve(process.argv[2])
const runtime = resolve(process.argv[3])
const environment = {
  ...process.env,
  LD_LIBRARY_PATH: resolve(process.argv[4]),
}
const cli = fileURLToPath(
  new URL('../lib/scripts/fde-harden-initramfs.mjs', import.meta.url),
)
const library = fileURLToPath(
  new URL('../lib/scripts/fde-initramfs-control.mjs', import.meta.url),
)

async function fixture(t) {
  const directory = await mkdtemp(join(tmpdir(), 'fde-node-runtime-'))
  t.after(() => rm(directory, { recursive: true, force: true }))
  const tree = join(directory, 'tree')
  await cp(providers, tree, { recursive: true, verbatimSymlinks: true })
  return { directory, tree }
}

function run(arguments_) {
  return spawnSync(runtime, arguments_, { env: environment, encoding: 'utf8' })
}

test('the existing Node 20 runtime imports the control-plane library without invoking a CLI', () => {
  const version = run(['--version'])
  assert.equal(version.status, 0, version.stderr)
  assert.match(version.stdout, /^v20\./)
  const result = run([
    '--input-type=module',
    '-e',
    `const api = await import(${JSON.stringify(library)}); if (typeof api.harden !== 'function' || typeof api.admitInitramfsSources !== 'function') throw Error('Missing API'); console.log('imported');`,
  ])
  assert.equal(result.status, 0, result.stderr)
  assert.equal(result.stdout, 'imported\n')
})

test('the real hardening CLI runs through a symlink under the existing Node 20 runtime', async t => {
  const { directory, tree } = await fixture(t)
  const entry = join(directory, 'entry.mjs')
  await symlink(cli, entry)
  const result = run([entry, tree])
  assert.equal(result.status, 0, result.stderr)
  const init = await readFile(join(tree, 'init'), 'utf8')
  assert.ok(init.includes("trap 'fde_fail' EXIT"))
  assert.ok(init.includes('BOOT=startos-fde\nIP=none\nnoresume=y'))
  assert.ok(
    (await readFile(join(tree, 'scripts/startos-fde'), 'utf8')).includes(
      'TPM2TOOLS_TCTI=',
    ),
  )
})

test('Node 20 CLI rejects unsupported input before mutation and rejects missing arguments', async t => {
  const { tree } = await fixture(t)
  await writeFile(join(tree, 'init'), 'unreviewed input')
  const result = run([cli, tree])
  assert.notEqual(result.status, 0)
  assert.match(result.stderr, /Unsupported/)
  assert.equal(await readFile(join(tree, 'init'), 'utf8'), 'unreviewed input')
  const missing = run([cli])
  assert.notEqual(missing.status, 0)
  assert.match(missing.stderr, /Usage:/)
})
