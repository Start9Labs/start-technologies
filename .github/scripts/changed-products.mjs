#!/usr/bin/env node
// Prints which of a workflow's products a set of changed paths rebuilds, as `<key>=true|false`
// lines for $GITHUB_OUTPUT plus `products=<space-separated list>`. Paths arrive one per line on
// stdin.
//
// Usage: changed-products.mjs --workflow <file> --event push|pull_request
//        changed-products.mjs --workflow <file> --all

import { readFileSync } from 'node:fs'
import { pathToFileURL } from 'node:url'

// Mirrors the source inputs in build/common.mk's CORE_SRC.
const CORE_SOURCE_PATHS = [
  'shared-libs/crates/**',
  'projects/start-os/build/lib/scripts/**',
  'projects/start-os/build/dpkg-deps/uefi.platforms',
]

// Each product's build inputs, mirroring its build.mk (root AGENTS.md, "Coupled changes"), and
// the workflow that builds it. Patterns follow GitHub's `paths:` filters: the last matching
// pattern decides, and `!` excludes. `release` paths are publishing inputs that count for pushes
// only (root AGENTS.md, on metadata-only master changes).
export const PRODUCTS = {
  'start-os': {
    workflow: 'build-products.yaml',
    paths: [
      'scripts/changelog*.mjs',
      '.prettierrc.json',
      'projects/start-os/**',
      'shared-libs/**',
      'projects/start-sdk/**',
      'Cargo.toml',
      'Cargo.lock',
      'angular.json',
      'package.json',
      'package-lock.json',
      'tsconfig.json',
      'tsconfig.lib.json',
      'build/**',
      'debian/**',
      'apt/**',
      '.github/workflows/startos-iso.yaml',
      '.github/workflows/startos-image.yaml',
      '!**/*.md',
      'projects/start-os/release-notes/**',
      'projects/start-os/changelog/**',
    ],
    release: ['scripts/s3-put-once.sh'],
  },
  'start-cli': {
    workflow: 'build-products.yaml',
    paths: [
      'projects/start-cli/**',
      ...CORE_SOURCE_PATHS,
      'Cargo.toml',
      'Cargo.lock',
      'build/**',
      'debian/**',
      '!**/*.md',
    ],
    release: releaseMetadata('start-cli'),
  },
  'start-registry': {
    workflow: 'build-products.yaml',
    paths: [
      'projects/start-registry/**',
      ...CORE_SOURCE_PATHS,
      'Cargo.toml',
      'Cargo.lock',
      'build/**',
      'debian/**',
      '!**/*.md',
    ],
    release: releaseMetadata('start-registry'),
  },
  'start-tunnel': {
    workflow: 'build-products.yaml',
    paths: [
      'projects/start-tunnel/**',
      ...CORE_SOURCE_PATHS,
      'shared-libs/ts-modules/**',
      'Cargo.toml',
      'Cargo.lock',
      'angular.json',
      'package.json',
      'package-lock.json',
      'tsconfig.json',
      'tsconfig.lib.json',
      'build/**',
      'debian/**',
      'apt/**',
      '!**/*.md',
    ],
    release: releaseMetadata('start-tunnel'),
  },
  'start-wrt': {
    workflow: 'start-wrt.yaml',
    paths: [
      'projects/start-wrt/**',
      ...CORE_SOURCE_PATHS,
      'shared-libs/ts-modules/**',
      'Cargo.toml',
      'Cargo.lock',
      'angular.json',
      'package.json',
      'package-lock.json',
      'tsconfig.json',
      'tsconfig.lib.json',
      'build/**',
      '!**/*.md',
    ],
    release: [...releaseMetadata('start-wrt'), 'scripts/s3-put-once.sh'],
  },
}

// Image-assembly inputs: a pull request touching one of these also builds the StartOS images.
export const OS_IMAGE_PATHS = [
  'build/**',
  'debian/**',
  'apt/**',
  'projects/start-os/build/**',
  'projects/start-os/build.mk',
  'projects/start-os/debian/**',
  'projects/start-os/release-notes/**',
  'projects/start-os/changelog/**',
  'projects/start-os/*.service',
  'projects/start-os/*.slice',
  'projects/start-os/container-runtime/*.sh',
  'projects/start-os/container-runtime/*.service',
  'scripts/changelog*.mjs',
  'shared-libs/crates/start-core/locales/release-notes.json',
  '.prettierrc.json',
  'package.json',
  '.github/workflows/build-products.yaml',
  '.github/workflows/startos-iso.yaml',
  '.github/workflows/startos-image.yaml',
  '.github/actions/setup-build/**',
]

// A change to the workflow that builds a product, or to how it classifies, rebuilds the product.
function workflowInputs(workflow) {
  return [
    `.github/workflows/${workflow}`,
    '.github/scripts/changed-products.mjs',
    '.github/scripts/classify-event.sh',
    '.github/actions/setup-build/**',
  ]
}

function releaseMetadata(product) {
  return [
    `projects/${product}/release-notes/**`,
    `projects/${product}/changelog/**`,
    'scripts/changelog*.mjs',
    'scripts/manage-release.sh',
    '.prettierrc.json',
  ]
}

function globToRegExp(glob) {
  let source = ''
  for (let i = 0; i < glob.length; i++) {
    const char = glob[i]
    if (char === '*' && glob[i + 1] === '*') {
      i++
      if (glob[i + 1] === '/') {
        i++
        source += '(?:.*/)?'
      } else {
        source += '.*'
      }
    } else if (char === '*') {
      source += '[^/]*'
    } else {
      source += char.replace(/[.+?^${}()|[\]\\]/g, '\\$&')
    }
  }
  return new RegExp(`^${source}$`)
}

export function matches(patterns, path) {
  let included = false
  for (const pattern of patterns) {
    const excluded = pattern.startsWith('!')
    if (globToRegExp(excluded ? pattern.slice(1) : pattern).test(path))
      included = !excluded
  }
  return included
}

// The products a workflow builds; every product when no workflow is named.
export function productsOf(workflow) {
  return Object.keys(PRODUCTS).filter(
    product =>
      workflow === undefined || PRODUCTS[product].workflow === workflow,
  )
}

// The products, and whether the StartOS images, that the changed paths rebuild.
export function classify(paths, event, workflow) {
  const affected = productsOf(workflow).filter(product => {
    const { workflow: builder, paths: inputs, release } = PRODUCTS[product]
    const patterns = [
      ...workflowInputs(builder),
      ...inputs,
      ...(event === 'push' ? release : []),
    ]
    return paths.some(path => matches(patterns, path))
  })
  const osImage =
    affected.includes('start-os') &&
    paths.some(path => matches(OS_IMAGE_PATHS, path))
  return { affected, osImage }
}

function output(products, { affected, osImage }) {
  return [
    ...products.map(product => `${product}=${affected.includes(product)}`),
    `os-image=${osImage}`,
    `products=${affected.join(' ')}`,
  ].join('\n')
}

function parseArgs(args) {
  const options = {}
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--all') options.all = true
    else if (args[i] === '--event') options.event = args[++i]
    else if (args[i] === '--workflow') options.workflow = args[++i]
    else return null
  }
  return options
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const options = parseArgs(process.argv.slice(2))
  const products = productsOf(options?.workflow)
  if (
    options?.workflow === undefined ||
    products.length === 0 ||
    !(options.all || ['push', 'pull_request'].includes(options.event))
  ) {
    console.error(
      'usage: changed-products.mjs --workflow <file> (--event push|pull_request | --all)',
    )
    process.exit(2)
  }
  if (options.all) {
    console.log(
      output(products, {
        affected: products,
        osImage: products.includes('start-os'),
      }),
    )
  } else {
    const paths = readFileSync(0, 'utf8').split('\n').filter(Boolean)
    console.log(
      output(products, classify(paths, options.event, options.workflow)),
    )
  }
}
