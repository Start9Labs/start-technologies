import { legacyDatabaseSubscription } from '@start9labs/shared'
import type { RPC, RpcStreamClient } from '@start9labs/start-core'
import type { Revision, Update } from 'patch-db-client'
import type { DataModel } from '../patch-db/data-model'
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

  /** Waits for initiating PatchDB fences; database observation belongs to watchDb$. */
  abstract readonly streams: Omit<RpcStreamClient<RPC.StartOS>, 'db'>

  watchDb$(): Observable<Update<DataModel>> {
    return legacyDatabaseSubscription<DataModel>(
      () => this.rpc.db.subscribe({}),
      guid => this.openWebsocket$<Revision>(guid),
    )
  }

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

  /** Reaches a foreign server unsigned; rejection never changes local authentication. */
  abstract echo(params: Params<'echo'>, url: string): Promise<string>
}
