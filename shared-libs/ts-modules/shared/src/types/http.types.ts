import { HttpHeaders, HttpResponse } from '@angular/common/http'

export type Method = 'GET' | 'POST'

type ParamPrimitive = string | number | boolean

export interface HttpOptions {
  method: Method
  url: string
  headers?: {
    [header: string]: string | string[]
  }
  params?: {
    [param: string]: ParamPrimitive | ParamPrimitive[]
  }
  responseType?: 'json' | 'text' | 'arrayBuffer'
  body?: any
  timeout?: number
}

export interface HttpAngularOptions {
  observe: 'response'
  headers?:
    | HttpHeaders
    | {
        [header: string]: string | string[]
      }
  params?: {
    [param: string]: ParamPrimitive | ParamPrimitive[]
  }
  responseType?: 'json' | 'text' | 'arrayBuffer'
}

export interface LocalHttpResponse<T> extends HttpResponse<T> {
  body: T
}

export interface RequestError {
  code: number
  message: string
  details: string
}
