import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import { readFileSync, existsSync } from 'node:fs'
import { test } from 'node:test'

const require = createRequire(import.meta.url)
const yaml = require('js-yaml')
const {
  inlinePackageWorkflow,
} = require('../../../scripts/inline-package-workflow.cjs')
const { generate } = require('../../../scripts/generate-forgejo-workflows.cjs')
const root = fileURLToPath(new URL('../../../', import.meta.url))
const outputs = await generate(root)
const directory = 'projects/start-sdk/docs/gitea-workflows'
const document = name => yaml.load(outputs.get(`${directory}/${name}.yml`))

test('Gitea workflows are opt-in copies rather than active package-template files', () => {
  assert.equal(
    existsSync(`${root}/projects/start-sdk/docs/package-template/.gitea`),
    false,
  )
  for (const filename of [
    'build',
    'pr-retarget',
    'release',
    'tagAndRelease',
    'syncNext',
  ]) {
    assert.ok(outputs.has(`${directory}/${filename}.yml`))
    const source = yaml.load(
      readFileSync(
        `${root}/projects/start-sdk/docs/package-template/.github/workflows/${filename}.yml`,
        'utf8',
      ),
    )
    assert.deepEqual(document(filename).on, source.on)
    if (source.concurrency && filename !== 'pr-retarget')
      assert.deepEqual(document(filename).concurrency, source.concurrency)
    const forgejo = yaml.load(
      outputs.get(
        `projects/start-sdk/docs/package-template/.forgejo/workflows/${filename}.yml`,
      ),
    )
    assert.deepEqual(forgejo.concurrency, source.concurrency)
    assert.deepEqual(
      Object.values(forgejo.jobs).map(job => job.if),
      Object.values(source.jobs).map(job => job.if),
    )
  }
})

test('Gitea jobs are self-contained and retain their dependency graph', () => {
  for (const filename of [
    'build',
    'pr-retarget',
    'release',
    'tagAndRelease',
    'syncNext',
  ]) {
    const workflow = document(filename)
    for (const job of Object.values(workflow.jobs)) {
      assert.equal(job.uses, undefined)
      assert.equal(job['runs-on'], 'ubuntu-latest')
      for (const dependency of [job.needs || []].flat())
        assert.ok(workflow.jobs[dependency], dependency)
      for (const match of JSON.stringify(job).matchAll(/needs\.([\w-]+)\./g))
        assert.ok(workflow.jobs[match[1]], match[1])
    }
    assert.doesNotMatch(JSON.stringify(workflow), /\binputs\./)
  }
  assert.deepEqual(Object.keys(document('release').jobs), [
    'release_DiscoverMatrix',
    'release_BuildMatrix',
    'release_Publish',
  ])
  assert.deepEqual(document('release').jobs.release_BuildMatrix.needs, [
    'release_DiscoverMatrix',
  ])
  assert.equal(document('syncNext').concurrency['cancel-in-progress'], false)
})

test('PR builds withhold signing secrets and Gitea edits retain draft gating', () => {
  for (const job of Object.values(document('build').jobs)) {
    assert.match(
      job.if,
      /github\.event_name != 'pull_request' \|\| github\.event\.pull_request\.draft == false/,
    )
    assert.equal(job.permissions.contents, 'read')
    assert.doesNotMatch(JSON.stringify(job), /secrets\./)
  }
  const edits = document('pr-retarget')
  assert.equal(edits.name, 'Rebuild after PR edit')
  assert.equal(
    edits.concurrency.group,
    "package-build-${{ github.event.changes.ref && github.event.pull_request.number || format('metadata-{0}', github.event.pull_request.number) }}",
  )
  for (const job of Object.values(edits.jobs)) {
    assert.match(job.if, /github\.event\.pull_request\.draft == false/)
    assert.doesNotMatch(job.if, /changes\./)
    assert.doesNotMatch(JSON.stringify(job), /secrets\./)
  }
})

test('nested release jobs use the tag output and all remain gated by the registry check', () => {
  const workflow = document('tagAndRelease')
  for (const id of [
    'tag_Release_DiscoverMatrix',
    'tag_Release_BuildMatrix',
    'tag_Release_Publish',
  ]) {
    const job = workflow.jobs[id]
    assert.ok(job.needs.includes('tag_Tag'))
    assert.match(job.if, /needs\.tag_Tag\.outputs\.skip == 'false'/)
    const checkout = job.steps.find(
      step => step.uses === 'https://github.com/actions/checkout@v6',
    )
    assert.match(checkout.with.ref, /needs\.tag_Tag\.outputs\.tag/)
  }
  assert.deepEqual(workflow.jobs.tag_Release_BuildMatrix.needs, [
    'tag_Tag',
    'tag_Release_DiscoverMatrix',
  ])
  const publish = workflow.jobs.tag_Release_Publish
  assert.deepEqual(publish.needs, ['tag_Tag', 'tag_Release_BuildMatrix'])
  assert.match(JSON.stringify(publish), /secrets\.DEV_KEY/)
  assert.match(JSON.stringify(publish), /vars\.RELEASE_REGISTRY/)
  assert.match(JSON.stringify(publish), /vars\.S3_S9PKS_BASE_URL/)
})

test('Gitea reuses Forgejo-compatible artifact and release primitives', () => {
  const publish = document('release').jobs.release_Publish
  assert.ok(
    publish.steps.some(
      step =>
        step.uses === 'https://data.forgejo.org/forgejo/download-artifact@v4',
    ),
  )
  const release = publish.steps.find(step =>
    step.uses?.endsWith('/.github/actions/forgejo-release@master'),
  )
  assert.equal(release.with.token, '${{ github.token }}')
  assert.ok(
    publish.steps.some(
      step =>
        step.uses === 'https://data.forgejo.org/forgejo/upload-artifact@v4',
    ),
  )
})

test('inlining binds supplied values, omitted secrets, defaults, guards, and scoped outputs', () => {
  const result = inlinePackageWorkflow(
    {
      on: { push: null },
      jobs: {
        package: {
          uses: 'shared',
          if: "github.ref_name == 'master'",
          with: { VALUE: '${{ vars.VALUE }}' },
          secrets: { KEY: '${{ secrets.SIGNING_KEY }}' },
          permissions: { contents: 'read' },
        },
      },
    },
    reference => {
      assert.equal(reference, 'shared')
      return {
        on: {
          workflow_call: {
            inputs: {
              VALUE: { type: 'string' },
              ENABLED: { type: 'boolean', default: false },
              LABEL: { type: 'string', default: "a'b" },
            },
          },
        },
        jobs: {
          discover: {
            'runs-on': 'ubuntu-latest',
            outputs: { value: '${{ steps.read.outputs.value }}' },
            steps: [],
          },
          build: {
            'runs-on': 'ubuntu-latest',
            needs: 'discover',
            if: '${{ inputs.ENABLED }}',
            steps: [
              {
                run: '${{ inputs.VALUE }}|${{ inputs.LABEL }}|${{ secrets.KEY }}|${{ secrets.OMITTED }}|${{ needs.discover.outputs.value }}',
              },
            ],
          },
        },
      }
    },
  )
  assert.deepEqual(result.jobs.package_build.needs, ['package_discover'])
  assert.equal(
    result.jobs.package_build.if,
    "${{ (github.ref_name == 'master') && ((false)) }}",
  )
  assert.equal(
    result.jobs.package_build.steps[0].run,
    "${{ (vars.VALUE) }}|${{ ('a''b') }}|${{ (secrets.SIGNING_KEY) }}|${{ ('') }}|${{ needs.package_discover.outputs.value }}",
  )
  assert.equal(result.jobs.package_discover.permissions.contents, 'read')
})
