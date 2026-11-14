import {
  HttpClient,
  HttpEventType,
  HttpErrorResponse,
  HttpResponse,
  HttpXhrBackend,
} from '@angular/common/http'
import { DOCUMENT, inject, Service } from '@angular/core'
import {
  filter,
  firstValueFrom,
  fromEvent,
  interval,
  map,
  Observable,
  race,
  take,
  takeUntil,
  tap,
} from 'rxjs'

import { HttpError } from '../classes/http-error'
import { RELATIVE_URL } from '../tokens/relative-url'
import {
  HttpAngularOptions,
  HttpOptions,
  LocalHttpResponse,
} from '../types/http.types'
import { RPCOptions, RPCResponse } from '../types/rpc.types'
import { rpcRequestBody, rpcRequestHeaders } from './rpc-request'
import { withLegacyPatchSequence } from './rpc-response'

@Service()
export class HttpService {
  private readonly relativeUrl = inject(RELATIVE_URL)
  private readonly http = inject(HttpClient)
  private readonly xhr = new HttpClient(inject(HttpXhrBackend))
  private readonly fullUrl = inject(DOCUMENT).location.origin

  async rpcRequest<T>(
    opts: RPCOptions,
    fullUrl?: string,
  ): Promise<LocalHttpResponse<RPCResponse<T>>> {
    return this.rpcMessage<T>(rpcRequestBody(opts), opts, fullUrl)
  }

  async rpcMessage<T>(
    body: string,
    { headers, timeout }: Pick<RPCOptions, 'headers' | 'timeout'>,
    fullUrl?: string,
  ): Promise<LocalHttpResponse<RPCResponse<T>>> {
    const response = await this.httpRequest<RPCResponse<T>>({
      method: 'POST',
      url: fullUrl || this.relativeUrl,
      headers: rpcRequestHeaders(headers),
      body,
      timeout,
    })
    return response.clone({
      body: withLegacyPatchSequence(
        response.body,
        response.headers.get('x-patch-sequence'),
      ),
    }) as LocalHttpResponse<RPCResponse<T>>
  }

  async httpRequest<T>(opts: HttpOptions): Promise<LocalHttpResponse<T>> {
    let { method, url, headers, body, responseType, timeout } = opts
    opts.signal?.throwIfAborted()

    url = opts.url.startsWith('/') ? this.fullUrl + url : url

    const { params } = opts
    if (hasParams(params)) {
      Object.keys(params).forEach(key => {
        if (params[key] === undefined) {
          delete params[key]
        }
      })
    }

    const options: HttpAngularOptions = {
      observe: 'response',
      headers,
      params,
      responseType: responseType || 'json',
    }

    const client = opts.uploadProgress ? this.xhr : this.http
    let req = client
      .request<T>(method, url, {
        ...options,
        body,
        observe: 'events',
        reportProgress: !!opts.uploadProgress,
        responseType: options.responseType as 'json',
      })
      .pipe(
        tap(event => {
          if (event.type === HttpEventType.UploadProgress) {
            opts.uploadProgress?.(event.loaded, event.total)
          }
        }),
        filter(event => event instanceof HttpResponse),
      ) as Observable<LocalHttpResponse<T>>

    if (opts.signal) {
      req = req.pipe(
        takeUntil(
          fromEvent(opts.signal, 'abort').pipe(
            map(() => {
              throw opts.signal!.reason
            }),
          ),
        ),
      )
    }

    return firstValueFrom(timeout ? withTimeout(req, timeout) : req).catch(
      (error: unknown) => {
        throw error instanceof HttpErrorResponse ? new HttpError(error) : error
      },
    )
  }
}

function hasParams(
  params?: HttpOptions['params'],
): params is Record<string, string | string[]> {
  return !!params
}

/** Unsubscribes the source when the response deadline wins. */
export function withTimeout<U>(
  req: Observable<U>,
  timeout: number,
): Observable<U> {
  return race(
    req,
    interval(timeout).pipe(
      take(1),
      map(() => {
        throw new Error('timeout')
      }),
    ),
  )
}
