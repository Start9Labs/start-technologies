import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import yaml from 'js-yaml'
import { REPO, fixture } from '../../../scripts/tests/fixtures.mjs'

const s3 = 'https://s3.invalid/'
const packages =
  'https://forge.invalid/api/packages/owner/generic/fixture/1.0.0_0'
const files = ['arm.s9pk', 'x86.s9pk']
const s3Link = file => `[${file}](https://s3.invalid/fixture/1.0.0:0/${file})`
const packageLink = file => `[${file}](${packages}/${file})`

for (const path of [
  '.github/workflows/release.yml',
  '.forgejo/workflows/release.yml',
  'projects/start-sdk/docs/gitea-workflows/release.yml',
  'projects/start-sdk/docs/gitea-workflows/tagAndRelease.yml',
]) {
  const workflow = yaml.load(readFileSync(join(REPO, path), 'utf8'))
  const step = Object.values(workflow.jobs)
    .flatMap(job => job.steps ?? [])
    .find(step => step.name === 'Generate changelog')
  assert.ok(step, path)

  for (const [name, attached, hosted, s3Base, links] of [
    ['all attached', files, [], s3, []],
    ['mixed assets', ['x86.s9pk'], [], s3, [s3Link('arm.s9pk')]],
    ['all S3-only', [], [], s3, files.map(s3Link)],
    ['S3 disabled', ['x86.s9pk'], [], '', []],
    [
      'package registry',
      ['x86.s9pk'],
      ['arm.s9pk'],
      '',
      [packageLink('arm.s9pk')],
    ],
  ]) {
    test(`${path}: release body links only unattached packages (${name})`, t => {
      const f = fixture(t)
      for (const file of files) f.write(file, '')
      f.write('.gh_release_files', attached.join('\n'))
      f.write('.package_files', hosted.join('\n'))
      f.nodeExecutable(
        'start-cli',
        'console.log(JSON.stringify({ releaseNotes: "Fixture notes" }))',
      )
      const values = {
        'inputs.S3_S9PKS_BASE_URL': s3Base,
        'vars.S3_S9PKS_BASE_URL': s3Base,
        'steps.version.outputs.package_id': 'fixture',
        'steps.version.outputs.version': '#1.0.0:0',
        'steps.filter_s9pk.outputs.packages_base': packages,
      }
      const script = step.run.replace(/\$\{\{([\s\S]*?)\}\}/g, (_, source) => {
        const reference = source.replace(/[()\s]/g, '')
        assert.ok(reference in values, reference)
        return values[reference]
      })
      const result = f.run('bash', ['-e', '-o', 'pipefail', '-c', script], {
        encoding: 'utf8',
        env: { ...f.env, PATH: `${f.root}:${f.env.PATH}` },
      })
      assert.equal(result.status, 0, result.stderr)
      const body = readFileSync(join(f.root, 'change-log.txt'), 'utf8')
      assert.match(body, /Fixture notes/)
      assert.equal(body.includes('## Downloads'), links.length > 0)
      for (const link of [...files.map(s3Link), ...files.map(packageLink)]) {
        assert.equal(body.includes(link), links.includes(link), link)
      }
    })
  }
}
