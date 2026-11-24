import { normalizeProjection } from 'patch-db-client'
import type { Dump, Projection, Revision, Update } from 'patch-db-client'
import { createRpcStreamClient } from '@start9labs/start-core'
import type { RPC, RpcStreamClient } from '@start9labs/start-core'
import {
  bufferTime,
  connect,
  concatWith,
  defer,
  filter,
  from,
  startWith,
  switchMap,
  throwError,
} from 'rxjs'
import type { Observable, OperatorFunction } from 'rxjs'
import type { AuthKeyService } from './auth-key.service'
import type { RpcSocketService } from './rpc-socket.service'
import type { ServerIdentityService } from './server-identity.service'
import { authenticatedRpcTransport } from './local-rpc'
import { withAbortSignal } from '../util/with-abort'

type LegacySubscription = { dump: Dump<unknown>; guid: string }

export type DatabaseCursor = { epoch: string; id: number }

/** A dump, or the revisions a resumed subscriber lacks, current as of `id`. */
export type DatabaseStart<T> = { epoch?: string } & (
  | (Dump<T> & { revisions?: null })
  | { id: number; revisions: Revision[] }
)

export type DatabaseUpdate<T> = DatabaseStart<T> | Revision

/** The initial start establishes PatchDB fences; callers must not await the cache before emitting it. */
export function databaseSubscription<T>(
  sockets: RpcSocketService,
  authKeys: AuthKeyService,
  identity: ServerIdentityService,
  since: () => DatabaseCursor | null = () => null,
  pointer?: string,
  projection?: Projection | null,
): Observable<DatabaseUpdate<T>> {
  return withAbortSignal(signal => {
    const client = createRpcStreamClient<RpcStreamClient<RPC.StartOS>>(
      authenticatedRpcTransport(sockets, authKeys, identity),
    )
    return from(
      (async function* () {
        const cursor = since()
        const { initial, items } = await client.db.subscribe(
          {
            ...(cursor ? { since: cursor } : {}),
            ...(pointer ? { pointer } : {}),
            ...normalizeProjection(projection),
          },
          { signal },
        )
        yield initial as DatabaseStart<T>
        for await (const revision of items) yield revision as Revision
      })(),
    )
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
  updates: Observable<DatabaseUpdate<T>>,
): Observable<Update<T>[]> {
  return updates.pipe(
    bufferDatabaseRevisions(),
    concatWith(throwError(() => new Error('Database subscription ended'))),
  )
}

/** Delivers each start immediately and batches following revisions. */
export function bufferDatabaseRevisions<T>(): OperatorFunction<
  DatabaseUpdate<T>,
  Update<T>[]
> {
  return connect(updates =>
    updates.pipe(
      filter((update): update is DatabaseStart<T> => !('patch' in update)),
      switchMap(start =>
        updates.pipe(
          filter((update): update is Revision => 'patch' in update),
          bufferTime(250),
          filter(revisions => revisions.length > 0),
          startWith(
            start.revisions
              ? [...start.revisions, { id: start.id, patch: [] }]
              : [start],
          ),
        ),
      ),
    ),
  )
}
