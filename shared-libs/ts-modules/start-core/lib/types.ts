export * as inputSpecTypes from './actions/input/inputSpecTypes'
export * from './osBindings'
export { SDKManifest } from './types/ManifestTypes'
export { Effects }
import { InputSpec as InputSpecClass } from './actions/input/builder/inputSpec'

import { Action, Actions } from './actions/setupActions'
import { Effects } from './Effects'
import { ExtendedVersion, VersionRange } from './exver'
import {
  ActionId,
  DependencyRequirement,
  Manifest,
  NamedHealthCheckResult,
  ServiceInterface,
} from './osBindings'
import { StringObject, ToKebab } from './util'

/** An object that can be built into a terminable daemon process. */
export type DaemonBuildable = {
  build(): Promise<{
    term(): Promise<void>
  }>
}

/** The three categories of service network interfaces. */
export type ServiceInterfaceType = 'ui' | 'p2p' | 'api'
/** A Node.js signal name (e.g. `"SIGTERM"`, `"SIGKILL"`). */
export type Signals = NodeJS.Signals
/** The SIGTERM signal — used for graceful daemon termination. */
export const SIGTERM: Signals = 'SIGTERM'
/** The SIGKILL signal — used for forceful daemon termination. */
export const SIGKILL: Signals = 'SIGKILL'
/** Sentinel value (`-1`) indicating that no timeout should be applied. */
export const NO_TIMEOUT = -1

/** A function that builds an absolute file path from a volume name and relative path. */
export type PathMaker = (options: { volume: string; path: string }) => string
/** A value that may or may not be wrapped in a `Promise`. */
export type MaybePromise<A> = Promise<A> | A
/**
 * Namespace defining the required exports for a StartOS service package.
 * Every package must export implementations matching these types.
 */
export namespace ExpectedExports {
  /** For backing up service data though the startOS UI */
  export type createBackup = (options: { effects: Effects }) => Promise<unknown>

  /**
   * This is the entrypoint for the main container. Used to start up something like the service that the
   * package represents, like running a bitcoind in a bitcoind-wrapper.
   */
  export type main = (options: { effects: Effects }) => Promise<DaemonBuildable>

  /**
   * Every time a service launches (both on startup, and on install) this function is called before packageInit
   * Can be used to register callbacks
   */
  export type init = (options: {
    effects: Effects
    kind: 'install' | 'update' | 'restore' | null
  }) => Promise<unknown>
  /** This will be ran during any time a package is uninstalled, for example during a update
   * this will be called.
   */
  export type uninit = (options: {
    effects: Effects
    target: ExtendedVersion | VersionRange | null
  }) => Promise<unknown>

  /** The package manifest describing the service's metadata, dependencies, and interfaces. */
  export type manifest = Manifest

  /** The map of user-invocable actions defined by this service. */
  export type actions = Actions<Record<ActionId, Action<ActionId, any>>>
}
/**
 * The complete ABI (Application Binary Interface) for a StartOS service package.
 * Maps all required exports to their expected types.
 */
export type ABI = {
  createBackup: ExpectedExports.createBackup
  main: ExpectedExports.main
  init: ExpectedExports.init
  uninit: ExpectedExports.uninit
  manifest: ExpectedExports.manifest
  actions: ExpectedExports.actions
}
/** A time value in milliseconds. */
export type TimeMs = number
/** A version string in string form. */
export type VersionString = string

declare const DaemonProof: unique symbol
/** Opaque branded type proving that a daemon was started. Cannot be constructed directly. */
export type DaemonReceipt = {
  [DaemonProof]: never
}
/** A running daemon with methods to wait for completion or terminate it. */
export type Daemon = {
  /** Waits for the daemon to exit and returns its exit message. */
  wait(): Promise<string>
  /** Terminates the daemon. */
  term(): Promise<null>
  [DaemonProof]: never
}

/** The result status of a health check (extracted from `NamedHealthCheckResult`). */
export type HealthStatus = NamedHealthCheckResult['result']
/** SMTP mail server configuration values. */
export type SmtpValue = {
  host: string
  port: number
  from: string
  username: string
  password: string | null | undefined
  security: 'starttls' | 'tls'
}

/**
 * Marker class indicating that a container should use its own built-in entrypoint
 * rather than a custom command. Optionally accepts an override command array.
 */
export class UseEntrypoint {
  readonly USE_ENTRYPOINT = 'USE_ENTRYPOINT'
  constructor(readonly overridCmd?: string[]) {}
}
/** Type guard that checks if a {@link CommandType} is a {@link UseEntrypoint} instance. */
export function isUseEntrypoint(
  command: CommandType,
): command is UseEntrypoint {
  return typeof command === 'object' && 'USE_ENTRYPOINT' in command
}

/**
 * The ways to specify a command to run in a container:
 * - A shell string (run via `sh -c`)
 * - An explicit argv array
 * - A {@link UseEntrypoint} to use the container's built-in entrypoint
 */
export type CommandType =
  | string
  | [string, ...string[]]
  | readonly [string, ...string[]]
  | UseEntrypoint

/** The return type from starting a daemon — provides `wait()` and `term()` controls. */
export type DaemonReturned = {
  /** Waits for the daemon process to exit. */
  wait(): Promise<unknown>
  /** Sends a signal to terminate the daemon. If it doesn't exit within `timeout` ms, sends SIGKILL. */
  term(options?: { signal?: Signals; timeout?: number }): Promise<null>
}

export declare const hostName: unique symbol
/** A branded string type for hostnames (e.g. `.onion` addresses or IP addresses). */
export type Hostname = string & { [hostName]: never }

/** A string identifier for a service network interface. */
export type ServiceInterfaceId = string

export { ServiceInterface }

/** Maps effect method names to their kebab-case RPC equivalents. */
export type EffectMethod<T extends StringObject = Effects> = {
  [K in keyof T]-?: K extends string
    ? T[K] extends Function
      ? ToKebab<K>
      : T[K] extends StringObject
        ? `${ToKebab<K>}.${EffectMethod<T[K]>}`
        : never
    : never
}[keyof T]

/** Options for rsync-based file synchronization (used in backup/restore). */
export type SyncOptions = {
  /** delete files that exist in the target directory, but not in the source directory */
  delete: boolean
  /** do not sync files with paths that match these patterns */
  exclude: string[]
}

/**
 * This is the metadata that is returned from the metadata call.
 */
export type Metadata = {
  fileType: string
  isDir: boolean
  isFile: boolean
  isSymlink: boolean
  len: number
  modified?: Date
  accessed?: Date
  created?: Date
  readonly: boolean
  uid: number
  gid: number
  mode: number
}

/** Result type for setting a service's dependency configuration and restart signal. */
export type SetResult = {
  dependsOn: DependsOn
  signal: Signals
}

/** A string identifier for a StartOS package (e.g. `"bitcoind"`). */
export type PackageId = string
/** A user-facing message string. */
export type Message = string
/** Whether a dependency needs to be actively running or merely installed. */
export type DependencyKind = 'running' | 'exists'

/**
 * Maps package IDs to the health check IDs that must pass before this service considers
 * the dependency satisfied.
 */
export type DependsOn = {
  [packageId: string]: string[] | readonly string[]
}

/**
 * A typed error that can be displayed to the user.
 * Either a plain error message string, or a structured error code with description.
 */
export type KnownError =
  | { error: string }
  | {
      errorCode: [number, string] | readonly [number, string]
    }

/** An array of dependency requirements for a service. */
export type Dependencies = Array<DependencyRequirement>

/** Recursively makes all properties of `T` optional. */
export type DeepPartial<T> = T extends [infer A, ...infer Rest]
  ? [DeepPartial<A>, ...DeepPartial<Rest>]
  : T extends {}
    ? { [P in keyof T]?: DeepPartial<T[P]> }
    : T

/** Recursively removes all `readonly` modifiers from `T`. */
export type DeepWritable<T> = {
  -readonly [K in keyof T]: T[K]
}

/** Casts a value to {@link DeepWritable} (identity at runtime, removes `readonly` at the type level). */
export function writable<T>(value: T): DeepWritable<T> {
  return value
}

/** Recursively makes all properties of `T` readonly. */
export type DeepReadonly<T> = {
  readonly [P in keyof T]: DeepReadonly<T[P]>
}

/** Casts a value to {@link DeepReadonly} (identity at runtime, adds `readonly` at the type level). */
export function readonly<T>(value: T): DeepReadonly<T> {
  return value
}

/** Accepts either a mutable or deeply-readonly version of `T`. */
export type AllowReadonly<T> =
  | T
  | {
      readonly [P in keyof T]: AllowReadonly<T[P]>
    }

export type InputSpec<
  Type extends StaticValidatedAs,
  StaticValidatedAs extends Record<string, unknown> = Type,
> = InputSpecClass<Type, StaticValidatedAs>
