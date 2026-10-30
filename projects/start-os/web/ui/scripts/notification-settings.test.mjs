import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import {
  createEnvironmentInjector,
  runInInjectionContext,
  signal,
} from '@angular/core'
import { FormControl } from '@angular/forms'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(
  new Map([
    [
      '@start9labs/shared',
      'export class TaskService {} export class i18nPipe {}',
    ],
    ['@taiga-ui/core', 'export class TuiCell {} export class TuiTitle {}'],
    ['@taiga-ui/kit', 'export class TuiSwitch {}'],
    ['@taiga-ui/layout', 'export class TuiHeader {}'],
    [
      '@taiga-ui/polymorpheus',
      'export const injectContext = () => ({ id: "settings" })',
    ],
    ['src/app/services/push.service', 'export class PushService {}'],
  ]),
)
const { NotificationSettings } =
  await import('src/app/routes/portal/routes/notifications/settings')
const { PushService } = await import('src/app/services/push.service')
const { TaskService } = await import('@start9labs/shared')

for (const { name, operation, initial, failure, observed } of [
  { name: 'enables push', operation: 'enable', initial: false, observed: true },
  {
    name: 'disables push',
    operation: 'disable',
    initial: true,
    observed: false,
  },
  {
    name: 'restores off after permission denial',
    operation: 'enable',
    initial: false,
    failure: true,
    observed: false,
  },
  {
    name: 'restores on after native unsubscribe failure',
    operation: 'disable',
    initial: true,
    failure: true,
    observed: true,
  },
  {
    name: 'stays off after server cleanup failure',
    operation: 'disable',
    initial: true,
    failure: true,
    observed: false,
  },
]) {
  test(`${name} without emitting another ngModelChange`, async t => {
    const enabled = signal(initial)
    const requests = []
    const errors = []
    const perform = async operation => {
      requests.push(operation)
      enabled.set(observed)
      if (failure) throw new Error(name)
    }
    const injector = createEnvironmentInjector([
      {
        provide: PushService,
        useValue: {
          enabled,
          enable: () => perform('enable'),
          disable: () => perform('disable'),
        },
      },
      {
        provide: TaskService,
        useValue: {
          run: async operation => {
            try {
              await operation()
            } catch (error) {
              errors.push(error)
            }
          },
        },
      },
    ])
    t.after(() => injector.destroy())
    const settings = runInInjectionContext(
      injector,
      () => new NotificationSettings(),
    )
    const control = new FormControl(!initial)
    let displayed = !initial
    control.registerOnChange((value, emitViewToModelChange) => {
      assert.equal(emitViewToModelChange, false)
      displayed = value
    })
    await settings.toggle({ control, reset: value => control.reset(value) })
    assert.deepEqual(requests, [operation])
    assert.equal(displayed, observed)
    assert.equal(control.value, observed)
    assert.equal(errors.length, failure ? 1 : 0)
  })
}
