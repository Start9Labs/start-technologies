import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  utimesSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'

const source = join(dirname(fileURLToPath(import.meta.url)), '..', 's9pk.mk')

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 's9pk-make-test-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const bin = join(root, 'bin')
  for (const directory of [bin, join(root, 'startos', 'manifest')]) {
    mkdirSync(directory, { recursive: true })
  }
  writeFileSync(
    join(root, 'startos', 'manifest', 'index.ts'),
    "id: 'example'\n",
  )
  writeFileSync(join(root, 'startos', 'index.ts'), 'original source')
  writeFileSync(join(root, 'Dockerfile'), 'FROM scratch\n')
  writeFileSync(join(root, 'tsconfig.json'), '{}')
  writeFileSync(join(root, 'package.json'), '{}')
  writeFileSync(join(root, 'package-lock.json'), '{}')
  mkdirSync(join(root, 'node_modules'))
  writeFileSync(
    join(root, 'Makefile'),
    `TS_CHECK := echo typecheck >> steps
FORMAT := echo format >> steps
JS_BUNDLE := mkdir -p javascript && cp startos/index.ts javascript/index.js && echo bundle >> steps
include ${source}
`,
  )
  writeFileSync(
    join(bin, 'start-cli'),
    `#!${process.execPath}
const fs = require('node:fs')
const args = process.argv.slice(2)
fs.appendFileSync('calls', JSON.stringify(args) + '\\n')
if (args.join(' ') === 's9pk list-ingredients') {
  console.log('javascript/index.js startos/manifest/index.ts')
  if (process.env.DYNAMIC_INGREDIENTS) {
    if (fs.existsSync('javascript/index.js')) {
      console.log(JSON.parse(fs.readFileSync('javascript/index.js', 'utf8')).ingredients.join('\\n'))
    }
  } else {
    console.log('Dockerfile')
  }
  process.exitCode = Number(process.env.LIST_STATUS || 0)
} else if (args.join(' ') === 'init-key') {
  process.exit(Number(process.env.INIT_STATUS || 0))
} else if (args.join(' ') === 'server device-info --format json') {
  console.log(JSON.stringify({ hardware: { arch: process.env.ARCH || 'aarch64' } }))
  process.exit(Number(process.env.DEVICE_STATUS || 0))
} else if (args[0] === 's9pk' && args[1] === 'inspect') {
  console.log(JSON.stringify({ title: 'Example', version: '1.0.0', images: {}, sdkVersion: 'test', gitHash: 'test' }))
} else if (args[0] === 's9pk' && args[1] === 'pack') {
  if (process.env.DYNAMIC_INGREDIENTS) {
    const ingredients = JSON.parse(fs.readFileSync('javascript/index.js', 'utf8')).ingredients
    if (ingredients.some(path => !fs.existsSync(path))) {
      console.error('Missing current ingredient')
      process.exit(1)
    }
  }
  process.exitCode = Number(process.env.PACK_STATUS || 0)
  if (!process.exitCode) {
    const output = args[args.indexOf('-o') + 1]
    fs.writeFileSync(output, JSON.stringify({
      arch: args.find(arg => arg.startsWith('--arch='))?.slice(7),
      variant: process.env.VARIANT || '',
      bundle: fs.readFileSync('javascript/index.js', 'utf8'),
    }))
  }
} else if (args[0] === 'package' && args[1] === 'install' && args[2] === '-s') {
  fs.copyFileSync(args[3], 'installed')
  process.exitCode = Number(process.env.INSTALL_STATUS || 0)
} else {
  console.error('Unexpected start-cli invocation: ' + args.join(' '))
  process.exitCode = 1
}
`,
  )
  chmodSync(join(bin, 'start-cli'), 0o755)
  return {
    root,
    make(env = {}, args = ['install']) {
      return spawnSync('make', ['--no-print-directory', ...args], {
        cwd: root,
        encoding: 'utf8',
        env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, ...env },
      })
    },
    calls() {
      return readFileSync(join(root, 'calls'), 'utf8')
        .trim()
        .split('\n')
        .map(line => JSON.parse(line))
    },
    installed() {
      return JSON.parse(readFileSync(join(root, 'installed'), 'utf8'))
    },
    stale(name) {
      utimesSync(join(root, name), new Date(0), new Date(0))
    },
  }
}

function succeeds(result) {
  assert.equal(result.status, 0, result.stdout + result.stderr)
}

for (const arch of ['x86_64', 'aarch64', 'riscv64']) {
  test(`install builds only the server's ${arch} architecture from a clean package`, t => {
    const f = fixture(t)
    succeeds(f.make({ ARCH: arch }))
    assert.deepEqual(f.installed(), {
      arch,
      variant: '',
      bundle: 'original source',
    })
    assert.deepEqual(
      readdirSync(f.root).filter(name => name.endsWith('.s9pk')),
      [`example_${arch}.s9pk`],
    )
    assert.deepEqual(f.calls().at(-1), [
      'package',
      'install',
      '-s',
      `example_${arch}.s9pk`,
    ])
  })
}

test('install reuses an up-to-date artifact without repacking or rebundling', t => {
  const f = fixture(t)
  succeeds(f.make({}, ['example_aarch64.s9pk']))
  succeeds(f.make())
  assert.equal(f.calls().filter(args => args[1] === 'pack').length, 1)
  assert.equal(
    readFileSync(join(f.root, 'steps'), 'utf8'),
    'typecheck\nformat\nbundle\n',
  )
  assert.equal(f.calls().filter(args => args[1] === 'install').length, 1)
})

test('install rebuilds changed TypeScript through the existing build gates', t => {
  const f = fixture(t)
  succeeds(f.make({}, ['example_aarch64.s9pk']))
  f.stale('example_aarch64.s9pk')
  f.stale('javascript/index.js')
  writeFileSync(join(f.root, 'startos', 'index.ts'), 'changed source')
  succeeds(f.make())
  assert.equal(f.installed().bundle, 'changed source')
  assert.equal(f.calls().filter(args => args[1] === 'pack').length, 2)
  assert.equal(
    readFileSync(join(f.root, 'steps'), 'utf8'),
    'typecheck\nformat\nbundle\ntypecheck\nformat\nbundle\n',
  )
})

test('install repacks changed non-TypeScript ingredients without rebundling', t => {
  const f = fixture(t)
  succeeds(f.make({}, ['example_aarch64.s9pk']))
  f.stale('example_aarch64.s9pk')
  writeFileSync(join(f.root, 'Dockerfile'), 'FROM changed\n')
  succeeds(f.make())
  assert.equal(f.calls().filter(args => args[1] === 'pack').length, 2)
  assert.equal(
    readFileSync(join(f.root, 'steps'), 'utf8'),
    'typecheck\nformat\nbundle\n',
  )
})

for (const variantSource of ['default', 'environment', 'command line']) {
  test(`install builds and installs the ${variantSource} VARIANT, ignoring other artifacts`, t => {
    const f = fixture(t)
    for (const name of [
      'example.s9pk',
      variantSource === 'default'
        ? 'example_gpu_aarch64.s9pk'
        : 'example_aarch64.s9pk',
      'example_gpu_x86_64.s9pk',
    ]) {
      writeFileSync(join(f.root, name), 'unrelated artifact')
    }
    const env = variantSource === 'environment' ? { VARIANT: 'gpu' } : {}
    const args =
      variantSource === 'command line'
        ? ['install', 'VARIANT=gpu']
        : ['install']
    succeeds(f.make(env, args))
    assert.deepEqual(f.installed(), {
      arch: 'aarch64',
      variant: variantSource === 'default' ? '' : 'gpu',
      bundle: 'original source',
    })
    assert.deepEqual(f.calls().at(-1), [
      'package',
      'install',
      '-s',
      variantSource === 'default'
        ? 'example_aarch64.s9pk'
        : 'example_gpu_aarch64.s9pk',
    ])
  })
}

for (const [name, env] of [
  ['signing key initialization', { INIT_STATUS: '1' }],
  ['device lookup', { DEVICE_STATUS: '1' }],
  ['packing', { PACK_STATUS: '1' }],
]) {
  test(`${name} failure prevents installation`, t => {
    const f = fixture(t)
    assert.notEqual(f.make(env).status, 0)
    assert.equal(
      f.calls().some(args => args[1] === 'install'),
      false,
    )
  })
}

test('a failed rebuild does not install a stale artifact', t => {
  const f = fixture(t)
  succeeds(f.make({}, ['example_aarch64.s9pk']))
  f.stale('example_aarch64.s9pk')
  assert.notEqual(f.make({ PACK_STATUS: '1' }).status, 0)
  assert.equal(
    f.calls().some(args => args[1] === 'install'),
    false,
  )
})

test('installation failures propagate', t => {
  const f = fixture(t)
  assert.notEqual(f.make({ INSTALL_STATUS: '1' }).status, 0)
  assert.equal(
    f.calls().some(args => args[1] === 'install'),
    true,
  )
})

test('build gate failures prevent packing and installation', t => {
  const f = fixture(t)
  assert.notEqual(f.make({}, ['install', 'TS_CHECK=false']).status, 0)
  assert.equal(
    f.calls().some(args => ['pack', 'install'].includes(args[1])),
    false,
  )
})

for (const target of ['x86', 'universal']) {
  for (const removed of [
    'image/Dockerfile',
    'image',
    'dependency.svg',
    'dependency.s9pk',
  ]) {
    test(`${target} builds on the first attempt after removing ${removed} from the manifest`, t => {
      const f = fixture(t)
      const env = { DYNAMIC_INGREDIENTS: '1' }
      const output = target === 'x86' ? 'example_x86_64.s9pk' : 'example.s9pk'
      mkdirSync(join(f.root, 'image'))
      writeFileSync(join(f.root, 'image', 'Dockerfile'), 'FROM scratch\n')
      writeFileSync(join(f.root, 'dependency.svg'), 'synthetic icon')
      writeFileSync(join(f.root, 'dependency.s9pk'), 'synthetic dependency')
      writeFileSync(
        join(f.root, 'startos', 'index.ts'),
        JSON.stringify({ ingredients: [removed] }),
      )
      succeeds(f.make(env, [target]))
      f.stale('javascript/index.js')
      f.stale(output)
      rmSync(join(f.root, removed), { recursive: true })
      const updated = JSON.stringify({ ingredients: [] })
      writeFileSync(join(f.root, 'startos', 'index.ts'), updated)
      succeeds(f.make(env, [target]))
      assert.equal(
        JSON.parse(readFileSync(join(f.root, output), 'utf8')).bundle,
        updated,
      )
      assert.equal(f.calls().filter(args => args[1] === 'pack').length, 2)
      succeeds(f.make(env, [target]))
      assert.equal(f.calls().filter(args => args[1] === 'pack').length, 2)
      assert.equal(
        readFileSync(join(f.root, 'steps'), 'utf8'),
        'typecheck\nformat\nbundle\ntypecheck\nformat\nbundle\n',
      )
    })
  }
}

for (const target of ['x86', 'universal']) {
  test(`${target} fails when the current manifest still requires a missing ingredient`, t => {
    const f = fixture(t)
    const env = { DYNAMIC_INGREDIENTS: '1' }
    writeFileSync(
      join(f.root, 'startos', 'index.ts'),
      JSON.stringify({ ingredients: ['Dockerfile'] }),
    )
    succeeds(f.make(env, [target]))
    f.stale(target === 'x86' ? 'example_x86_64.s9pk' : 'example.s9pk')
    rmSync(join(f.root, 'Dockerfile'))
    assert.notEqual(f.make(env, [target]).status, 0)
  })
}

for (const target of ['x86', 'universal']) {
  test(`${target} fails closed on a partial ingredient-list failure`, t => {
    const f = fixture(t)
    succeeds(f.make({}, [target]))
    const packs = f.calls().filter(args => args[1] === 'pack').length
    const result = f.make({ LIST_STATUS: '9' }, [target])
    assert.notEqual(result.status, 0)
    assert.match(result.stderr, /list-ingredients.*failed/)
    assert.equal(f.calls().filter(args => args[1] === 'pack').length, packs)
  })
}
