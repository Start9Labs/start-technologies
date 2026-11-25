import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { signal } from '@angular/core'
import { FormControl, FormGroup } from '@angular/forms'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(
  new Map([
    [
      '@start9labs/shared',
      `export class DialogService {}
       export class DocsLinkDirective {}
       export const escapeHtml = value => value
       export class i18nPipe {}
       export class TaskService {}`,
    ],
    [
      '@start9labs/start-core',
      'export const ISB = {}; export const inputSpec = {}; export const utils = {}',
    ],
    [
      '@taiga-ui/core',
      `export class TuiButton {}
       export class TuiError {}
       export class TuiInput {}
       export class TuiLink {}
       export class TuiTitle {}`,
    ],
    ['@taiga-ui/layout', 'export class TuiHeader {}'],
    [
      'src/app/routes/portal/components/form/containers/group.component',
      'export class FormGroupComponent {}',
    ],
    ['src/app/services/api/embassy-api.service', 'export class ApiService {}'],
    [
      'src/app/services/form.service',
      'export class FormService {}; export const trim = () => {}',
    ],
    ['src/app/services/title.service', 'export class TitleDirective {}'],
    [
      'src/app/utils/configBuilderToSpec',
      'export const configBuilderToSpec = value => value',
    ],
  ]),
)
const { default: Smtp } =
  await import('src/app/routes/portal/routes/system/routes/smtp/smtp.component')

for (const fails of [false, true]) {
  const outcome = fails
    ? 'preserves failed edits'
    : 'resets without a public update'
  test(`password-only save ${outcome}`, async () => {
    const password = new FormControl('replacement-secret')
    const form = new FormGroup({
      smtp: new FormGroup({
        selection: new FormControl('enabled'),
        value: new FormGroup({
          provider: new FormGroup({
            selection: new FormControl('other'),
            value: new FormGroup({
              host: new FormControl('smtp.example.com'),
              from: new FormControl('a@example.com'),
              username: new FormControl('a'),
              password,
              security: new FormGroup({
                selection: new FormControl('tls'),
                value: new FormGroup({ port: new FormControl('465') }),
              }),
            }),
          }),
        }),
      }),
    })
    password.markAsDirty()
    const requests = []
    const page = Object.assign(Object.create(Smtp.prototype), {
      removePassword: signal(false),
      api: {
        rpc: {
          server: {
            setSmtp: async params => {
              requests.push(params)
              if (fails) throw new Error('save failed')
            },
          },
        },
      },
      tasks: {
        run: async operation => {
          try {
            await operation()
            return true
          } catch {
            return false
          }
        },
      },
    })

    await page.save(form)

    assert.equal(requests.length, 1)
    assert.equal(requests[0].password, 'replacement-secret')
    assert.equal(requests[0].keepPassword, true)
    assert.equal(password.value, fails ? 'replacement-secret' : null)
    assert.equal(form.pristine, !fails)
    assert.equal(form.value.smtp.value.provider.value.host, 'smtp.example.com')
  })
}
