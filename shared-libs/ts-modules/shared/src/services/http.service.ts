import { HttpClient } from '@angular/common/http'
import { DOCUMENT, inject, Service } from '@angular/core'
import {
  firstValueFrom,
  from,
  interval,
  lastValueFrom,
  map,
  Observable,
  race,
  take,
} from 'rxjs'

import { HttpError } from '../classes/http-error'
import { RELATIVE_URL } from '../tokens/relative-url'
import {
  HttpAngularOptions,
  HttpOptions,
  LocalHttpResponse,
} from '../types/http.types'
import { RPCOptions, RPCResponse } from '../types/rpc.types'

@Service()
export class HttpService {
  private readonly relativeUrl = inject(RELATIVE_URL)
  private readonly http = inject(HttpClient)
  private readonly fullUrl = inject(DOCUMENT).location.origin

  async rpcRequest<T>(
    opts: RPCOptions,
    fullUrl?: string,
  ): Promise<LocalHttpResponse<RPCResponse<T>>> {
    const { method, headers, params, timeout } = opts

    return this.httpRequest<RPCResponse<T>>({
      method: 'POST',
      url: fullUrl || this.relativeUrl,
      headers,
      body: { method, params },
      timeout,
    })
  }

  async httpRequest<T>(opts: HttpOptions): Promise<LocalHttpResponse<T>> {
    let { method, url, headers, body, responseType, timeout } = opts

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

    let req: Observable<LocalHttpResponse<T>>
    if (method === 'GET') {
      req = this.http.get(url, options as any) as any
    } else {
      req = this.http.post(url, body, options as any) as any
    }

    return firstValueFrom(timeout ? withTimeout(req, timeout) : req).catch(
      e => {
        throw new HttpError(e)
      },
    )
  }
}

function hasParams(
  params?: HttpOptions['params'],
): params is Record<string, string | string[]> {
  return !!params
}

function withTimeout<U>(req: Observable<U>, timeout: number): Observable<U> {
  return race(
    from(lastValueFrom(req)), // this guarantees it only emits on completion, intermediary emissions are suppressed.
    interval(timeout).pipe(
      take(1),
      map(() => {
        throw new Error('timeout')
      }),
    ),
  )
}
