import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { FormBuilder, Validators } from '@angular/forms'
import { loadTypeScript } from './typescript-loader.mjs'

const classes = names => names.map(name => `export class ${name} {}`).join('\n')
loadTypeScript(
  new Map([
    [
      '@taiga-ui/cdk',
      classes(['TuiValueChanges']) +
        '\nexport const tuiMarkControlAsTouchedAndValidate = () => {};',
    ],
    [
      '@taiga-ui/core',
      classes(['TuiButton', 'TuiNotification']) +
        '\nexport const TUI_DEFAULT_ERROR_MESSAGE = Symbol();',
    ],
    ['@taiga-ui/kit', classes(['TuiConfirmService'])],
    ['@taiga-ui/polymorpheus', 'export const injectContext = () => null;'],
    [
      'src/app/routes/portal/components/form/containers/group.component',
      classes(['FormGroupComponent']),
    ],
    [
      'src/app/routes/portal/components/form/containers/control.directive',
      classes(['InvalidService']),
    ],
    [
      'src/app/services/form.service',
      classes(['FormService']) + '\nexport const trim = () => {};',
    ],
    ['./array.component', classes(['FormArrayComponent'])],
    ['./control.component', classes(['FormControlComponent'])],
    ['./object.component', classes(['FormObjectComponent'])],
    ['./union.component', classes(['FormUnionComponent'])],
  ]),
)
const { FormComponent } =
  await import('src/app/routes/portal/components/form.component')
const { FormGroupComponent } =
  await import('../src/app/routes/portal/components/form/containers/group.component.ts')

test('recreating a dynamic inner form preserves the dialog discard obligation', () => {
  let dirty = true
  const confirm = {
    markAsPristine: () => {
      dirty = false
    },
    markAsDirty: () => {
      dirty = true
    },
  }
  const form = new FormBuilder().group({ name: ['Retained edit'] })
  const spec = { name: {} }
  const value = { name: 'Retained edit' }
  const model = Object.assign(Object.create(FormComponent.prototype), {
    confirm,
    spec,
    value,
    operations: [],
    formService: {
      createForm: (actualSpec, actualValue) => {
        assert.equal(actualSpec, spec)
        assert.equal(actualValue, value)
        return form
      },
    },
  })
  model.ngOnInit()
  assert.equal(model.form, form)
  assert.equal(dirty, true)
})

for (const op of ['add', 'replace']) {
  test(`${op} prefills cannot accept an acknowledgement`, async () => {
    const form = new FormBuilder().group({
      confirm: [false, Validators.requiredTrue],
      address: ['Original address'],
    })
    const model = Object.assign(Object.create(FormComponent.prototype), {
      spec: {
        confirm: { type: 'acknowledgement' },
        address: { type: 'text' },
      },
      operations: [
        { op, path: '/confirm', value: true },
        { op, path: '/address', value: 'Prefilled address' },
      ],
      formService: { createForm: () => form },
      invalidService: { scrollIntoView: () => {} },
    })
    model.ngOnInit()
    assert.deepEqual(form.value, {
      confirm: false,
      address: 'Prefilled address',
    })
    assert.equal(form.controls.confirm.pristine, true)
    assert.equal(form.controls.confirm.untouched, true)
    assert.equal(form.controls.address.dirty, true)
    assert.equal(form.controls.address.touched, true)

    let submissions = 0
    const submit = () => {
      submissions++
    }
    await model.onClick(submit)
    assert.equal(submissions, 0)
    form.controls.confirm.setValue(true)
    await model.onClick(submit)
    assert.equal(submissions, 1)
  })
}

test('field ordering puts acknowledgements after integer and string keys', () => {
  const entries = Object.entries({
    0: { type: 'acknowledgement' },
    1: { type: 'text' },
    confirm: { type: 'acknowledgement' },
    address: { type: 'text' },
    approve: { type: 'acknowledgement' },
  }).map(([key, value]) => ({ key, value }))
  const group = new FormGroupComponent()
  assert.deepEqual(
    entries.sort(group.fieldOrder).map(({ key }) => key),
    ['1', 'address', '0', 'confirm', 'approve'],
  )
})
