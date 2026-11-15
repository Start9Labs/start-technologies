import { HttpHeaders, HttpResponse } from '@angular/common/http'

export type Method = 'GET' | 'POST'

type ParamPrimitive = string | number | boolean

export interface HttpOptions {
  method: Method
  url: string
  headers?:
    | HttpHeaders
    | {
        [header: string]: string | string[]
      }
  params?: {
    [param: string]: ParamPrimitive | ParamPrimitive[]
  }
  responseType?: 'json' | 'text' | 'arraybuffer'
  body?: any
  timeout?: number
  /** Uses XHR to report bytes sent; fetch does not expose upload progress. */
  uploadProgress?: (loaded: number, total?: number) => void
  signal?: AbortSignal
}

export interface HttpAngularOptions {
  observe: 'response'
  headers?: HttpOptions['headers']
  params?: {
    [param: string]: ParamPrimitive | ParamPrimitive[]
  }
  responseType?: 'json' | 'text' | 'arraybuffer'
}

export interface LocalHttpResponse<T> extends HttpResponse<T> {
  body: T
}

export interface RequestError {
  code: number
  message: string
  details: string
}
