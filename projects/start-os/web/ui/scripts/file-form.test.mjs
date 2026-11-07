import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { createEnvironmentInjector, runInInjectionContext } from '@angular/core'
import { UntypedFormBuilder } from '@angular/forms'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(new Map())
const { FormService, numberInRange, isInteger } =
  await import('../src/app/services/form.service.ts')

const { wireguardConfigInput, readWireguardConfig } =
  await import('../src/app/routes/portal/routes/system/routes/gateways/config-input.ts')

test('gateway config forms retain browser-only selection and paste flows', async () => {
  const field = wireguardConfigInput({ transform: value => value })
  const built = await field.build({ effects: {}, prefill: null })
  assert.equal(built.spec.variants.select.spec.file.uploadId, null)
  assert.equal(built.spec.variants.select.spec.file.maxSize, 31_457_280)
  const contents = '[Interface]\nAddress = 10.0.0.2/32\n'
  const file = new File([contents], 'gateway.conf')
  const selected = built.validator.parse({
    selection: 'select',
    value: { file },
  })
  assert.equal(await readWireguardConfig(selected), contents)
  assert.equal(
    await readWireguardConfig({
      selection: 'paste',
      value: { file: contents },
    }),
    contents,
  )
})

function form(spec, value) {
  const injector = createEnvironmentInjector([
    { provide: UntypedFormBuilder, useFactory: () => new UntypedFormBuilder() },
  ])
  try {
    return runInInjectionContext(injector, () =>
      new FormService().createForm(
        {
          file: {
            type: 'file',
            uploadId: 'prepared-id',
            name: 'File',
            description: null,
            warning: null,
            extensions: [],
            maxSize: null,
            required: true,
            ...spec,
          },
        },
        { file: value },
      ),
    )
  } finally {
    injector.destroy()
  }
}

test('fresh file controls require a real selection and nullable controls permit none', () => {
  assert.equal(form({}).valid, false)
  assert.equal(form({ required: false }).valid, true)
  assert.equal(form({}, { name: 'forged.pem', path: 'forged' }).valid, false)
})

test('retrying retains browser Files and their form validity', () => {
  const file = new File(['certificate'], 'certificate.pem')
  const retry = form({ extensions: ['.pem'] }, file)
  assert.equal(retry.value.file, file)
  assert.equal(retry.valid, true)
})

test('file extension validation rejects the wrong format without rejecting large valid files', () => {
  assert.equal(
    form({ extensions: ['.pem'] }, new File(['x'], 'wrong.txt')).valid,
    false,
  )
  const file = new File([new Uint8Array(31_000_000)], 'large.pem')
  assert.equal(form({ extensions: ['.pem'] }, file).valid, true)
  assert.equal(form({}, new File(['x'], 'any.bin')).valid, true)
})

test('prepared byte limits include the boundary and support empty-file-only uploads', () => {
  assert.equal(form({ maxSize: 3 }, new File(['abc'], 'file.bin')).valid, true)
  assert.equal(
    form({ maxSize: 3 }, new File(['abcd'], 'file.bin')).valid,
    false,
  )
  assert.equal(form({ maxSize: 0 }, new File([], 'file.bin')).valid, true)
  assert.equal(form({ maxSize: 0 }, new File(['x'], 'file.bin')).valid, false)
})

test('extension filters match case-insensitive filename suffixes, including compound extensions', () => {
  assert.equal(
    form({ extensions: ['.pem'] }, new File(['x'], 'CERT.PEM')).valid,
    true,
  )
  assert.equal(
    form({ extensions: ['.tar.gz'] }, new File(['x'], 'backup.TAR.GZ')).valid,
    true,
  )
  assert.equal(
    form({ extensions: ['.tar.gz'] }, new File(['x'], 'backup.gz')).valid,
    false,
  )
})

test('numeric controls preserve zero defaults and share inclusive bounds with action validation', () => {
  const injector = createEnvironmentInjector([
    { provide: UntypedFormBuilder, useFactory: () => new UntypedFormBuilder() },
  ])
  try {
    const numbers = runInInjectionContext(injector, () =>
      new FormService().createForm({
        count: {
          type: 'number',
          name: 'Count',
          default: 0,
          required: true,
          integer: true,
          min: 0,
          max: null,
        },
      }),
    )
    assert.equal(numbers.value.count, 0)
    assert.equal(numbers.valid, true)
    assert.deepEqual(numberInRange(0, null)({ value: -1 }), {
      numberNotInRange: 'Number must be greater than or equal to 0',
    })
    assert.deepEqual(numberInRange(null, 0)({ value: 1 }), {
      numberNotInRange: 'Number must be less than or equal to 0',
    })
    assert.equal(numberInRange(0, 1)({ value: 0 }), null)
    assert.deepEqual(isInteger()({ value: 1.5 }), {
      numberNotInteger: 'Must be an integer',
    })
    assert.equal(isInteger()({ value: '2' }), null)
  } finally {
    injector.destroy()
  }
})
