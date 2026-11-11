import type { Dump, Revision, Update } from 'patch-db-client'
import { bufferTime, connect, defer, filter, startWith, switchMap } from 'rxjs'
import type { Observable, OperatorFunction } from 'rxjs'
import type { RPCResponse, RPCStreamResponse } from '../types/rpc.types'
import type { AuthKeyService } from './auth-key.service'
import type { RpcSocketService } from './rpc-socket.service'
import { rpcRequestBody } from './rpc-request'

type LegacySubscription = { dump: Dump<unknown>; guid: string }

/** The initial dump establishes PatchDB fences; callers must not await the cache before emitting it. */
export function databaseSubscription<T>(
  sockets: RpcSocketService,
  authKeys: AuthKeyService,
): Observable<RPCResponse<Update<T>> | RPCStreamResponse<Update<T>>> {
  return defer(() => authKeys.lease()).pipe(
    switchMap(lease => {
      if (!lease)
        throw new Error('Database subscriptions require an authenticated key')
      return sockets.stream<Update<T>>(
        rpcRequestBody({ method: 'db.subscribe', params: {} }),
        lease,
      )
    }),
  )
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
