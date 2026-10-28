import { Observable } from 'rxjs'
import { ApiClient, Params, WebsocketConfig } from './api.types'

export abstract class ApiService {
  /** The server's RPC methods, as generated from its bindings. */
  abstract readonly rpc: ApiClient

  abstract uploadFile(guid: string, body: Blob): Promise<void>

  // for getting static files: ex license
  abstract getStatic(
    urls: string[],
    params: Record<string, string | number>,
  ): Promise<string>

  abstract getStaticObjectUrl(url: string): Promise<string>

  abstract openWebsocket$<T>(
    guid: string,
    config?: WebsocketConfig<T>,
  ): Observable<T>

  /** Reaches a server other than the one serving the UI, unsigned. */
  abstract echo(params: Params<'echo'>, url: string): Promise<string>
}
