import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import yaml from 'js-yaml'
import { REPO, fixture } from '../../../scripts/tests/fixtures.mjs'

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

  for (const [name, attached, s3, linked] of [
    ['all attached', ['arm.s9pk', 'x86.s9pk'], 'https://s3.invalid/', []],
    ['mixed assets', ['x86.s9pk'], 'https://s3.invalid/', ['arm.s9pk']],
    ['all S3-only', [], 'https://s3.invalid/', ['arm.s9pk', 'x86.s9pk']],
    ['S3 disabled', ['x86.s9pk'], '', []],
  ]) {
    test(`${path}: release body links only S3-only packages (${name})`, t => {
      const f = fixture(t)
      f.write('arm.s9pk', '')
      f.write('x86.s9pk', '')
      f.write('.gh_release_files', attached.join('\n'))
      f.nodeExecutable(
        'start-cli',
        'console.log(JSON.stringify({ releaseNotes: "Fixture notes" }))',
      )
      const values = {
        'inputs.S3_S9PKS_BASE_URL': s3,
        'vars.S3_S9PKS_BASE_URL': s3,
        'steps.version.outputs.package_id': 'fixture',
        'steps.version.outputs.version': '#1.0.0:0',
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
      assert.equal(body.includes('## Downloads'), linked.length > 0)
      for (const file of ['arm.s9pk', 'x86.s9pk']) {
        assert.equal(
          body.includes(
            `[${file}](https://s3.invalid/fixture/1.0.0:0/${file})`,
          ),
          linked.includes(file),
        )
      }
    })
  }
}
