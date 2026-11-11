import type { Dump, Revision, Update } from 'patch-db-client'
import { createRpcStreamClient } from '@start9labs/start-core'
import type { RPC, RpcStreamClient } from '@start9labs/start-core'
import {
  bufferTime,
  connect,
  concatWith,
  defer,
  filter,
  finalize,
  from,
  startWith,
  switchMap,
  throwError,
} from 'rxjs'
import type { Observable, OperatorFunction } from 'rxjs'
import type { AuthKeyService } from './auth-key.service'
import type { RpcSocketService } from './rpc-socket.service'
import type { ServerIdentityService } from './server-identity.service'
import { rpcRequestBody } from './rpc-request'

type LegacySubscription = { dump: Dump<unknown>; guid: string }

/** The initial dump establishes PatchDB fences; callers must not await the cache before emitting it. */
export function databaseSubscription<T>(
  sockets: RpcSocketService,
  authKeys: AuthKeyService,
  identity: ServerIdentityService,
): Observable<Update<T>> {
  return defer(() => {
    const abort = new AbortController()
    const client = createRpcStreamClient<RpcStreamClient<RPC.StartOS>>({
      stream: async (method, params, options) => {
        const body = rpcRequestBody({
          method,
          params: params as Record<string, unknown>,
        })
        const lease = await authKeys.lease()
        options?.signal?.throwIfAborted()
        if (!lease)
          throw new Error('Database subscriptions require an authenticated key')
        return sockets.stream(body, lease, {
          ...options,
          openObserver: identity.openObserver,
        })
      },
    })
    return from(
      (async function* () {
        const { initial, items } = await client.db.subscribe(
          {},
          { signal: abort.signal },
        )
        yield initial as Dump<T>
        for await (const revision of items) yield revision as Revision
      })(),
    ).pipe(finalize(() => abort.abort()))
  })
}

/** Observes the GUID result of a database subscription without rpcstream opt-in. */
export function legacyDatabaseSubscription<T>(
  subscribe: () => Promise<LegacySubscription | Dump<unknown>>,
  revisions: (guid: string) => Observable<Revision>,
): Observable<Update<T>> {
  return defer(subscribe).pipe(
    switchMap(subscription => {
      const { dump, guid } = subscription as LegacySubscription
      return revisions(guid).pipe(startWith(dump as Dump<T>))
    }),
  )
}

/** Successful subscription termination still requires source recovery. */
export function databaseSource<T>(
  updates: Observable<Update<T>>,
): Observable<Update<T>[]> {
  return updates.pipe(
    bufferDatabaseRevisions(),
    concatWith(throwError(() => new Error('Database subscription ended'))),
  )
}

/** Delivers initial dumps immediately and batches following revisions. */
export function bufferDatabaseRevisions<T>(): OperatorFunction<
  Update<T>,
  Update<T>[]
> {
  return connect(updates =>
    updates.pipe(
      filter((update): update is Dump<T> => 'value' in update),
      switchMap(dump =>
        updates.pipe(
          filter((update): update is Revision => 'patch' in update),
          bufferTime(250),
          filter(revisions => revisions.length > 0),
          startWith([dump]),
        ),
      ),
    ),
  )
}
