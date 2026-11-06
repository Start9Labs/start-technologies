import { Injectable, DOCUMENT, inject } from '@angular/core'
import { HttpClient, HttpResponse } from '@angular/common/http'
import { firstValueFrom, timeout, TimeoutError } from 'rxjs'
import { HttpErrorResponse } from '@angular/common/http'
import {
  RELATIVE_URL,
  type HttpOptions as SharedHttpOptions,
} from '@start9labs/shared'
import { StaleUiService } from 'src/app/services/stale-ui.service'

@Injectable({
  providedIn: 'root',
})
export class HttpService {
  private readonly url = inject(DOCUMENT).location.origin + inject(RELATIVE_URL)
  private readonly http = inject(HttpClient)
  private readonly staleUi = inject(StaleUiService)

  async request<T>(options: HttpOptions): Promise<HttpResponse<T>> {
    const { params, body, headers, timeout: timeoutMs } = options

    if (hasParams(params)) {
      Object.keys(params).forEach(key => {
        if (params[key] === undefined) {
          delete params[key]
        }
      })
    }

    const req$ = this.http.post<T>(this.url, body, {
      observe: 'response',
      params,
      headers,
      responseType: 'json',
    })

    try {
      // Timeout unsubscription aborts the underlying request.
      const res = await firstValueFrom(
        timeoutMs ? req$.pipe(timeout({ each: timeoutMs })) : req$,
      )
      // A daemon restart can change firmware without dropping this connection.
      this.staleUi.checkHash(res.headers.get('x-startwrt-git-hash'))
      return res
    } catch (e: any) {
      // Timeouts drive the same reconnect flow as network failures.
      if (e instanceof TimeoutError) {
        throw Object.assign(new Error('Network timeout'), { code: 0 })
      }
      throw new HttpError(e)
    }
  }
}

function hasParams(
  params?: HttpOptions['params'],
): params is Record<string, string | string[]> {
  return !!params
}

export class HttpError {
  readonly code: number
  readonly message: string

  constructor(private readonly error: HttpErrorResponse) {
    this.code = this.error.status
    this.message = this.error.statusText
  }
}

type ParamPrimitive = string | number | boolean

export type HttpOptions = {
  headers?: SharedHttpOptions['headers']
  params?: {
    [param: string]: ParamPrimitive | ParamPrimitive[]
  }
  body?: any
  /** Abort the request after this many ms, surfaced as a network error. */
  timeout?: number
}
