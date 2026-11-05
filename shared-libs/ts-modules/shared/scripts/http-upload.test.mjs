import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import '@angular/compiler'
import { HttpEventType, HttpResponse } from '@angular/common/http'
import { Observable, Subject } from 'rxjs'
import ts from 'typescript'

registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('.') && context.parentURL?.endsWith('.ts')) {
      const file = new URL(`${specifier}.ts`, context.parentURL)
      if (existsSync(file)) return { url: file.href, shortCircuit: true }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (!url.endsWith('.ts')) return next(url, context)
    return {
      format: 'module',
      source: ts.transpileModule(readFileSync(new URL(url), 'utf8'), {
        compilerOptions: {
          target: ts.ScriptTarget.ES2022,
          module: ts.ModuleKind.ES2022,
          experimentalDecorators: true,
        },
      }).outputText,
      shortCircuit: true,
    }
  },
})
const { HttpService } = await import('../src/services/http.service.ts')
const { getErrorMessage } = await import('../src/services/error.service.ts')

test('transport error messages preserve structured failures and handle unknown thrown values', () => {
  assert.equal(getErrorMessage(new Error('Upload failed')), 'Upload failed')
  assert.equal(getErrorMessage({ code: 12, message: 'Disk full' }), 'Disk full')
  assert.equal(getErrorMessage('Cancelled'), 'Cancelled')
  assert.match(
    getErrorMessage({ code: 0, message: 'Network' }),
    /couldn't reach your server/,
  )
  for (const error of [undefined, null, 42, { message: 12 }]) {
    assert.equal(getErrorMessage(error), 'Unknown Error')
  }
})

function transport() {
  const events = new Subject()
  const requests = []
  const instance = {
    fullUrl: 'https://start.local',
    http: {
      request: (...args) => {
        requests.push(['fetch', ...args])
        return events
      },
    },
    xhr: {
      request: (...args) => {
        requests.push(['xhr', ...args])
        return events
      },
    },
  }
  return {
    instance,
    events,
    requests,
    request: options =>
      HttpService.prototype.httpRequest.call(instance, options),
  }
}

test('upload progress uses XHR and does not resolve until the server confirms the file', async () => {
  const { request, requests, events } = transport()
  const progress = []
  let completed = false
  const body = new Blob(['contents'])
  const pending = request({
    method: 'POST',
    url: '/rest/rpc/upload',
    body,
    headers: { 'ngsw-bypass': 'true' },
    uploadProgress: (loaded, total) => progress.push([loaded, total]),
  }).then(response => {
    completed = true
    return response
  })
  assert.equal(requests[0][0], 'xhr')
  assert.equal(requests[0][2], 'https://start.local/rest/rpc/upload')
  assert.equal(requests[0][3].body, body)
  assert.equal(requests[0][3].reportProgress, true)
  events.next({ type: HttpEventType.Sent })
  events.next({ type: HttpEventType.UploadProgress, loaded: 4 })
  events.next({ type: HttpEventType.UploadProgress, loaded: 8, total: 8 })
  await Promise.resolve()
  assert.equal(completed, false)
  const response = new HttpResponse({ status: 204 })
  events.next(response)
  assert.equal(await pending, response)
  assert.deepEqual(progress, [
    [4, undefined],
    [8, 8],
  ])
})

test('ordinary requests retain the configured HTTP backend and response type', async () => {
  const { request, requests, events } = transport()
  const pending = request({
    method: 'GET',
    url: '/rpc',
    responseType: 'arraybuffer',
  })
  assert.equal(requests[0][0], 'fetch')
  assert.equal(requests[0][3].responseType, 'arraybuffer')
  assert.equal(requests[0][3].reportProgress, false)
  const response = new HttpResponse({ body: new ArrayBuffer(1) })
  events.next(response)
  assert.equal(await pending, response)
})

test('aborting unsubscribes the in-flight request instead of merely ignoring its response', async () => {
  const { request, instance } = transport()
  let unsubscribed = false
  instance.xhr.request = () =>
    new Observable(() => () => {
      unsubscribed = true
    })
  const abort = new AbortController()
  const pending = request({
    method: 'POST',
    url: '/rest/rpc/upload',
    uploadProgress: () => {},
    signal: abort.signal,
  })
  abort.abort(new Error('cancelled'))
  await assert.rejects(pending)
  assert.equal(unsubscribed, true)
})

test('an already aborted request does not reach either HTTP backend', async () => {
  const { request, requests } = transport()
  const abort = new AbortController()
  abort.abort()
  await assert.rejects(
    request({ method: 'POST', url: '/rest/rpc/upload', signal: abort.signal }),
  )
  assert.deepEqual(requests, [])
})
