import assert from 'node:assert/strict'
import test from 'node:test'
import Mustache from 'mustache'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(new Map())
const { renderDisplayAs } = await import('src/app/utils/display-as')

test('displayAs preserves punctuation and literal markup in input values', () => {
  for (const name of [
    "DrBoneZ's Registry",
    'Alice & Bob <registry> "main" / `backup` = ready',
    '<img src=x onerror=alert(1)>',
    'Literal &#39; and &amp;',
  ]) {
    assert.equal(renderDisplayAs('{{name}}', { name }), name)
  }
})

test('displayAs retains Mustache sections, nested fields and missing values', () => {
  assert.equal(
    renderDisplayAs(
      '{{#registry}}{{name}}{{/registry}}{{^registry}}None{{/registry}} {{missing}}',
      { registry: { name: "DrBoneZ's Registry" } },
    ),
    "DrBoneZ's Registry ",
  )
  assert.equal(renderDisplayAs('{{registry.name}}', {}), '')
  assert.equal(renderDisplayAs('', {}), '')
})

test('plain-text displayAs does not change Mustache HTML escaping elsewhere', () => {
  renderDisplayAs('{{name}}', { name: '<registry>' })
  assert.equal(
    Mustache.render('{{name}}', { name: '<registry>' }),
    '&lt;registry&gt;',
  )
})
