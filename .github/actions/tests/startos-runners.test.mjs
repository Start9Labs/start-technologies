import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import yaml from 'js-yaml'

const workflow = yaml.load(
  readFileSync(
    new URL('../../workflows/startos-iso.yaml', import.meta.url),
    'utf8',
  ),
)
const image = workflow.jobs.image
const [standard, fast] = Array.from(
  image['runs-on'].matchAll(/fromJson\('([\s\S]*?)'\)/g),
  match => yaml.load(match[1]),
)
const architectures = yaml.load(
  image.env.ARCH.match(/fromJson\('([\s\S]*?)'\)/)[1],
)

for (const platform of ['riscv64', 'riscv64-nonfree']) {
  test(`${platform} assembles on supported Node hosts without changing the target`, () => {
    assert.equal(standard[platform], 'ubuntu-24.04-arm')
    assert.equal(fast[platform], 'amd64-fast')
    assert.equal(architectures[platform], 'riscv64')
  })
}
