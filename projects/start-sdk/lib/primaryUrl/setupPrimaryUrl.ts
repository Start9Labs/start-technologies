import { createTask } from '@start9labs/start-core/actions'
import { InputSpec } from '@start9labs/start-core/actions/input/builder/inputSpec'
import { Value } from '@start9labs/start-core/actions/input/builder/value'
import {
  Action,
  ActionInfo,
  MaybeFn,
} from '@start9labs/start-core/actions/setupActions'
import { InitScript, setupOnInit } from '@start9labs/start-core/inits'
import * as T from '@start9labs/start-core/types'
import { getOwnHost } from '@start9labs/start-core/util/GetHostInfo'
import { Watchable, WatchSource } from '@start9labs/start-core/util/Watchable'
import { FilledHost } from '@start9labs/start-core/util/filledAddress'

/** A reader in the shape `FileHelper.read()` returns. */
export type Reader<A> = {
  once(): Promise<A>
  watch(effects: T.Effects, abort?: AbortSignal): AsyncGenerator<A, unknown>
}

export type SetupPrimaryUrlParams<Id extends T.ActionId> = {
  /** The id of the action that sets the URL. */
  id: Id
  /** The id given to `sdk.MultiHost.of` for the host the interface is exported from. */
  hostId: T.HostId
  /** The exported interface whose addresses the user chooses from. */
  interfaceId: T.ServiceInterfaceId
  /** The action's metadata, as for `sdk.Action.withInput`. */
  metadata: MaybeFn<Omit<T.ActionMetadata, 'hasInput'>>
  /** The label and description of the URL select. */
  field: { name: string; description: string | null }
  /** Reads the stored URL, e.g. `storeJson.read(s => s.primaryUrl)`. */
  get: Reader<string | null | undefined>
  /** Stores the URL the user chose. */
  set: (effects: T.Effects, url: string) => Promise<unknown>
}

export type PrimaryUrl<Id extends T.ActionId> = {
  /** The action that sets the URL. Register it with `sdk.Actions.of()`. */
  action: Action<Id, { url: string }>
  /**
   * Reader for the URL the service should use: the stored one, followed to its
   * hostname's current port and scheme; else the `.local` address, else the
   * first.
   */
  bestUsable: (effects: T.Effects) => WatchSource<string | null>
  /**
   * Keeps a task on `action` raised while the stored URL is unset or no longer
   * one of the interface's addresses, pre-filled with the `.local` address.
   * Register it with `sdk.setupInit()`, after the actions.
   */
  setupTask: (
    severity: T.TaskSeverity,
    options?: { reason?: string; replayId?: string },
  ) => InitScript
}

const parse = (url: string) => {
  try {
    return new URL(url)
  } catch {
    return null
  }
}

const urlsOf =
  (interfaceId: T.ServiceInterfaceId) =>
  (host: FilledHost | null): string[] => {
    const binding =
      host &&
      Object.values(host.bindings).find(b => interfaceId in b.interfaces)
    return binding
      ? binding.interfaces[interfaceId].addressInfo.nonLocal.format()
      : []
  }

/** The stored URL, or its hostname's address at another port or scheme. */
function follow(stored: string | null | undefined, urls: string[]) {
  if (!stored || urls.includes(stored)) return stored ?? undefined
  const was = parse(stored)
  const sameHost = urls.filter(u => parse(u)?.hostname === was?.hostname)
  return sameHost.find(u => parse(u)?.protocol === was?.protocol) ?? sameHost[0]
}

function fallback(urls: string[]) {
  return urls.find(u => parse(u)?.hostname.endsWith('.local')) ?? urls[0]
}

type Stored = string | null | undefined

const resolve = ([stored, urls]: [Stored, string[]]): string | null =>
  follow(stored, urls) ?? fallback(urls) ?? stored ?? null

export function setupPrimaryUrl<Id extends T.ActionId>(
  packageId: T.PackageId,
  {
    id,
    hostId,
    interfaceId,
    metadata,
    field,
    get,
    set,
  }: SetupPrimaryUrlParams<Id>,
): PrimaryUrl<Id> {
  const urls = (effects: T.Effects) =>
    getOwnHost(effects, hostId, urlsOf(interfaceId))

  const action = Action.withInput(
    id,
    metadata,
    InputSpec.of({
      url: Value.dynamicSelect(async ({ effects }) => {
        const offered = await urls(effects).once()
        return {
          ...field,
          values: Object.fromEntries(offered.map(u => [u, u])),
          default: fallback(offered) ?? null,
        }
      }),
    }),
    async ({ effects }) => {
      const stored = await get.once()
      return {
        url: follow(stored, await urls(effects).once()) ?? stored ?? undefined,
      }
    },
    async ({ effects, input }) => {
      await set(effects, input.url)
    },
  )

  return {
    action,
    bestUsable: effects =>
      Watchable.combine(
        effects,
        [
          { once: () => get.once(), watch: abort => get.watch(effects, abort) },
          urls(effects),
        ],
        resolve,
      ),
    setupTask: (severity, options) =>
      setupOnInit(async effects => {
        const offered = await urls(effects).const()
        if (!offered.length) return
        await createTask<ActionInfo<T.ActionId, { url: string }>>({
          effects,
          packageId,
          action,
          severity,
          options: {
            ...options,
            when: { condition: 'input-not-matches', once: false },
            input: {
              kind: 'partial',
              accept: offered.map(url => ({ url })),
              set: { url: fallback(offered) },
            },
          },
        })
      }),
  }
}
