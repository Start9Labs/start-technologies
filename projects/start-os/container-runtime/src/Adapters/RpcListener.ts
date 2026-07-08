// @ts-check

import * as net from 'net'

import {
  ExtendedVersion,
  types as T,
  utils,
  VersionRange,
  z,
} from '@start9labs/start-sdk'
import * as fs from 'fs'

import { CallbackHolder } from '../Models/CallbackHolder'
import { AllGetDependencies } from '../Interfaces/AllGetDependencies'
import { jsonPath, unNestPath } from '../Models/JsonPath'
import { System } from '../Interfaces/System'
import { makeEffects } from './EffectCreator'
type MaybePromise<T> = T | Promise<T>
export const matchRpcResult = z.union([
  z.object({ result: z.any() }),
  z.object({
    error: z.object({
      code: z.number(),
      message: z.string(),
      data: z
        .object({
          details: z.string().optional(),
          debug: z.any().optional(),
        })
        .nullable()
        .optional(),
    }),
  }),
])

export type RpcResult = z.infer<typeof matchRpcResult>
type SocketResponse = ({ jsonrpc: '2.0'; id: IdType } & RpcResult) | null

const SOCKET_PARENT = '/media/startos/rpc'
const SOCKET_PATH = '/media/startos/rpc/service.sock'
const jsonrpc = '2.0' as const

const isResultSchema = z.object({ result: z.any() })
const isResult = (v: unknown): v is z.infer<typeof isResultSchema> =>
  isResultSchema.safeParse(v).success

const idType = z.union([z.string(), z.number(), z.literal(null)])
type IdType = null | string | number | undefined
const runType = z.object({
  id: idType.optional(),
  method: z.literal('execute'),
  params: z.object({
    id: z.string(),
    procedure: z.string(),
    input: z.any(),
    timeout: z.number().nullable().optional(),
  }),
})
const sandboxRunType = z.object({
  id: idType.optional(),
  method: z.literal('sandbox'),
  params: z.object({
    id: z.string(),
    procedure: z.string(),
    input: z.any(),
    timeout: z.number().nullable().optional(),
  }),
})
const callbackType = z.object({
  method: z.literal('callback'),
  params: z.object({
    id: z.number(),
    args: z.array(z.unknown()),
  }),
})
const initType = z.object({
  id: idType.optional(),
  method: z.literal('init'),
  params: z.object({
    id: z.string(),
    kind: z.enum(['install', 'update', 'restore']).nullable(),
  }),
})
const startType = z.object({
  id: idType.optional(),
  method: z.literal('start'),
})
const stopType = z.object({
  id: idType.optional(),
  method: z.literal('stop'),
})
const exitType = z.object({
  id: idType.optional(),
  method: z.literal('exit'),
  params: z.object({
    id: z.string(),
    target: z.string().nullable(),
  }),
})
const evalType = z.object({
  id: idType.optional(),
  method: z.literal('eval'),
  params: z.object({
    script: z.string(),
  }),
})

const isDevBuild = (process.env.STARTOS_ENVIRONMENT ?? '')
  .split('-')
  .includes('dev')

const jsonParse = (x: string) => JSON.parse(x)

const handleRpc = (id: IdType, result: Promise<RpcResult>) =>
  result
    .then(result => {
      return {
        jsonrpc,
        id,
        ...result,
      }
    })
    .then(x => {
      if (
        ('result' in x && x.result === undefined) ||
        !('error' in x || 'result' in x)
      )
        (x as any).result = null
      return x
    })
    .catch(error => ({
      jsonrpc,
      id,
      error: {
        code: 0,
        message: typeof error,
        data: { details: '' + error, debug: error?.stack },
      },
    }))

const hasIdSchema = z.object({ id: idType })
const hasId = (v: unknown): v is z.infer<typeof hasIdSchema> =>
  hasIdSchema.safeParse(v).success
export class RpcListener {
  shouldExit = false
  unixSocketServer = net.createServer(async server => {})
  private _system: System | undefined
  private callbacks: CallbackHolder | undefined

  constructor(readonly getDependencies: AllGetDependencies) {
    if (!fs.existsSync(SOCKET_PARENT)) {
      fs.mkdirSync(SOCKET_PARENT, { recursive: true })
    }
    if (fs.existsSync(SOCKET_PATH)) fs.rmSync(SOCKET_PATH, { force: true })

    this.unixSocketServer.listen(SOCKET_PATH)

    console.log('Listening on %s', SOCKET_PATH)

    this.unixSocketServer.on('connection', s => {
      let id: IdType = null
      let lineBuffer = ''
      const captureId = <X>(x: X) => {
        if (hasId(x)) id = x.id
        return x
      }
      const logData =
        (location: string) =>
        <X>(x: X) => {
          if (isDevBuild)
            console.log({
              location,
              stringified: JSON.stringify(x),
              type: typeof x,
              id,
            })
          return x
        }
      const mapError = (error: any): SocketResponse => ({
        jsonrpc,
        id,
        error: {
          message: typeof error,
          data: {
            details: error?.message ?? String(error),
            debug: error?.stack,
          },
          code: 1,
        },
      })
      const writeDataToSocket = (x: SocketResponse) => {
        if (x != null) {
          return new Promise(resolve =>
            s.write(JSON.stringify(x) + '\n', resolve),
          )
        }
      }
      s.on('data', a => {
        lineBuffer += a.toString()
        const lines = lineBuffer.split('\n')
        lineBuffer = lines.pop()! // keep incomplete trailing chunk in buffer
        for (const line of lines) {
          if (line)
            Promise.resolve(line)
              .then(logData('dataIn'))
              .then(jsonParse)
              .then(captureId)
              .then(x => this.dealWithInput(x))
              .catch(mapError)
              .then(logData('response'))
              .then(writeDataToSocket)
              .then(_ => {
                if (this.shouldExit) {
                  process.exit(0)
                }
              })
              .catch(e => {
                console.error(`Major error in socket handling: ${e}`)
                console.debug(`Data in: ${line}`)
              })
        }
      })
    })
  }

  private get system() {
    if (!this._system) throw new Error('System not initialized')
    return this._system
  }

  callCallback(callback: number, args: any[]): void {
    if (this.callbacks) {
      this.callbacks
        .callCallback(callback, args)
        .catch(error =>
          console.error(`callback ${callback} failed`, utils.asError(error)),
        )
    } else {
      console.warn(
        `callback ${callback} ignored because system is not initialized`,
      )
    }
  }

  private dealWithInput(input: unknown): MaybePromise<SocketResponse> {
    const parsed = z.object({ method: z.string() }).safeParse(input)
    if (!parsed.success) {
      console.warn(
        `Couldn't parse the following input ${JSON.stringify(input)}`,
      )
      return {
        jsonrpc,
        id: (input as any)?.id,
        error: {
          code: -32602,
          message: 'invalid params',
          data: {
            details: JSON.stringify(input),
          },
        },
      }
    }

    switch (parsed.data.method) {
      case 'execute': {
        const { id, params } = runType.parse(input)
        const system = this.system
        const procedure = jsonPath.parse(params.procedure)
        const { input: inp, timeout, id: eventId } = params
        const result = this.getResult(procedure, system, eventId, timeout, inp)

        return handleRpc(id, result)
      }
      case 'sandbox': {
        const { id, params } = sandboxRunType.parse(input)
        const system = this.system
        const procedure = jsonPath.parse(params.procedure)
        const { input: inp, timeout, id: eventId } = params
        const result = this.getResult(procedure, system, eventId, timeout, inp)

        return handleRpc(id, result)
      }
      case 'callback': {
        const {
          params: { id, args },
        } = callbackType.parse(input)
        this.callCallback(id, args)
        return null
      }
      case 'start': {
        const { id } = startType.parse(input)
        const callbacks =
          this.callbacks?.getChild('main') || this.callbacks?.child('main')
        const effects = makeEffects({
          eventId: null,
          callbacks,
        })
        return handleRpc(
          id,
          this.system.start(effects).then(result => ({ result })),
        )
      }
      case 'stop': {
        const { id } = stopType.parse(input)
        this.callbacks?.removeChild('main')
        return handleRpc(
          id,
          this.system.stop().then(result => ({ result })),
        )
      }
      case 'exit': {
        const { id, params } = exitType.parse(input)
        return handleRpc(
          id,
          (async () => {
            if (this._system) {
              let target = null
              if (params.target)
                try {
                  target = ExtendedVersion.parse(params.target)
                } catch (_) {
                  target = VersionRange.parse(params.target).normalize()
                }
              await this._system.exit(
                makeEffects({
                  eventId: params.id,
                }),
                target,
              )
              this.shouldExit = true
            }
          })().then(result => ({ result })),
        )
      }
      case 'init': {
        const { id, params } = initType.parse(input)
        return handleRpc(
          id,
          (async () => {
            if (!this._system) {
              const system = await this.getDependencies.system()
              this.callbacks = new CallbackHolder(
                makeEffects({
                  eventId: params.id,
                }),
              )
              const callbacks = this.callbacks.child('init')
              console.error('Initializing...')
              await system.init(
                makeEffects({
                  eventId: params.id,
                  callbacks,
                }),
                params.kind,
              )
              console.error('Initialization complete.')
              this._system = system
            }
          })().then(result => ({ result })),
        )
      }
      case 'eval': {
        const { id, params } = evalType.parse(input)
        return handleRpc(
          id,
          (async () => {
            const result = await new Function(
              `return (async () => { return (${params.script}) }).call(this)`,
            ).call({
              listener: this,
              require: require,
            })
            return {
              jsonrpc,
              id,
              result: ![
                'string',
                'number',
                'boolean',
                'null',
                'object',
              ].includes(typeof result)
                ? null
                : result,
            }
          })(),
        )
      }
      default: {
        const { id, method } = z
          .object({ id: idType.optional(), method: z.string() })
          .passthrough()
          .parse(input)
        return {
          jsonrpc,
          id,
          error: {
            code: -32601,
            message: 'Method not found',
            data: {
              details: method,
            },
          },
        }
      }
    }
  }
  private getResult(
    procedure: z.infer<typeof jsonPath>,
    system: System,
    eventId: string,
    timeout: number | null | undefined,
    input: any,
  ) {
    const ensureResultTypeShape = (
      result: void | T.ActionInput | T.ActionResult | null,
    ): { result: any } => {
      return { result }
    }
    const callbacks = this.callbacks?.child(procedure)
    const effects = makeEffects({
      eventId,
      callbacks,
    })

    return (async () => {
      switch (procedure) {
        case '/backup/create':
          return system.createBackup(effects, timeout || null)
        default:
          const procedures = unNestPath(procedure)
          switch (true) {
            case procedures[1] === 'actions' && procedures[3] === 'getInput':
              return system.getActionInput(
                effects,
                procedures[2],
                input?.prefill ?? null,
                timeout || null,
              )
            case procedures[1] === 'actions' && procedures[3] === 'run':
              return system.runAction(
                effects,
                procedures[2],
                input.input,
                timeout || null,
              )
          }
      }
    })().then(ensureResultTypeShape, error => {
      const errorSchema = z.object({
        error: z.string(),
        code: z.number().default(0),
      })
      const parsed = errorSchema.safeParse(error)
      if (parsed.success) {
        return {
          error: { code: parsed.data.code, message: parsed.data.error },
        }
      }
      return { error: { code: 0, message: String(error) } }
    })
  }
}
