import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { FormBuilder } from '@angular/forms'
import { loadTypeScript } from './typescript-loader.mjs'

const classes = names => names.map(name => `export class ${name} {}`).join('\n')
loadTypeScript(
  new Map([
    [
      '@taiga-ui/cdk',
      classes(['TuiValueChanges']) +
        '\nexport const tuiMarkControlAsTouchedAndValidate = () => {};',
    ],
    ['@taiga-ui/core', classes(['TuiButton', 'TuiNotification'])],
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
  ]),
)
const { FormComponent } =
  await import('src/app/routes/portal/components/form.component')

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
