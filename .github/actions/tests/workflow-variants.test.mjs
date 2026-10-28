import assert from 'node:assert/strict'
import { readFileSync, existsSync } from 'node:fs'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import { join } from 'node:path'
import { test } from 'node:test'

const require = createRequire(import.meta.url)
const yaml = require('js-yaml')
const {
  generate,
  generatedPaths,
} = require('../../../scripts/generate-forgejo-workflows.cjs')
const root = fileURLToPath(new URL('../../../', import.meta.url))
const outputs = await generate(root)
const document = file => yaml.load(outputs.get(file))
const template = 'projects/start-sdk/docs/package-template'

test('the generated tree contains no obsolete variants', () => {
  assert.deepEqual(generatedPaths(root).sort(), [...outputs.keys()].sort())
})

for (const [file, expected] of outputs) {
  test(`${file} is generated from its GitHub source`, () => {
    assert.equal(readFileSync(join(root, file), 'utf8'), expected)
  })
  test(`${file} qualifies every remote dependency and resolves first-party variants`, () => {
    const source = yaml.load(expected)
    const uses = [
      ...Object.values(source.jobs || {}).flatMap(job => [
        job.uses,
        ...(job.steps || []).map(step => step.uses),
      ]),
      ...(source.runs?.steps || []).map(step => step.uses),
    ].filter(Boolean)
    for (const reference of uses) {
      assert.match(reference, /^https:\/\//)
      const firstParty = reference.match(
        /^https:\/\/github.com\/Start9Labs\/start-technologies\/(.+)@master$/,
      )
      if (firstParty) {
        const file = firstParty[1].includes('/actions/')
          ? `${firstParty[1]}/action.yml`
          : firstParty[1]
        assert.ok(outputs.has(file) || existsSync(join(root, file)), reference)
      }
    }
  })
}

test('Forgejo callers use the Forgejo workflow variants without changing GitHub callers', () => {
  for (const [filename, target] of [
    ['build', 'build'],
    ['pr-retarget', 'build'],
    ['release', 'release'],
    ['tagAndRelease', 'tagAndRelease'],
    ['syncNext', 'syncNext'],
  ]) {
    const source = yaml.load(
      readFileSync(
        join(root, template, `.github/workflows/${filename}.yml`),
        'utf8',
      ),
    )
    const variant = document(`${template}/.forgejo/workflows/${filename}.yml`)
    assert.deepEqual(variant.on, source.on)
    assert.deepEqual(variant.concurrency, source.concurrency)
    const jobName = Object.keys(source.jobs)[0]
    assert.equal(
      source.jobs[jobName].uses,
      `Start9Labs/start-technologies/.github/workflows/${target}.yml@master`,
    )
    assert.equal(
      variant.jobs[jobName].uses,
      `https://github.com/Start9Labs/start-technologies/.forgejo/workflows/${target}.yml@master`,
    )
    assert.deepEqual(
      variant.jobs[jobName].secrets,
      source.jobs[jobName].secrets,
    )
  }
})

test('remote calls and their inner jobs select the x86 runner for emulated builds', () => {
  for (const text of outputs.values()) {
    for (const job of Object.values(yaml.load(text).jobs || {})) {
      assert.equal(job['runs-on'], 'ubuntu-latest')
    }
  }
})

test('Forgejo artifacts use compatible actions and preserve the build-to-publish handoff', () => {
  const release = document('.forgejo/workflows/release.yml')
  const upload = release.jobs.BuildMatrix.steps.find(
    step => step.name === 'Upload s9pks for Publish stage',
  )
  assert.equal(
    upload.uses,
    'https://data.forgejo.org/forgejo/upload-artifact@v4',
  )
  assert.equal(upload.with.name, "build-${{ matrix.target || 'default' }}")
  const download = release.jobs.Publish.steps.find(
    step => step.name === 'Collect s9pks from BuildMatrix',
  )
  assert.equal(
    download.uses,
    'https://data.forgejo.org/forgejo/download-artifact@v4',
  )
  assert.equal(download.with.pattern, 'build-*')
  assert.equal(download.with['merge-multiple'], true)
  const build = document('.forgejo/workflows/build.yml')
  assert.equal(
    build.jobs.BuildMatrix.steps.at(-1).uses,
    'https://data.forgejo.org/forgejo/upload-artifact@v4',
  )
  assert.equal(
    build.jobs.BuildMatrix.steps.at(-1).with.name,
    "s9pks-${{ matrix.target || 'default' }}",
  )
})

test('Forgejo releases use the in-place Forgejo publisher and caller credentials', () => {
  const release = document('.forgejo/workflows/release.yml')
  const publish = release.jobs.Publish.steps.find(
    step => step.name === 'Create Forgejo release',
  )
  assert.equal(
    publish.uses,
    'https://github.com/Start9Labs/start-technologies/.github/actions/forgejo-release@master',
  )
  assert.equal(publish.with.token, '${{ github.token }}')
  assert.equal(
    publish.with.files,
    '${{ steps.filter_s9pk.outputs.release_files }}',
  )
  const tagged = document('.forgejo/workflows/tagAndRelease.yml')
  assert.equal(
    tagged.jobs.Release.uses,
    'https://github.com/Start9Labs/start-technologies/.forgejo/workflows/release.yml@master',
  )
})
