// deno-lint-ignore no-namespace
export type ExpectedExports = {
  version: 2
  /** Set configuration is called after we have modified and saved the configuration in the embassy ui. Use this to make a file for the docker to read from for configuration.  */
  setConfig: (effects: Effects, input: Config) => Promise<ResultType<SetResult>>
  /** Get configuration returns a shape that describes the format that the embassy ui will generate, and later send to the set config  */
  getConfig: (effects: Effects) => Promise<ResultType<ConfigRes>>
  /** These are how we make sure the our dependency configurations are valid and if not how to fix them. */
  dependencies: Dependencies
  /** For backing up service data though the embassyOS UI */
  createBackup: (effects: Effects) => Promise<ResultType<unknown>>
  /** For restoring service data that was previously backed up using the embassyOS UI create backup flow. Backup restores are also triggered via the embassyOS UI, or doing a system restore flow during setup. */
  restoreBackup: (effects: Effects) => Promise<ResultType<unknown>>
  /**  Properties are used to get values from the docker, like a username + password, what ports we are hosting from */
  properties: (effects: Effects) => Promise<ResultType<Properties>>
  health: {
    /** Should be the health check id */
    [id: string]: (
      effects: Effects,
      dateMs: number,
    ) => Promise<ResultType<unknown>>
  }
  migration: (
    effects: Effects,
    version: string,
    ...args: unknown[]
  ) => Promise<ResultType<MigrationRes>>
  action: {
    [id: string]: (
      effects: Effects,
      config?: Config,
    ) => Promise<ResultType<ActionResult>>
  }

  /**
   * This is the entrypoint for the main container. Used to start up something like the service that the
   * package represents, like running a bitcoind in a bitcoind-wrapper.
   */
  main: (effects: Effects) => Promise<ResultType<unknown>>
}

/** Used to reach out from the pure js runtime */
export type Effects = {
  /** Usable when not sandboxed */
  writeFile(input: {
    path: string
    volumeId: string
    toWrite: string
  }): Promise<void>
  readFile(input: { volumeId: string; path: string }): Promise<string>
  metadata(input: { volumeId: string; path: string }): Promise<Metadata>
  /** Create a directory. Usable when not sandboxed */
  createDir(input: { volumeId: string; path: string }): Promise<string>

  readDir(input: { volumeId: string; path: string }): Promise<string[]>
  /** Remove a directory. Usable when not sandboxed */
  removeDir(input: { volumeId: string; path: string }): Promise<string>
  removeFile(input: { volumeId: string; path: string }): Promise<void>

  /** Write a json file into an object. Usable when not sandboxed */
  writeJsonFile(input: {
    volumeId: string
    path: string
    toWrite: Record<string, unknown>
  }): Promise<void>

  /** Read a json file into an object */
  readJsonFile(input: {
    volumeId: string
    path: string
  }): Promise<Record<string, unknown>>

  runCommand(input: {
    command: string
    args?: string[]
    timeoutMillis?: number
  }): Promise<ResultType<string>>
  runDaemon(input: { command: string; args?: string[] }): {
    wait(): Promise<ResultType<string>>
    term(): Promise<void>
  }

  chown(input: { volumeId: string; path: string; uid: string }): Promise<null>
  chmod(input: { volumeId: string; path: string; mode: string }): Promise<null>

  sleep(timeMs: number): Promise<null>

  /** Log at the trace level */
  trace(whatToPrint: string): void
  /** Log at the warn level */
  warn(whatToPrint: string): void
  /** Log at the error level */
  error(whatToPrint: string): void
  /** Log at the debug level */
  debug(whatToPrint: string): void
  /** Log at the info level */
  info(whatToPrint: string): void

  /** Sandbox mode lets us read but not write */
  is_sandboxed(): boolean

  // Does a volume and path exist?
  exists(input: { volumeId: string; path: string }): Promise<boolean>

  fetch(
    url: string,
    options?: {
      method?: 'GET' | 'POST' | 'PUT' | 'DELETE' | 'HEAD' | 'PATCH'
      headers?: Record<string, string>
      body?: string
    },
  ): Promise<{
    method: string
    ok: boolean
    status: number
    headers: Record<string, string>
    body?: string | null
    /// Returns the body as a string
    text(): Promise<string>
    /// Returns the body as a json
    json(): Promise<unknown>
  }>
  diskUsage(options?: {
    volumeId: string
    path: string
  }): Promise<{ used: number; total: number }>

  runRsync(options: {
    srcVolume: string
    dstVolume: string
    srcPath: string
    dstPath: string
    // rsync options: https://linux.die.net/man/1/rsync
    options: BackupOptions
  }): {
    id: () => Promise<string>
    wait: () => Promise<null>
    progress: () => Promise<number>
  }
}

// rsync options: https://linux.die.net/man/1/rsync
export type BackupOptions = {
  delete: boolean
  force: boolean
  ignoreExisting: boolean
  exclude: string[]
}
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

export type MigrationRes = {
  configured: boolean
}

export type ActionResult = {
  version: '0'
  message: string
  value?: string
  copyable: boolean
  qr: boolean
}

export type ConfigRes = {
  /** This should be the previous config, that way during set config we start with the previous */
  config?: Config
  /** Shape that is describing the form in the ui */
  spec: ConfigSpec
}
export type Config = {
  [propertyName: string]: unknown
}

export type ConfigSpec = {
  /** Given a config value, define what it should render with the following spec */
  [configValue: string]: ValueSpecAny
}
export type WithDefault<T, Default> = T & {
  default: Default
}
export type WithNullableDefault<T, Default> = T & {
  default?: Default
}

export type WithDescription<T> = T & {
  description?: string
  name: string
  warning?: string
}

export type WithOptionalDescription<T> = T & {
  /** @deprecated - optional only for backwards compatibility */
  description?: string
  /** @deprecated - optional only for backwards compatibility */
  name?: string
  warning?: string
}

export type ListSpec<T> = {
  spec: T
  range: string
}

export type Tag<T extends string, V> = V & {
  type: T
}

export type Subtype<T extends string, V> = V & {
  subtype: T
}

export type Target<T extends string, V> = V & {
  target: T
}

export type UniqueBy =
  | {
      any: UniqueBy[]
    }
  | string
  | null

export type WithNullable<T> = T & {
  nullable: boolean
}
export type DefaultString =
  | string
  | {
      /** The chars available for the random generation */
      charset?: string
      /** Length that we generate to */
      len: number
    }

export type ValueSpecString = // deno-lint-ignore ban-types
  (
    | {}
    | {
        pattern: string
        'pattern-description': string
      }
  ) & {
    copyable?: boolean
    masked?: boolean
    placeholder?: string
  }
export type ValueSpecNumber = {
  /** Something like [3,6] or [0, *) */
  range?: string
  integral?: boolean
  /** Used a description of the units */
  units?: string
  placeholder?: number
}
export type ValueSpecBoolean = Record<string, unknown>
export type ValueSpecAny =
  | Tag<'boolean', WithDescription<WithDefault<ValueSpecBoolean, boolean>>>
  | Tag<
      'string',
      WithDescription<
        WithNullableDefault<WithNullable<ValueSpecString>, DefaultString>
      >
    >
  | Tag<
      'number',
      WithDescription<
        WithNullableDefault<WithNullable<ValueSpecNumber>, number>
      >
    >
  | Tag<
      'enum',
      WithDescription<
        WithDefault<
          {
            values: readonly string[] | string[]
            'value-names': {
              [key: string]: string
            }
          },
          string
        >
      >
    >
  | Tag<'list', ValueSpecList>
  | Tag<'object', WithDescription<WithNullableDefault<ValueSpecObject, Config>>>
  | Tag<'union', WithOptionalDescription<WithDefault<ValueSpecUnion, string>>>
  | Tag<
      'pointer',
      WithDescription<
        | Subtype<
            'package',
            | Target<
                'tor-key',
                {
                  'package-id': string
                  interface: string
                }
              >
            | Target<
                'tor-address',
                {
                  'package-id': string
                  interface: string
                }
              >
            | Target<
                'lan-address',
                {
                  'package-id': string
                  interface: string
                }
              >
            | Target<
                'config',
                {
                  'package-id': string
                  selector: string
                  multi: boolean
                }
              >
          >
        | Subtype<'system', Record<string, unknown>>
      >
    >
export type ValueSpecUnion = {
  /** What tag for the specification, for tag unions */
  tag: {
    id: string
    name: string
    description?: string
    'variant-names': {
      [key: string]: string
    }
  }
  /** The possible enum values */
  variants: {
    [key: string]: ConfigSpec
  }
  'display-as'?: string
  'unique-by'?: UniqueBy
}
export type ValueSpecObject = {
  spec: ConfigSpec
  'display-as'?: string
  'unique-by'?: UniqueBy
}
export type ValueSpecList =
  | Subtype<
      'boolean',
      WithDescription<WithDefault<ListSpec<ValueSpecBoolean>, boolean[]>>
    >
  | Subtype<
      'string',
      WithDescription<WithDefault<ListSpec<ValueSpecString>, string[]>>
    >
  | Subtype<
      'number',
      WithDescription<WithDefault<ListSpec<ValueSpecNumber>, number[]>>
    >
  | Subtype<
      'enum',
      WithDescription<WithDefault<ListSpec<ValueSpecEnum>, string[]>>
    >
  | Subtype<
      'object',
      WithDescription<
        WithNullableDefault<
          ListSpec<ValueSpecObject>,
          Record<string, unknown>[]
        >
      >
    >
  | Subtype<
      'union',
      WithDescription<WithDefault<ListSpec<ValueSpecUnion>, string[]>>
    >
export type ValueSpecEnum = {
  values: string[]
  'value-names': { [key: string]: string }
}

export type SetResult = {
  /** These are the unix process signals */
  signal:
    | 'SIGTERM'
    | 'SIGHUP'
    | 'SIGINT'
    | 'SIGQUIT'
    | 'SIGILL'
    | 'SIGTRAP'
    | 'SIGABRT'
    | 'SIGBUS'
    | 'SIGFPE'
    | 'SIGKILL'
    | 'SIGUSR1'
    | 'SIGSEGV'
    | 'SIGUSR2'
    | 'SIGPIPE'
    | 'SIGALRM'
    | 'SIGSTKFLT'
    | 'SIGCHLD'
    | 'SIGCONT'
    | 'SIGSTOP'
    | 'SIGTSTP'
    | 'SIGTTIN'
    | 'SIGTTOU'
    | 'SIGURG'
    | 'SIGXCPU'
    | 'SIGXFSZ'
    | 'SIGVTALRM'
    | 'SIGPROF'
    | 'SIGWINCH'
    | 'SIGIO'
    | 'SIGPWR'
    | 'SIGSYS'
    | 'SIGEMT'
    | 'SIGINFO'
  'depends-on': DependsOn
}

export type DependsOn = {
  [packageId: string]: string[]
}

export type KnownError =
  | { error: string }
  | {
      'error-code': [number, string] | readonly [number, string]
    }
export type ResultType<T> = KnownError | { result: T }

export type PackagePropertiesV2 = {
  [name: string]: PackagePropertyObject | PackagePropertyString
}
export type PackagePropertyString = {
  type: 'string'
  description?: string
  value: string
  /** Let's the ui make this copyable button */
  copyable?: boolean
  /** Let the ui create a qr for this field */
  qr?: boolean
  /** Hiding the value unless toggled off for field */
  masked?: boolean
}
export type PackagePropertyObject = {
  value: PackagePropertiesV2
  type: 'object'
  description: string
}

export type Properties = {
  version: 2
  data: PackagePropertiesV2
}

export type Dependencies = {
  /** Id is the id of the package, should be the same as the manifest */
  [id: string]: {
    /** Checks are called to make sure that our dependency is in the correct shape. If a known error is returned we know that the dependency needs modification */
    check(effects: Effects, input: Config): Promise<ResultType<void | null>>
    /** This is called after we know that the dependency package needs a new configuration, this would be a transform for defaults */
    autoConfigure(effects: Effects, input: Config): Promise<ResultType<Config>>
  }
}
