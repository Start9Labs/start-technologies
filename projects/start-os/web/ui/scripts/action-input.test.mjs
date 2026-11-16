import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { createEnvironmentInjector, runInInjectionContext } from '@angular/core'
import { Observable, of } from 'rxjs'
import { loadTypeScript } from './typescript-loader.mjs'

const boundaries = new Map([
  [
    '@start9labs/shared',
    `
    export class DialogService {}
    export class TaskService {}
    export class i18nPipe {}
    export const getErrorMessage = error => error.message;
  `,
  ],
  ['@start9labs/start-core', 'export const utils = {};'],
  [
    'fast-json-patch',
    `import patch from '${import.meta.resolve('fast-json-patch')}'; export const compare = patch.compare;`,
  ],
  [
    '@taiga-ui/core',
    'export class TuiButton {}; export class TuiLoader {}; export class TuiNotification {};',
  ],
  [
    '@taiga-ui/kit',
    'export class TuiProgress {}; export class TuiNotificationMiddleService {};',
  ],
  [
    '@taiga-ui/polymorpheus',
    'export const injectContext = () => globalThis.actionContext; export class PolymorpheusComponent {};',
  ],
  ['patch-db-client', 'export class PatchDB {};'],
  [
    'src/app/routes/portal/components/form.component',
    'export class FormComponent {};',
  ],
  [
    'src/app/routes/portal/components/form/containers/control.directive',
    'export class InvalidService {};',
  ],
  [
    'src/app/routes/portal/components/pkg-icon',
    'export class PkgIconDirective {};',
  ],
  [
    'src/app/routes/portal/routes/services/modals/action-confirm.component',
    'export const ACTION_CONFIRM_MODAL = {};',
  ],
  [
    'src/app/routes/portal/routes/services/modals/action-success/action-success.page',
    'export class ActionSuccessPage {};',
  ],
  ['src/app/services/action.service', 'export class ActionService {};'],
  [
    'src/app/services/form-dialog.service',
    'export class FormDialogService {};',
  ],
  ['src/app/services/api/embassy-api.service', 'export class ApiService {};'],
  [
    'src/app/utils/get-package-data',
    `
    export const getAllPackages = async () => ({});
    export const getLiveTasks = () => [];
    export const getManifest = () => ({});
  `,
  ],
])
loadTypeScript(boundaries)
const { ActionInputModal } =
  await import('../src/app/routes/portal/routes/services/modals/action-input.component.ts')
const { ApiService } = await import('src/app/services/api/embassy-api.service')
const { ActionService } = await import('src/app/services/action.service')
const { ActionService: ActionRunner } =
  await import('../src/app/services/action.service.ts')
const { FormDialogService } =
  await import('src/app/services/form-dialog.service')
const { TuiNotificationMiddleService } = await import('@taiga-ui/kit')
const { DialogService, TaskService, i18nPipe } =
  await import('@start9labs/shared')
const { PatchDB } = await import('patch-db-client')

function deferred() {
  let resolve, reject
  const promise = new Promise((yes, no) => {
    resolve = yes
    reject = no
  })
  return { promise, resolve, reject }
}

async function modal(options = {}) {
  const events = []
  const errors = []
  const alerts = []
  let forms = 0
  globalThis.actionContext = {
    data: {
      pkgInfo: { id: 'service', title: 'Files', status: 'running' },
      actionInfo: { id: 'import', metadata: { warning: null } },
    },
    $implicit: {
      complete: () => {
        events.push(['closed'])
        options.onClose?.()
      },
    },
  }
  const injector = createEnvironmentInjector([
    {
      provide: ApiService,
      useValue: {
        rpc: {
          package: {
            action: {
              run: async () => {
                events.push(['run'])
                return options.run?.()
              },
              getInput: async () => {
                forms++
                events.push(['form', forms])
                return {
                  eventId: `event-${forms}`,
                  spec: {
                    first: { type: 'file', uploadId: `first-${forms}` },
                    second: {
                      type: 'file',
                      uploadId: options.duplicate
                        ? `first-${forms}`
                        : `second-${forms}`,
                    },
                    text: { type: 'text' },
                  },
                  value: null,
                }
              },
            },
          },
        },
        uploadFile: (id, file, config) => {
          events.push(['upload', id, file.name])
          return options.upload?.(id, file, config) ?? Promise.resolve()
        },
      },
    },
    {
      provide: ActionService,
      useValue: {
        execute: async (...args) => {
          events.push(['execute', ...args])
          await options.execute?.(...args)
        },
      },
    },
    {
      provide: TaskService,
      useValue: {
        runInline: async fn => {
          try {
            await fn()
            return true
          } catch (error) {
            errors.push(error)
            return false
          }
        },
      },
    },
    { provide: i18nPipe, useValue: { transform: key => key } },
    {
      provide: DialogService,
      useValue: {
        openAlert: message => {
          alerts.push(message)
          events.push(['alert'])
          return of(undefined)
        },
      },
    },
    {
      provide: TuiNotificationMiddleService,
      useValue: {
        open: () =>
          new Observable(() => {
            events.push(['loading'])
            return () => events.push(['loaded'])
          }),
      },
    },
    { provide: FormDialogService, useValue: {} },
    { provide: PatchDB, useValue: {} },
  ])
  const instance = runInInjectionContext(injector, () => new ActionInputModal())
  await Promise.resolve()
  return { instance, injector, events, errors, alerts, forms: () => forms }
}

test('submitting uploads every selected file before executing with tokens', async () => {
  const started = deferred()
  const response = deferred()
  const { instance, injector, events } = await modal({
    upload: async (id, file, config) => {
      config.progress(file.size, file.size)
      started.resolve()
      await response.promise
    },
  })
  try {
    const file = new File(['certificate'], 'cert.pem')
    const attachment = new File(['attachment'], 'notes.txt')
    const pending = instance.execute({
      first: file,
      second: attachment,
      text: 'keep',
    })
    await started.promise
    assert.equal(instance.submitting(), true)
    await instance.execute({ first: file, second: attachment, text: 'keep' })
    assert.equal(instance.uploads()[0].complete, false)
    assert.equal(
      events.some(([kind]) => kind === 'execute'),
      false,
    )
    response.resolve()
    await pending
    assert.deepEqual(events, [
      ['form', 1],
      ['upload', 'first-1', 'cert.pem'],
      ['upload', 'second-1', 'notes.txt'],
      ['closed'],
      [
        'execute',
        'service',
        'event-1',
        'import',
        { first: 'first-1', second: 'second-1', text: 'keep' },
      ],
    ])
    assert.equal(instance.uploads().length, 2)
    assert.equal(
      instance.uploads().every(upload => upload.complete),
      true,
    )
  } finally {
    injector.destroy()
  }
})

test('upload completion closes and destroys the form before a pending action runs', async () => {
  const started = deferred()
  const response = deferred()
  const options = {
    execute: async () => {
      started.resolve()
      await response.promise
    },
  }
  const { instance, injector, events, errors } = await modal(options)
  options.onClose = () => injector.destroy()
  try {
    const pending = instance.execute({
      first: new File(['certificate'], 'cert.pem'),
      second: null,
    })
    await started.promise
    assert.equal(injector.destroyed, true)
    assert.deepEqual(
      events.map(([kind]) => kind),
      ['form', 'upload', 'closed', 'execute'],
    )
    response.resolve()
    await pending
    assert.deepEqual(errors, [])
  } finally {
    if (!injector.destroyed) injector.destroy()
  }
})

test('submissions without uploads dismiss directly into normal action execution', async () => {
  const { instance, injector, events } = await modal()
  try {
    await instance.execute({ first: null, second: null, text: 'keep' })
    assert.deepEqual(
      events.map(([kind]) => kind),
      ['form', 'closed', 'execute'],
    )
    assert.deepEqual(instance.uploads(), [])
  } finally {
    injector.destroy()
  }
})

test('a failed upload gets fresh capabilities while preserving selected files and other input', async () => {
  const failure = new Error('Upload failed')
  const { instance, injector, events, errors, forms } = await modal({
    upload: async () => {
      throw failure
    },
  })
  try {
    const file = new File(['certificate'], 'cert.pem')
    await instance.execute({ first: file, second: null, text: 'keep' })
    assert.equal(forms(), 2)
    assert.equal(instance.res().spec.first.uploadId, 'first-2')
    assert.equal(instance.res().originalValue.first, file)
    assert.equal(instance.res().originalValue.text, 'keep')
    assert.equal(instance.submitting(), false)
    assert.deepEqual(instance.uploads(), [])
    assert.deepEqual(errors, [failure])
    assert.equal(
      events.some(([kind]) => kind === 'execute' || kind === 'closed'),
      false,
    )
  } finally {
    injector.destroy()
  }
})

test('failed action execution does not reload the dismissed form', async () => {
  const failure = new Error('Action failed')
  const options = {
    run: async () => {
      throw failure
    },
  }
  const { instance, injector, events, errors, alerts, forms } =
    await modal(options)
  const runner = runInInjectionContext(injector, () => new ActionRunner())
  options.execute = async (...args) => {
    assert.equal(await runner.execute(...args), undefined)
  }
  try {
    await instance.execute({
      first: new File(['certificate'], 'cert.pem'),
      second: null,
    })
    assert.equal(forms(), 1)
    assert.deepEqual(alerts, [failure.message])
    assert.deepEqual(errors, [])
    assert.deepEqual(
      events.map(([kind]) => kind),
      [
        'form',
        'upload',
        'closed',
        'execute',
        'loading',
        'run',
        'alert',
        'loaded',
      ],
    )
  } finally {
    injector.destroy()
  }
})

test('one prepared destination cannot receive two files in the same submission', async () => {
  const { instance, injector, events, errors } = await modal({
    duplicate: true,
  })
  try {
    await instance.execute({
      first: new File(['a'], 'a'),
      second: new File(['b'], 'b'),
    })
    assert.equal(
      events.some(([kind]) => kind === 'upload' || kind === 'execute'),
      false,
    )
    assert.match(errors[0].message, /own upload destination/)
  } finally {
    injector.destroy()
  }
})

test('closing during upload cancels the request and never executes the action', async () => {
  const started = deferred()
  const { instance, injector, events, errors, forms } = await modal({
    upload: (id, file, config) =>
      new Promise((resolve, reject) => {
        config.signal.addEventListener(
          'abort',
          () => reject(config.signal.reason),
          { once: true },
        )
        started.resolve()
      }),
  })
  const pending = instance.execute({
    first: new File(['a'], 'a'),
    second: null,
  })
  await started.promise
  injector.destroy()
  await pending
  assert.equal(forms(), 1)
  assert.deepEqual(errors, [])
  assert.equal(
    events.some(([kind]) => kind === 'execute'),
    false,
  )
})
