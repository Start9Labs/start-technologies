import {
  ExtendedVersion,
  types as T,
  utils,
  VersionRange,
  z,
} from '@start9labs/start-sdk'
import * as net from 'net'
import { Effects } from '../Models/Effects'

import { CallbackHolder } from '../Models/CallbackHolder'
import { asError } from '@start9labs/start-core/util'
import { RPC } from '@start9labs/start-core'
const matchRpcError = z.looseObject({
  error: z.looseObject({
    code: z.number(),
    message: z.string(),
    data: z
      .union([
        z.string(),
        z.looseObject({
          details: z.string(),
          debug: z.string().nullable().optional(),
        }),
      ])
      .nullable()
      .optional(),
  }),
})
function testRpcError(v: unknown): v is RpcError {
  return matchRpcError.safeParse(v).success
}
const matchRpcResult = z.looseObject({
  result: z.unknown(),
})
function testRpcResult(v: unknown): v is z.infer<typeof matchRpcResult> {
  return matchRpcResult.safeParse(v).success
}
type RpcError = z.infer<typeof matchRpcError>

const SOCKET_PATH = '/media/startos/rpc/host.sock'
let hostSystemId = 0

export type EffectContext = {
  eventId: string | null
  callbacks?: CallbackHolder
  constRetry?: () => void
}

const rpcRoundFor =
  (eventId: string | null) =>
  <K extends RPC.RpcMethod<RPC.Effects>>(
    method: K,
    params: RPC.RpcParamType<RPC.Effects, K>,
  ): Promise<RPC.RpcReturnType<RPC.Effects, K>> => {
    const id = hostSystemId++
    const client = net.createConnection({ path: SOCKET_PATH }, () => {
      client.write(
        JSON.stringify({
          id,
          method,
          params: { ...params, eventId: eventId ?? undefined },
        }) + '\n',
      )
    })
    let bufs: Buffer[] = []
    return new Promise<RPC.RpcReturnType<RPC.Effects, K>>((resolve, reject) => {
      client.on('data', data => {
        try {
          bufs.push(data)
          if (data.reduce((acc, x) => acc || x == 10, false)) {
            const res: unknown = JSON.parse(
              Buffer.concat(bufs).toString().split('\n')[0],
            )
            if (testRpcError(res)) {
              let message = res.error.message
              console.error(
                'Error in host RPC:',
                utils.asError({ method, params, error: res.error }),
              )
              if (typeof res.error.data === 'string') {
                message += ': ' + res.error.data
                console.error(`Details: ${res.error.data}`)
              } else {
                if (res.error.data?.details) {
                  message += ': ' + res.error.data.details
                  console.error(`Details: ${res.error.data.details}`)
                }
                if (res.error.data?.debug) {
                  message += '\n' + res.error.data.debug
                  console.error(`Debug: ${res.error.data.debug}`)
                }
              }
              reject(new Error(`${message}@${method}`))
            } else if (testRpcResult(res)) {
              resolve(res.result as RPC.RpcReturnType<RPC.Effects, K>)
            } else {
              reject(new Error(`malformed response ${JSON.stringify(res)}`))
            }
          }
        } catch (error) {
          reject(error)
        }
        client.end()
      })
      client.on('error', error => {
        reject(error)
      })
    })
  }

export function makeEffects(context: EffectContext): Effects {
  const rpcRound = rpcRoundFor(context.eventId)
  const self: Effects = {
    eventId: context.eventId,
    child: name =>
      makeEffects({ ...context, callbacks: context.callbacks?.child(name) }),
    constRetry: context.constRetry,
    isInContext: !!context.callbacks,
    onLeaveContext:
      context.callbacks?.onLeaveContext?.bind(context.callbacks) ||
      (() => {
        console.warn(
          'no context for this effects object',
          new Error().stack?.replace(/^Error/, ''),
        )
      }),
    clearCallbacks(...[options]: Parameters<T.Effects['clearCallbacks']>) {
      return rpcRound('clear-callbacks', {
        ...options,
      })
    },
    action: {
      clear(...[options]: Parameters<T.Effects['action']['clear']>) {
        return rpcRound('action.clear', {
          ...options,
        })
      },
      export(...[options]: Parameters<T.Effects['action']['export']>) {
        return rpcRound('action.export', {
          ...options,
        })
      },
      getInput(...[options]: Parameters<T.Effects['action']['getInput']>) {
        return rpcRound('action.get-input', {
          ...options,
        })
      },
      createTask(...[options]: Parameters<T.Effects['action']['createTask']>) {
        return rpcRound('action.create-task', {
          ...options,
        })
      },
      run(...[options]: Parameters<T.Effects['action']['run']>) {
        return rpcRound('action.run', {
          ...options,
          input: options.input ?? null,
        })
      },
      clearTasks(...[options]: Parameters<T.Effects['action']['clearTasks']>) {
        return rpcRound('action.clear-tasks', {
          ...options,
        })
      },
    },
    bind(...[options]: Parameters<T.Effects['bind']>) {
      return rpcRound('bind', { ...options })
    },
    bindRange(...[options]: Parameters<T.Effects['bindRange']>) {
      return rpcRound('bind-range', { ...options })
    },
    clearBindings(...[options]: Parameters<T.Effects['clearBindings']>) {
      return rpcRound('clear-bindings', { ...options })
    },
    retireHost(...[options]: Parameters<T.Effects['retireHost']>) {
      return rpcRound('retire-host', { ...options })
    },
    retireBinding(...[options]: Parameters<T.Effects['retireBinding']>) {
      return rpcRound('retire-binding', { ...options })
    },
    clearServiceInterfaces(
      ...[options]: Parameters<T.Effects['clearServiceInterfaces']>
    ) {
      return rpcRound('clear-service-interfaces', { ...options })
    },
    getInstalledPackages(...[]: Parameters<T.Effects['getInstalledPackages']>) {
      return rpcRound('get-installed-packages', {})
    },
    getServiceManifest(
      ...[options]: Parameters<T.Effects['getServiceManifest']>
    ) {
      return rpcRound('get-service-manifest', {
        ...options,
        callback: context.callbacks?.addCallback(options.callback) || null,
      })
    },
    subcontainer: {
      createFs(options: { imageId: string; name: string }) {
        return rpcRound('subcontainer.create-fs', options)
      },
      destroyFs(options: { guid: string }): Promise<null> {
        return rpcRound('subcontainer.destroy-fs', options)
      },
    },
    exportServiceInterface: ((
      ...[options]: Parameters<Effects['exportServiceInterface']>
    ) => {
      return rpcRound('export-service-interface', options)
    }) as Effects['exportServiceInterface'],
    exportRangeServiceInterface: ((
      ...[options]: Parameters<Effects['exportRangeServiceInterface']>
    ) => {
      return rpcRound('export-range-service-interface', options)
    }) as Effects['exportRangeServiceInterface'],
    getContainerIp(...[options]: Parameters<T.Effects['getContainerIp']>) {
      return rpcRound('get-container-ip', {
        ...options,
        callback: context.callbacks?.addCallback(options.callback) || null,
      })
    },
    getOsIp(...[]: Parameters<T.Effects['getOsIp']>) {
      return rpcRound('get-os-ip', {})
    },
    getHostInfo: ((...[allOptions]: Parameters<T.Effects['getHostInfo']>) => {
      const options = {
        ...allOptions,
        callback: context.callbacks?.addCallback(allOptions.callback) || null,
      }
      return rpcRound('get-host-info', options) as any
    }) as Effects['getHostInfo'],
    getServiceInterface(
      ...[options]: Parameters<T.Effects['getServiceInterface']>
    ) {
      return rpcRound('get-service-interface', {
        ...options,
        callback: context.callbacks?.addCallback(options.callback) || null,
      })
    },

    getServicePortForward(
      ...[options]: Parameters<T.Effects['getServicePortForward']>
    ) {
      return rpcRound('get-service-port-forward', options)
    },
    getSslCertificate(options: Parameters<T.Effects['getSslCertificate']>[0]) {
      return rpcRound('get-ssl-certificate', {
        ...options,
        callback: context.callbacks?.addCallback(options.callback) || null,
      })
    },
    getSslKey(options: Parameters<T.Effects['getSslKey']>[0]) {
      return rpcRound('get-ssl-key', options)
    },
    getSystemSmtp(...[options]: Parameters<T.Effects['getSystemSmtp']>) {
      return rpcRound('get-system-smtp', {
        ...options,
        callback: context.callbacks?.addCallback(options.callback) || null,
      })
    },
    getOutboundGateway(
      ...[options]: Parameters<T.Effects['getOutboundGateway']>
    ) {
      return rpcRound('get-outbound-gateway', {
        ...options,
        callback: context.callbacks?.addCallback(options.callback) || null,
      })
    },
    listServiceInterfaces(
      ...[options]: Parameters<T.Effects['listServiceInterfaces']>
    ) {
      return rpcRound('list-service-interfaces', {
        ...options,
        callback: context.callbacks?.addCallback(options.callback) || null,
      })
    },
    mount(...[options]: Parameters<T.Effects['mount']>) {
      return rpcRound('mount', options)
    },
    restart(...[]: Parameters<T.Effects['restart']>) {
      console.log('Restarting service...')
      return rpcRound('restart', {})
    },
    setDependencies(
      dependencies: Parameters<T.Effects['setDependencies']>[0],
    ): ReturnType<T.Effects['setDependencies']> {
      return rpcRound('set-dependencies', dependencies)
    },
    checkDependencies(
      options: Parameters<T.Effects['checkDependencies']>[0],
    ): ReturnType<T.Effects['checkDependencies']> {
      return rpcRound('check-dependencies', options)
    },
    getDependencies(): ReturnType<T.Effects['getDependencies']> {
      return rpcRound('get-dependencies', {})
    },
    setHealth(...[options]: Parameters<T.Effects['setHealth']>) {
      return rpcRound('set-health', options)
    },
    setBackupProgress(
      ...[options]: Parameters<T.Effects['setBackupProgress']>
    ) {
      return rpcRound('set-backup-progress', options)
    },
    setInitProgress(...[options]: Parameters<T.Effects['setInitProgress']>) {
      return rpcRound('set-init-progress', options)
    },
    notification: {
      create(...[options]: Parameters<T.Effects['notification']['create']>) {
        return rpcRound('notification.create', options)
      },
    },

    getStatus(...[o]: Parameters<T.Effects['getStatus']>) {
      return rpcRound('get-status', {
        ...o,
        callback: context.callbacks?.addCallback(o.callback) || null,
      })
    },
    /// DEPRECATED
    setMainStatus(o: { status: 'running' | 'stopped' }): Promise<null> {
      return rpcRound('set-main-status', o)
    },

    shutdown(...[]: Parameters<T.Effects['shutdown']>) {
      return rpcRound('shutdown', {})
    },
    getDataVersion() {
      return rpcRound('get-data-version', {})
    },
    setDataVersion(...[options]: Parameters<T.Effects['setDataVersion']>) {
      return rpcRound('set-data-version', options)
    },
    plugin: {
      url: {
        register(
          ...[options]: Parameters<T.Effects['plugin']['url']['register']>
        ) {
          return rpcRound('plugin.url.register', options)
        },
        exportUrl(
          ...[options]: Parameters<T.Effects['plugin']['url']['exportUrl']>
        ) {
          return rpcRound('plugin.url.export-url', options)
        },
        clearUrls(
          ...[options]: Parameters<T.Effects['plugin']['url']['clearUrls']>
        ) {
          return rpcRound('plugin.url.clear-urls', options)
        },
      },
    },
  }
  if (context.callbacks?.onLeaveContext)
    self.onLeaveContext(() => {
      self.constRetry = undefined
      self.isInContext = false
      self.onLeaveContext = () => {
        console.warn(
          'this effects object is already out of context',
          new Error().stack?.replace(/^Error/, ''),
        )
      }
    })
  return self
}
