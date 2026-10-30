import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import {
  createEnvironmentInjector,
  runInInjectionContext,
  signal,
} from '@angular/core'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(
  new Map([
    ['@start9labs/shared', 'export class i18nPipe {}'],
    ['@taiga-ui/core', 'export class TuiIcon {} export class TuiLink {}'],
    ['@taiga-ui/kit', 'export class TuiAvatar {} export class TuiLineClamp {}'],
    ['patch-db-client', 'export class PatchDB {}'],
    [
      '@angular/core/rxjs-interop',
      'import { of } from "rxjs"; export const toObservable = () => of(undefined); export const toSignal = () => undefined',
    ],
    [
      'src/app/routes/portal/components/pkg-icon',
      'export class PkgIconDirective {}',
    ],
    [
      'src/app/services/notification.service',
      'export class NotificationService {}',
    ],
  ]),
)
const { NotificationItemComponent } =
  await import('src/app/routes/portal/routes/notifications/item.component')
const { NotificationService } =
  await import('src/app/services/notification.service')
const { PatchDB } = await import('patch-db-client')

test('notification link lookup is cached until the notification input changes', t => {
  let lookups = 0
  const injector = createEnvironmentInjector([
    { provide: PatchDB, useValue: {} },
    {
      provide: NotificationService,
      useValue: {
        getLink() {
          lookups++
          return '/system/general'
        },
      },
    },
  ])
  t.after(() => injector.destroy())
  const item = runInInjectionContext(
    injector,
    () => new NotificationItemComponent(),
  )
  const notification = signal(undefined)
  item.notificationItem = notification
  assert.equal(item.link(), null)
  assert.equal(lookups, 0)

  const warning = {
    id: 1,
    packageId: null,
    createdAt: '2026-10-30T00:00:00Z',
    code: 3,
    level: 'warning',
    title: 'StartOS security update available',
    message: 'StartOS 0.4.1 includes security updates.',
    data: { kind: 'os-update-available' },
    seen: false,
  }
  notification.set(warning)
  assert.equal(item.link(), '/system/general')
  assert.equal(item.link(), '/system/general')
  assert.equal(lookups, 1)

  notification.set({ ...warning, id: 2 })
  assert.equal(item.link(), '/system/general')
  assert.equal(item.link(), '/system/general')
  assert.equal(lookups, 2)

  notification.set(undefined)
  assert.equal(item.link(), null)
  assert.equal(lookups, 2)
})
