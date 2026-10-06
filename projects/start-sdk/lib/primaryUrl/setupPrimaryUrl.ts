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
import { Watchable } from '@start9labs/start-core/util/Watchable'
import { Filter, FilledHost } from '@start9labs/start-core/util/filledAddress'

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
  /** Narrows the addresses offered, on top of dropping loopback, link-local and bridge ones. */
  filter?: Filter
  /** Offer only addresses served over TLS. */
  ssl?: boolean
  /**
   * `false` when the service must run only on an address the user chose: the
   * form and task start empty, and `bestUsable` is `null` while nothing is
   * stored or the stored URL is gone. Defaults to `true`.
   */
  fallback?: boolean
}

export type PrimaryUrl<Id extends T.ActionId> = {
  /** The action that sets the URL. Register it with `sdk.Actions.of()`. */
  action: Action<Id, { url: string }>
  /**
   * Reader for the URL the service should use: the stored one, followed to its
   * hostname's current port and scheme; else the preferred address (see
   * `setupTask`), unless `fallback` is `false`.
   */
  bestUsable: (effects: T.Effects) => Watchable<string | null>
  /**
   * Keeps a task on `action` raised while the stored URL is unset or no longer
   * one of the interface's addresses, pre-filled with the preferred address: a
   * public domain, HTTPS first; else the `.local` address; else the first.
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

type Stored = string | null | undefined
type Offered = { urls: string[]; preferred: string | undefined }

const offeredBy =
  (interfaceId: T.ServiceInterfaceId, filter: Filter, ssl: boolean) =>
  (host: FilledHost | null): Offered => {
    const binding =
      host &&
      Object.values(host.bindings).find(b => interfaceId in b.interfaces)
    if (!binding) return { urls: [], preferred: undefined }
    const address = binding.interfaces[interfaceId].addressInfo.nonLocal
    const hostnames = address
      .filter(filter)
      .format('hostname-info')
      .filter(h => !ssl || h.ssl)
    const rank = (h: T.HostnameInfo) =>
      h.metadata.kind === 'public-domain'
        ? Number(!h.ssl)
        : h.hostname.endsWith('.local')
          ? 2
          : 3
    const best = [...hostnames].sort((a, b) => rank(a) - rank(b))[0]
    return {
      urls: hostnames.map(address.toUrl),
      preferred: best && address.toUrl(best),
    }
  }

/** The stored URL, or its hostname's address at another port or scheme. */
function follow(stored: Stored, urls: string[]) {
  if (!stored) return undefined
  if (urls.includes(stored)) return stored
  const was = parse(stored)
  const sameHost = urls.filter(u => parse(u)?.hostname === was?.hostname)
  return sameHost.find(u => parse(u)?.protocol === was?.protocol) ?? sameHost[0]
}

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
    filter = {},
    ssl = false,
    fallback = true,
  }: SetupPrimaryUrlParams<Id>,
): PrimaryUrl<Id> {
  const offered = (effects: T.Effects) =>
    getOwnHost(effects, hostId, offeredBy(interfaceId, filter, ssl))
  const resolve = ([stored, { urls, preferred }]: [Stored, Offered]) =>
    fallback
      ? (follow(stored, urls) ?? preferred ?? (stored || null))
      : (follow(stored, urls) ?? null)

  const action = Action.withInput(
    id,
    metadata,
    InputSpec.of({
      url: Value.dynamicSelect(async ({ effects }) => {
        const { urls, preferred } = await offered(effects).once()
        return {
          ...field,
          values: Object.fromEntries(urls.map(u => [u, u])),
          default: (fallback && preferred) || null,
        }
      }),
    }),
    async ({ effects }) => {
      const stored = await get.once()
      return {
        url:
          follow(stored, (await offered(effects).once()).urls) ??
          (stored || undefined),
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
          offered(effects),
        ],
        resolve,
      ),
    setupTask: (severity, options) =>
      setupOnInit(async effects => {
        const { urls, preferred } = await offered(effects).const()
        if (!urls.length) return
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
              accept: urls.map(url => ({ url })),
              set: fallback ? { url: preferred } : {},
            },
          },
        })
      }),
  }
}
