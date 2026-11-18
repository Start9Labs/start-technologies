import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { join } from 'node:path'
import { test } from 'node:test'
import { REPO } from '../../../scripts/tests/fixtures.mjs'
import {
  classify,
  PRODUCTS,
} from '../../../.github/scripts/changed-products.mjs'

const ALL = Object.keys(PRODUCTS)
const DEBS = ['start-cli', 'start-registry', 'start-tunnel']
const BUILD_PRODUCTS = ['start-os', ...DEBS]
const WEB = ['start-os', 'start-tunnel', 'start-wrt']
const NATIVE_OS_INPUTS = [
  'projects/start-os/build/lib/scripts/fde-boot-mode',
  'projects/start-os/build/lib/scripts/fde-initramfs-control.mjs',
  'projects/start-os/build/dpkg-deps/uefi.platforms',
]

for (const [path, push, pullRequest = push, osImage = false] of [
  ['shared-libs/crates/start-core/src/lib.rs', ALL],
  ...NATIVE_OS_INPUTS.map(path => [path, ALL, ALL, true]),
  [
    'projects/start-os/build/dpkg-deps/uefi.depends',
    ['start-os'],
    undefined,
    true,
  ],
  ['Cargo.lock', ALL],
  ['build/env/check-platform.sh', ALL, ALL, true],
  ['debian/build.sh', BUILD_PRODUCTS, BUILD_PRODUCTS, true],
  [
    '.github/workflows/build-products.yaml',
    BUILD_PRODUCTS,
    BUILD_PRODUCTS,
    true,
  ],
  ['.github/workflows/start-wrt.yaml', ['start-wrt']],
  ['.github/scripts/changed-products.mjs', ALL],
  ['.github/scripts/classify-event.sh', ALL],
  ['projects/start-cli/src/main.rs', ['start-cli']],
  ['projects/start-registry/Cargo.toml', ['start-registry']],
  ['projects/start-tunnel/web/src/main.ts', ['start-tunnel']],
  ['shared-libs/ts-modules/shared/src/index.ts', WEB],
  ['package-lock.json', WEB],
  ['package.json', WEB, undefined, true],
  ['projects/start-sdk/lib/index.ts', ['start-os']],
  [
    'projects/start-os/build/image-recipe/build.sh',
    ['start-os'],
    undefined,
    true,
  ],
  ['projects/start-os/startd.service', ['start-os'], undefined, true],
  ['projects/start-os/src/main.rs', ['start-os']],
  ['projects/start-os/docs/src/overview.md', []],
  ['projects/start-os/release-notes/0.4.1.md', ['start-os'], undefined, true],
  ['projects/start-cli/changelog/patch-fixed-x.md', ['start-cli'], []],
  ['projects/start-tunnel/release-notes/1.2.0.md', ['start-tunnel'], []],
  ['scripts/manage-release.sh', [...DEBS, 'start-wrt'], []],
  ['scripts/changelog.mjs', ALL, ['start-os'], true],
  ['.prettierrc.json', ALL, ['start-os'], true],
  ['scripts/s3-put-once.sh', ['start-os', 'start-wrt'], []],
  ['AGENTS.md', []],
  ['projects/start-wrt/backend/ctrl/src/lib.rs', ['start-wrt']],
  ['projects/start-wrt/web/src/main.ts', ['start-wrt']],
  ['projects/start-wrt/release-notes/1.3.0.md', ['start-wrt'], []],
  ['projects/start-wrt/docs/src/overview.md', []],
]) {
  test(`${path} rebuilds the products whose build reads it`, () => {
    for (const [event, expected] of [
      ['push', push],
      ['pull_request', pullRequest],
    ]) {
      const result = classify([path], event)
      assert.deepEqual(result.affected, expected, event)
      assert.equal(result.osImage, osImage && expected.includes('start-os'))
    }
  })
}

test('StartWRT rebuilds when native OS inputs change', () => {
  for (const event of ['push', 'pull_request']) {
    for (const path of NATIVE_OS_INPUTS) {
      assert.deepEqual(classify([path], event, 'start-wrt.yaml'), {
        affected: ['start-wrt'],
        osImage: false,
      })
    }
    assert.deepEqual(
      classify(
        ['projects/start-os/build/image-recipe/build.sh'],
        event,
        'start-wrt.yaml',
      ),
      { affected: [], osImage: false },
    )
  }
})

test('the script prints step outputs for the changed paths on stdin', () => {
  const script = join(REPO, '.github/scripts/changed-products.mjs')
  const run = (args, input) =>
    spawnSync(process.execPath, [script, ...args], { input, encoding: 'utf8' })
  const changed =
    'projects/start-cli/src/main.rs\nprojects/start-registry/src/main.rs\n'
  const result = run(
    ['--workflow', 'build-products.yaml', '--event', 'pull_request'],
    changed,
  )
  assert.equal(result.status, 0, result.stderr)
  assert.equal(
    result.stdout,
    [
      'start-os=false',
      'start-cli=true',
      'start-registry=true',
      'start-tunnel=false',
      'os-image=false',
      'products=start-cli start-registry',
      '',
    ].join('\n'),
  )
  const nativeInputs = NATIVE_OS_INPUTS.join('\n')
  const native = run(
    ['--workflow', 'build-products.yaml', '--event', 'push'],
    nativeInputs,
  )
  assert.equal(native.status, 0, native.stderr)
  assert.equal(
    native.stdout,
    [
      'start-os=true',
      'start-cli=true',
      'start-registry=true',
      'start-tunnel=true',
      'os-image=true',
      'products=start-os start-cli start-registry start-tunnel',
      '',
    ].join('\n'),
  )
  const nativeWrt = run(
    ['--workflow', 'start-wrt.yaml', '--event', 'push'],
    nativeInputs,
  )
  assert.equal(nativeWrt.status, 0, nativeWrt.stderr)
  assert.equal(
    nativeWrt.stdout,
    'start-wrt=true\nos-image=false\nproducts=start-wrt\n',
  )
  const all = run(['--workflow', 'build-products.yaml', '--all'], '')
  assert.equal(all.status, 0, all.stderr)
  assert.match(
    all.stdout,
    /^products=start-os start-cli start-registry start-tunnel$/m,
  )
  assert.match(all.stdout, /^os-image=true$/m)
  const wrt = run(['--workflow', 'start-wrt.yaml', '--event', 'push'], changed)
  assert.equal(wrt.status, 0, wrt.stderr)
  assert.equal(wrt.stdout, 'start-wrt=false\nos-image=false\nproducts=\n')
  assert.equal(
    run(['--workflow', 'start-wrt.yaml', '--all'], '').stdout,
    'start-wrt=true\nos-image=false\nproducts=start-wrt\n',
  )
  for (const args of [
    ['--workflow', 'build-products.yaml', '--event', 'workflow_dispatch'],
    ['--event', 'pull_request'],
    ['--workflow', 'other.yaml', '--all'],
  ])
    assert.notEqual(run(args, changed).status, 0, args.join(' '))
})
