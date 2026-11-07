import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { FormControl } from '@angular/forms'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(
  new Map([
    [
      '@taiga-ui/kit',
      'export const tuiCreateFileFormatValidator = () => () => null',
    ],
  ]),
)
const { datetimeMax, textLengthInRange, listUnique } =
  await import('src/app/services/form.service')

test('maximum datetime violations use their own validation key', () => {
  const validate = datetimeMax('12:00')
  assert.deepEqual(validate(new FormControl('12:01')), {
    datetimeMax: 'Maximum is 12:00',
  })
  assert.equal(validate(new FormControl('12:00')), null)
  assert.equal(validate(new FormControl('11:59')), null)
  assert.equal(validate(new FormControl(null)), null)
})

test('maximum text length reports a grammatical error message', () => {
  const validate = textLengthInRange(null, 3)
  assert.deepEqual(validate(new FormControl('four')), {
    listNotInRange: 'Cannot be greater than 3 characters',
  })
  assert.equal(validate(new FormControl('abc')), null)
})

test('duplicate-entry messages HTML-escape displayAs labels', () => {
  const validate = listUnique({
    spec: {
      type: 'object',
      displayAs: '{{name}}',
      uniqueBy: 'name',
      spec: { name: { type: 'text', name: 'Registry Name' } },
    },
  })
  assert.deepEqual(
    validate(
      new FormControl([
        { name: "<img src=x> DrBoneZ's & Friends" },
        { name: "<img src=x> DrBoneZ's & Friends" },
      ]),
    ),
    {
      listNotUnique:
        '"&lt;img src&#x3D;x&gt; DrBoneZ&#39;s &amp; Friends" and "&lt;img src&#x3D;x&gt; DrBoneZ&#39;s &amp; Friends" are not unique. Must be unique by: Registry Name',
    },
  )
  assert.equal(
    validate(new FormControl([{ name: 'First' }, { name: 'Second' }])),
    null,
  )
})
