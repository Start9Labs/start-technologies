import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { createEnvironmentInjector, runInInjectionContext } from '@angular/core'
import { Subject } from 'rxjs'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(
  new Map([
    [
      '@start9labs/shared',
      `export class DialogService {}
       export class DocsLinkDirective {}
       export class i18nPipe {}
       export class TaskService {}`,
    ],
    ['@start9labs/start-core', 'export const ISB = {}'],
    ['@taiga-ui/core', 'export class TuiButton {}'],
    ['src/app/services/api/embassy-api.service', 'export class ApiService {}'],
    [
      'src/app/services/form-dialog.service',
      'export class FormDialogService {}',
    ],
    ['src/app/services/title.service', 'export class TitleDirective {}'],
    [
      'src/app/routes/portal/components/form.component',
      'export class FormComponent {}',
    ],
    [
      'src/app/utils/configBuilderToSpec',
      'export const configBuilderToSpec = value => value',
    ],
    ['./table.component', 'export class SSHTableComponent {}'],
  ]),
)
const { PatchDB } = await import('patch-db-client')
const { DialogService, i18nPipe, TaskService } =
  await import('@start9labs/shared')
const { ApiService } = await import('src/app/services/api/embassy-api.service')
const { FormDialogService } =
  await import('src/app/services/form-dialog.service')
const { sshKeyRows } = await import('src/app/utils/ssh-keys')
const { default: Ssh } =
  await import('src/app/routes/portal/routes/system/routes/ssh/ssh.component')

const keyData =
  'AAAAC3NzaC1lZDI1NTE5AAAAIMcFf+jmv4w+tRbA1aXDvovqViM0NVRu86Hk5kcQItfI'
const fingerprint = '00:11:22:33:44:55:66:77:88:99:aa:bb:cc:dd:ee:ff'
const createdAt = '2020-01-01T00:00:00Z'
const stored = value => ({
  created_at: createdAt,
  updated_at: '2021-01-01T00:00:00Z',
  value,
})
const expected = hostname => ({
  fingerprint,
  alg: 'ssh-ed25519',
  hostname,
  createdAt,
})

for (const [name, options] of [
  ['bare key', ''],
  ['restricted key', 'restrict '],
  ['multiple options', 'from="192.0.2.0/24",no-pty '],
  ['quoted command', 'command="echo ssh-ed25519 AAAA decoy",no-pty '],
  [
    'escaped quotes',
    String.raw`command="printf \"ssh-rsa AAAA decoy\"",restrict `,
  ],
]) {
  test(`${name} keeps options out of the algorithm and hostname`, () => {
    const hostname = 'workstation  rack\tone'
    assert.deepEqual(
      sshKeyRows({
        [fingerprint]: stored(`${options}ssh-ed25519 ${keyData} ${hostname}`),
      }),
      [expected(hostname)],
    )
  })
}

test('empty collections and keys without comments remain displayable', () => {
  assert.deepEqual(sshKeyRows({}), [])
  for (const suffix of ['', ' ']) {
    assert.deepEqual(
      sshKeyRows({
        [fingerprint]: stored(`ssh-ed25519 ${keyData}${suffix}`),
      }),
      [expected('')],
    )
  }
})

test('an older cached snapshot stays loading until the SSH collection arrives', t => {
  const updates = new Subject()
  const patch = new PatchDB(updates)
  patch.start()
  updates.next([{ id: 7, value: { serverInfo: {} } }])
  const injector = createEnvironmentInjector([
    { provide: PatchDB, useValue: patch },
    ...[
      ApiService,
      TaskService,
      FormDialogService,
      i18nPipe,
      DialogService,
    ].map(provide => ({ provide, useValue: {} })),
  ])
  t.after(() => {
    injector.destroy()
    patch.stop()
  })
  const page = runInInjectionContext(injector, () => new Ssh())
  assert.equal(page.keys(), null)

  updates.next([
    {
      id: 8,
      value: {
        serverInfo: {},
        sshPubkeys: {
          [fingerprint]: stored(`restrict ssh-ed25519 ${keyData} workstation`),
        },
      },
    },
  ])
  assert.deepEqual(page.keys(), [expected('workstation')])
  updates.next([
    { id: 9, patch: [{ op: 'remove', path: `/sshPubkeys/${fingerprint}` }] },
  ])
  assert.deepEqual(page.keys(), [])
})
