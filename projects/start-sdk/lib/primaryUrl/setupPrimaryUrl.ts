import { createTask } from '@start9labs/start-core/actions'
import { InputSpec } from '@start9labs/start-core/actions/input/builder/inputSpec'
import { Value } from '@start9labs/start-core/actions/input/builder/value'
import { Action } from '@start9labs/start-core/actions/setupActions'
import { setupOnInit } from '@start9labs/start-core/inits'
import * as T from '@start9labs/start-core/types'
import { isIP } from 'node:net'
import { getOwnHost } from '@start9labs/start-core/util/GetHostInfo'
import {
  FilledHost,
  filterNonLocal,
} from '@start9labs/start-core/util/filledAddress'
import { z } from '@start9labs/start-core/zExport'
import { FileHelper } from '../util/fileHelper'

/** The field of a file model that holds the chosen URL. */
export type PrimaryUrlStore<A> = {
  file: FileHelper<A>
  get: (data: A) => string | null | undefined
  set: (url: string) => T.AllowReadonly<T.DeepPartial<A>>
}

export type SetupPrimaryUrlParams<A> = {
  /** The id given to `sdk.MultiHost.of` for the host the interface is exported from. */
  hostId: T.HostId
  /** The exported interface whose addresses the user chooses from. */
  interfaceId: T.ServiceInterfaceId
  store: PrimaryUrlStore<A>
  /** Defaults to `set-primary-url`. */
  actionId?: T.ActionId
  /** When the chosen hostname is gone: store `defaultUrl`'s pick in its place, or raise a task. Defaults to `fallback`. */
  onRemoved?: 'fallback' | 'task'
  /** Of the task raised when the chosen hostname is gone and nothing replaces it. Defaults to `important`. */
  severity?: T.TaskSeverity
  /** Chooses the first URL when none is stored. Defaults to the `.local` address, else the first. */
  defaultUrl?: (urls: string[]) => string | undefined
  name?: string
  description?: string
  warning?: string | null
  group?: string | null
  /** The label of the select field. */
  fieldName?: string
  /** Shown on the task. */
  reason?: string
}

type Addresses = {
  rows: { url: string; gateway: string | null }[]
  mdns: boolean
  up: string[]
}

// A `.local` name or an IP comes and goes with the LAN link; anything else is removed on purpose.
const kindOf = (hostname: string) =>
  hostname.endsWith('.local')
    ? 'mdns'
    : isIP(hostname.replace(/^\[|\]$/g, ''))
      ? 'ip'
      : 'other'

const parse = (url: string) => {
  try {
    const { protocol, hostname } = new URL(url)
    return { protocol, hostname }
  } catch {
    return null
  }
}

export function setupPrimaryUrl<A>(
  packageId: T.PackageId,
  params: SetupPrimaryUrlParams<A>,
) {
  const {
    hostId,
    interfaceId,
    store,
    actionId = 'set-primary-url',
    onRemoved = 'fallback',
    severity = 'important',
    defaultUrl = urls =>
      urls.find(u => parse(u)?.hostname.endsWith('.local')) ?? urls[0],
    name = 'Set Primary URL',
    description = 'Choose which of this service’s addresses it advertises in the links it generates.',
    warning = null,
    group = null,
    fieldName = 'URL',
    reason = 'The primary URL is no longer one of this service’s addresses. Choose a new one.',
  } = params
  const replayId = `${packageId}:${actionId}`

  const addresses = (host: FilledHost | null): Addresses | null => {
    const binding =
      host &&
      Object.values(host.bindings).find(b => interfaceId in b.interfaces)
    if (!binding) return null
    const address =
      binding.interfaces[interfaceId].addressInfo.configured.nonLocal
    const lan = filterNonLocal(binding.addresses.available)
    const gatewayOf = (h: T.HostnameInfo) =>
      h.metadata.kind === 'ipv4' || h.metadata.kind === 'ipv6'
        ? h.metadata.gateway
        : null
    return {
      rows: address.hostnames.map(h => ({
        url: address.toUrl(h),
        gateway: gatewayOf(h),
      })),
      mdns: lan.some(h => h.metadata.kind === 'mdns'),
      up: lan.flatMap(h => gatewayOf(h) ?? []),
    }
  }
  const read = () => store.file.read(store.get)
  // Which interface the chosen IP came from; an IP is gone only once that interface is up without it.
  const source = FileHelper.json(
    `${store.file.path}.${actionId}.json`,
    z.looseObject({
      url: z.string().optional(),
      gateway: z.string().nullable().optional(),
    }),
  )
  const record = (effects: T.Effects, rows: Addresses['rows'], url: string) =>
    source.write(effects, {
      url,
      gateway: rows.find(r => r.url === url)?.gateway ?? null,
    })
  // A damaged record reads as none, which the next store of a URL rewrites.
  const remembered = () =>
    source
      .read()
      .once()
      .catch(() => null)

  const action = Action.withInput(
    actionId,
    {
      name,
      description,
      warning,
      allowedStatuses: 'any',
      group,
      visibility: 'enabled',
    },
    InputSpec.of({
      url: Value.dynamicSelect(async ({ effects }) => {
        const urls = (
          (await getOwnHost(effects, hostId, addresses).once())?.rows ?? []
        ).map(r => r.url)
        return {
          name: fieldName,
          values: Object.fromEntries(urls.map(u => [u, u])),
          default: null,
        }
      }),
    }),
    async () => ({ url: (await read().once()) ?? undefined }),
    async ({ effects, input }) => {
      await store.file.merge(effects, store.set(input.url))
      const rows =
        (await getOwnHost(effects, hostId, addresses).once())?.rows ?? []
      await record(effects, rows, input.url)
    },
  )

  const init = setupOnInit(async effects => {
    const current = await getOwnHost(effects, hostId, addresses).const()
    const stored = await read().const(effects)
    const rows = current?.rows ?? []
    const urls = rows.map(r => r.url)
    const write = async (url: string) => {
      await store.file.merge(effects, store.set(url), {
        allowWriteAfterConst: true,
      })
      await record(effects, rows, url)
    }
    const clear = () => effects.action.clearTasks({ only: [replayId] })

    if (!stored) {
      const url = defaultUrl(urls)
      if (url) await write(url)
      return
    }
    if (urls.includes(stored)) {
      if ((await remembered())?.url !== stored)
        await record(effects, rows, stored)
      await clear()
      return
    }
    const was = parse(stored)
    const sameHost = urls.filter(u => parse(u)?.hostname === was?.hostname)
    const replacement =
      sameHost.find(u => parse(u)?.protocol === was?.protocol) ?? sameHost[0]
    if (replacement) {
      await write(replacement)
      await clear()
      return
    }
    if (!current) return
    const kind = kindOf(was?.hostname ?? '')
    if (kind === 'mdns' && !current.mdns) return
    if (kind === 'ip') {
      const known = await remembered()
      const gateway = known?.url === stored ? known.gateway : null
      if (!gateway || !current.up.includes(gateway)) return
    }
    const fallback = onRemoved === 'fallback' ? defaultUrl(urls) : undefined
    if (fallback) {
      await write(fallback)
      await clear()
      return
    }
    await createTask({
      effects,
      packageId,
      action,
      severity,
      options: { reason },
    })
  })

  return { action, init, read }
}
