import { Observable } from 'rxjs'
import { ApiClient, Params, WebsocketConfig } from './api.types'

export type FileUploadOptions = {
  filename?: string
  progress?: (loaded: number, total?: number) => void
  signal?: AbortSignal
}

export abstract class ApiService {
  /** The server's RPC methods, as generated from its bindings. */
  abstract readonly rpc: ApiClient

  abstract uploadFile(
    guid: string,
    body: Blob,
    options?: FileUploadOptions,
  ): Promise<void>

  abstract getStatic(
    urls: string[],
    params: Record<string, string | number>,
  ): Promise<string>

  abstract getStaticObjectUrl(url: string): Promise<string>

  abstract downloadFileUrl(guid: string): string

  abstract openWebsocket$<T>(
    guid: string,
    config?: WebsocketConfig<T>,
  ): Observable<T>

  /** Reaches a server other than the one serving the UI, unsigned. */
  abstract echo(params: Params<'echo'>, url: string): Promise<string>
}
