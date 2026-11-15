import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import {
  HttpClient,
  HttpErrorResponse,
  HttpEventType,
  HttpHeaders,
  HttpResponse,
  HttpXhrBackend,
} from '@angular/common/http'
import { Injector, runInInjectionContext } from '@angular/core'
import { Observable, Subject } from 'rxjs'
import './typescript-loader.mjs'
const { HttpService } = await import('../src/services/http.service.ts')
const { getErrorMessage } = await import('../src/services/error.service.ts')
const { HttpError } = await import('../src/classes/http-error.ts')

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

test('the pinned XHR backend preserves plaintext transfer errors without an HTTP reason phrase', async () => {
  const message = 'La taille du fichier dépasse la limite de 3 octets.'
  const xhr = new (class extends EventTarget {
    upload = new EventTarget()
    status = 400
    statusText = ''
    response = message
    readyState = 4
    DONE = 4
    open() {}
    setRequestHeader() {}
    getAllResponseHeaders() {
      return 'Content-Type: text/plain; charset=utf-8\r\n'
    }
    send() {
      queueMicrotask(() => this.dispatchEvent(new Event('load')))
    }
    abort() {}
  })()
  const injector = Injector.create({ providers: [] })
  try {
    const { request, instance } = transport()
    instance.xhr = new HttpClient(
      runInInjectionContext(
        injector,
        () => new HttpXhrBackend({ build: () => xhr }),
      ),
    )
    await assert.rejects(
      request({
        method: 'POST',
        url: '/rest/rpc/upload',
        body: new Blob(['large']),
        uploadProgress: () => {},
      }),
      error => {
        assert.ok(error instanceof HttpError)
        assert.equal(error.code, 400)
        assert.equal(error.message, message)
        return true
      },
    )
  } finally {
    injector.destroy()
  }
})

test('HTTP errors accept only declared plaintext bodies and preserve the framework fallback', async () => {
  for (const progress of [undefined, () => {}]) {
    for (const [status, contentType, body, expected] of [
      [
        400,
        'Text/Plain; charset=utf-8',
        'Maximum size: 3 bytes',
        'Maximum size: 3 bytes',
      ],
      [404, 'text/plain', 'Not Found', 'Not Found'],
      [500, 'text/plain', 'Internal Server Error', 'Internal Server Error'],
      [502, 'text/html', '<html>proxy error</html>', null],
      [403, 'application/json', { message: 'unrelated object' }, null],
      [500, 'text/plain', '', null],
    ]) {
      const response = new HttpErrorResponse({
        status,
        statusText: 'OK',
        headers: new HttpHeaders({ 'content-type': contentType }),
        error: body,
        url: 'https://start.local/rest/rpc/upload',
      })
      const { request, events } = transport()
      const pending = request({
        method: 'POST',
        url: '/rest/rpc/upload',
        uploadProgress: progress,
      })
      events.error(response)
      await assert.rejects(pending, error => {
        assert.ok(error instanceof HttpError)
        assert.equal(error.code, status)
        assert.equal(error.message, expected ?? response.message)
        assert.notEqual(error.message, 'OK')
        return true
      })
    }
  }
  const network = new HttpError(
    new HttpErrorResponse({ status: 0, error: new Error('offline') }),
  )
  assert.match(getErrorMessage(network), /couldn't reach your server/)
})

test('non-HTTP failures preserve their original error', async () => {
  const { request, events } = transport()
  const original = new Error('timeout')
  const pending = request({ method: 'GET', url: '/rpc' })
  events.error(original)
  await assert.rejects(pending, error => error === original)
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
  const reason = new Error('cancelled')
  abort.abort(reason)
  await assert.rejects(pending, error => error === reason)
  assert.equal(unsubscribed, true)
})

test('timing out cancels the transport subscription', async context => {
  context.mock.timers.enable({ apis: ['setInterval'] })
  const { request, events } = transport()
  const pending = request({ method: 'POST', url: '/rpc', timeout: 100 })
  assert.equal(events.observed, true)
  context.mock.timers.tick(100)
  await assert.rejects(pending, /timeout/)
  assert.equal(events.observed, false)
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
