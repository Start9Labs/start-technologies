import {
  ExtendedVersion,
  types as T,
  VersionRange,
} from '@start9labs/start-sdk'
import { Effects } from '../Models/Effects'
import { CallbackHolder } from '../Models/CallbackHolder'

export type Procedure =
  | '/backup/create'
  | `/actions/${string}/getInput`
  | `/actions/${string}/run`

export type ExecuteResult =
  | { ok: unknown }
  | { err: { code: number; message: string } }
export type System = {
  init(
    effects: T.Effects,
    kind: 'install' | 'update' | 'restore' | null,
  ): Promise<void>

  start(effects: T.Effects): Promise<void>
  stop(): Promise<void>

  createBackup(effects: T.Effects, timeoutMs: number | null): Promise<void>
  runAction(
    effects: Effects,
    actionId: string,
    input: unknown,
    timeoutMs: number | null,
  ): Promise<T.ActionResult | null>
  getActionInput(
    effects: Effects,
    actionId: string,
    prefill: Record<string, unknown> | null,
    timeoutMs: number | null,
  ): Promise<T.ActionInput | null>

  exit(
    effects: Effects,
    target: ExtendedVersion | VersionRange | null,
  ): Promise<void>
}

export type RunningMain = {
  callbacks: CallbackHolder
  stop(): Promise<void>
}
