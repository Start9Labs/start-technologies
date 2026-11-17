#!/usr/bin/env node
// Prints which products a set of changed paths rebuilds, as `<key>=true|false` lines for
// $GITHUB_OUTPUT plus `products=<space-separated list>`. Paths arrive one per line on stdin.
//
// Usage: changed-products.mjs --event push|pull_request
//        changed-products.mjs --all

import { readFileSync } from 'node:fs'
import { pathToFileURL } from 'node:url'

// Mirrors the source inputs in build/common.mk's CORE_SRC.
const CORE_SOURCE_PATHS = [
  'shared-libs/crates/**',
  'projects/start-os/build/lib/scripts/**',
  'projects/start-os/build/dpkg-deps/uefi.platforms',
]

const WORKFLOW_INPUTS = [
  '.github/workflows/build-products.yaml',
  '.github/scripts/changed-products.mjs',
  '.github/actions/setup-build/**',
]

// Each product's build inputs, mirroring its build.mk (root AGENTS.md, "Coupled changes").
// Patterns follow GitHub's `paths:` filters: the last matching pattern decides, and `!`
// excludes. `release` paths count for pushes only (root AGENTS.md, on metadata-only
// master changes).
export const PRODUCTS = {
  'start-os': {
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
      ...WORKFLOW_INPUTS,
      '.github/workflows/startos-iso.yaml',
      '.github/workflows/startos-image.yaml',
      '!**/*.md',
      'projects/start-os/release-notes/**',
      'projects/start-os/changelog/**',
    ],
    release: [],
  },
  'start-cli': {
    paths: [
      'projects/start-cli/**',
      ...CORE_SOURCE_PATHS,
      'Cargo.toml',
      'Cargo.lock',
      'build/**',
      'debian/**',
      ...WORKFLOW_INPUTS,
      '!**/*.md',
    ],
    release: releaseMetadata('start-cli'),
  },
  'start-registry': {
    paths: [
      'projects/start-registry/**',
      ...CORE_SOURCE_PATHS,
      'Cargo.toml',
      'Cargo.lock',
      'build/**',
      'debian/**',
      ...WORKFLOW_INPUTS,
      '!**/*.md',
    ],
    release: releaseMetadata('start-registry'),
  },
  'start-tunnel': {
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
      ...WORKFLOW_INPUTS,
      '!**/*.md',
    ],
    release: releaseMetadata('start-tunnel'),
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
  'scripts/changelog*.mjs',
  'shared-libs/crates/start-core/locales/release-notes.json',
  '.prettierrc.json',
  'package.json',
  '.github/workflows/build-products.yaml',
  '.github/workflows/startos-iso.yaml',
  '.github/workflows/startos-image.yaml',
  '.github/actions/setup-build/**',
]

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

// The products, and whether the StartOS images, that the changed paths rebuild.
export function classify(paths, event) {
  const affected = Object.entries(PRODUCTS)
    .filter(([, { paths: inputs, release }]) => {
      const patterns = event === 'push' ? [...inputs, ...release] : inputs
      return paths.some(path => matches(patterns, path))
    })
    .map(([product]) => product)
  const osImage =
    affected.includes('start-os') &&
    paths.some(path => matches(OS_IMAGE_PATHS, path))
  return { affected, osImage }
}

function output({ affected, osImage }) {
  return [
    ...Object.keys(PRODUCTS).map(
      product => `${product}=${affected.includes(product)}`,
    ),
    `os-image=${osImage}`,
    `products=${affected.join(' ')}`,
  ].join('\n')
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [flag, event] = process.argv.slice(2)
  if (flag === '--all') {
    console.log(output({ affected: Object.keys(PRODUCTS), osImage: true }))
  } else if (flag === '--event' && ['push', 'pull_request'].includes(event)) {
    const paths = readFileSync(0, 'utf8').split('\n').filter(Boolean)
    console.log(output(classify(paths, event)))
  } else {
    console.error(
      'usage: changed-products.mjs --event push|pull_request | --all',
    )
    process.exit(2)
  }
}
