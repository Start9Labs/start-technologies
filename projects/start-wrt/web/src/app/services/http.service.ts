import { Injectable, DOCUMENT, inject } from '@angular/core'
import { HttpClient, HttpResponse } from '@angular/common/http'
import { firstValueFrom, timeout, TimeoutError } from 'rxjs'
import { HttpErrorResponse } from '@angular/common/http'
import { RELATIVE_URL } from '@start9labs/shared'
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
      withCredentials: true,
      params,
      headers,
      responseType: 'json',
    })

    try {
      // A timeout here unsubscribes, aborting the underlying request, so a
      // wedged connection is dropped rather than awaited to its TCP deadline.
      const res = await firstValueFrom(
        timeoutMs ? req$.pipe(timeout({ each: timeoutMs })) : req$,
      )
      // Every RPC response carries the firmware's build stamp; a quick daemon
      // restart (CLI deploy) drops no request, so this — not connection-loss
      // recovery — is what lets an open tab notice a firmware update.
      this.staleUi.checkHash(res.headers.get('x-startwrt-git-hash'))
      return res
    } catch (e: any) {
      // Surface a timeout as a network error (code 0) so isNetworkError() and
      // the reconnect flow treat it the same as a dropped connection.
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
  headers?: {
    [header: string]: string | string[]
  }
  params?: {
    [param: string]: ParamPrimitive | ParamPrimitive[]
  }
  body?: any
  /** Abort the request after this many ms, surfaced as a network error. */
  timeout?: number
}
