import * as TOML from '@iarna/toml'
import {
  XMLBuilder,
  XMLParser,
  type X2jOptions,
  type XmlBuilderOptions,
} from 'fast-xml-parser'
import * as INI from 'ini'
import * as fs from 'node:fs/promises'
import * as YAML from 'yaml'
import { z } from '@start9labs/start-core/zExport'
import * as T from '@start9labs/start-core/types'
import { asError, deepEqual } from '@start9labs/start-core/util'
import { MappedWatchable } from '@start9labs/start-core/util/Watchable'
import { PathBase } from './Volume'
import {
  filePath,
  replaceFile,
  withFileLock,
  withFileQueue,
  withUpdateDeadline,
} from './fileAccess'
import { hasFileMounts, refreshFileMounts } from './fileMounts'

async function readRaw(path: string): Promise<string | null> {
  return fs.readFile(path, 'utf-8').catch(error => {
    if (error.code !== 'ENOENT') throw error
    return null
  })
}

const exists = (path: string) =>
  fs.access(path).then(
    () => true,
    () => false,
  )

/** Starts watching at once; `fs.watch` defers until its first read. */
function watchPath(path: string, abort: AbortSignal) {
  const ctrl = new AbortController()
  const onAbort = () => ctrl.abort()
  abort.addEventListener('abort', onAbort, { once: true })
  const events = fs.watch(path, { persistent: false, signal: ctrl.signal })
  const first = events.next()
  first.catch(() => {})
  return {
    events,
    first,
    stop: () => {
      abort.removeEventListener('abort', onAbort)
      ctrl.abort()
    },
  }
}

async function onCreated(path: string, abort: AbortSignal) {
  if (path === '/' || abort.aborted) return
  if (!path.startsWith('/')) path = `${process.cwd()}/${path}`
  if (await exists(path)) {
    return
  }
  const split = path.split('/')
  const filename = split.pop()
  const parent = split.join('/')
  await onCreated(parent, abort)
  if (abort.aborted) return
  const watch = watchPath(parent, abort)
  try {
    if (await exists(path)) return
    for (let r = await watch.first; !r.done; r = await watch.events.next()) {
      if (r.value.filename === filename) return
    }
  } finally {
    watch.stop()
  }
}

function fileMerge(...args: any[]): any {
  let res = args.shift()
  for (const arg of args) {
    if (res === arg) continue
    else if (
      res &&
      arg &&
      typeof res === 'object' &&
      typeof arg === 'object' &&
      !Array.isArray(res) &&
      !Array.isArray(arg)
    ) {
      for (const key of Object.keys(arg)) {
        res[key] = fileMerge(res[key], arg[key])
      }
    } else res = arg
  }
  return res
}

function filterUndefined<A>(a: A): A {
  if (a && typeof a === 'object') {
    if (Array.isArray(a)) {
      return a.map(filterUndefined) as A
    }
    return Object.entries(a).reduce<Record<string, any>>((acc, [k, v]) => {
      if (v !== undefined) {
        acc[k] = filterUndefined(v)
      }
      return acc
    }, {}) as A
  }
  return a
}

/**
 * Maps between the parsed file format and the schema's input.
 *
 * @typeParam Raw - The value the file format parses to
 * @typeParam Transformed - The value the schema validates
 */
export type Transformers<
  Raw = unknown,
  Transformed = unknown,
  Validated extends Transformed = Transformed,
> = {
  /** Converts the parsed file into the schema's input. */
  onRead: (value: Raw) => Transformed
  /** Converts validated data into the value the file format serializes. */
  onWrite: (value: Validated) => Raw
}

type ToPath = string | { base: PathBase; subpath: string }
function toPath(path: ToPath): string {
  if (typeof path === 'string') {
    return path
  }
  return path.base.subpath(path.subpath)
}

type Validator<_T, U> = z.ZodType<U>

type ReadType<A> = {
  once: () => Promise<A | null>
  const: (effects: T.Effects) => Promise<A | null>
  watch: (
    effects: T.Effects,
    abort?: AbortSignal,
  ) => AsyncGenerator<A | null, never, unknown>
  onChange: (
    effects: T.Effects,
    callback: (
      value: A | null,
      error?: Error,
    ) => { cancel: boolean } | Promise<{ cancel: boolean }>,
  ) => void
  waitFor: (
    effects: T.Effects,
    pred: (value: A | null) => boolean,
    abort?: AbortSignal,
  ) => Promise<A | null>
}

/**
 * A validated file with reactive reads and serialized writes.
 *
 * The schema mirrors the file: a value the upstream service does not require is
 * optional, even where the package always sets it. Give every key a `.catch()`
 * default, so a hand-edited value of the wrong type falls back to it.
 *
 * @example
 * ```ts
 * import { FileHelper, z } from '@start9labs/start-sdk'
 *
 * export const configToml = FileHelper.toml(
 *   { base: sdk.volumes.main, subpath: 'config.toml' },
 *   z.object({
 *     port: z.number().catch(8080),
 *     allow_registration: z.boolean().catch(false),
 *   }),
 * )
 * ```
 */
export interface FileHelper<A> {
  readonly path: string
  readonly writeData: (dataIn: A) => string
  readonly readData: (stringValue: string) => unknown
  readonly validate: (value: unknown) => A
  /**
   * Creates a reactive reader for this file.
   *
   * - `once()` - Reads the file once
   * - `const(effects)` - Reads once and re-runs the enclosing context when the value changes
   * - `watch(effects)` - Yields the value on each change
   * - `onChange(effects, callback)` - Calls back on each change
   * - `waitFor(effects, predicate, abort?)` - Resolves once the value satisfies the predicate
   *
   * @param map - Transforms the validated value
   * @param eq - Deduplicates emissions of the mapped value
   */
  read(): ReadType<A>
  read<B>(
    map: (value: A) => B,
    eq?: (left: B | null, right: B | null) => boolean,
  ): ReadType<B>
  /** Replaces the file with validated data; bind-mounted targets are written in place. */
  write(
    effects: T.Effects,
    data: T.AllowReadonly<A> | A,
    options?: { allowWriteAfterConst?: boolean },
  ): Promise<null>
  /** Deep-merges partial data into the file under the writer lock; arrays are replaced. */
  merge(
    effects: T.Effects,
    data: T.AllowReadonly<T.DeepPartial<A>>,
    options?: { allowWriteAfterConst?: boolean },
  ): Promise<null>
  /**
   * Replaces the file with the callback's result under the writer lock.
   * Returning `null` skips the write. The callback has five seconds; nested
   * mutations of the same file reject.
   */
  update(
    effects: T.Effects,
    change: (
      current: A | null,
    ) => T.AllowReadonly<A> | null | Promise<T.AllowReadonly<A> | null>,
    options?: { allowWriteAfterConst?: boolean },
  ): Promise<null>
  /** Returns a helper with the same format and schema at another path. */
  withPath(path: ToPath): FileHelper<A>
}

class FileHelperImpl<A> implements FileHelper<A> {
  private consts: [
    () => void,
    any,
    (a: any) => any,
    (left: any, right: any) => any,
  ][] = []
  constructor(
    readonly path: string,
    readonly writeData: (dataIn: A) => string,
    readonly readData: (stringValue: string) => unknown,
    readonly validate: (value: unknown) => A,
  ) {}

  private async writeLocked(
    path: string,
    data: string,
    temp: string | undefined,
    signal: AbortSignal,
  ): Promise<void> {
    await replaceFile(path, data, temp, signal)
    await refreshFileMounts(path, signal)
  }

  private async readFileRaw(): Promise<string | null> {
    if (!(await exists(this.path))) return null
    const target = await filePath(this.path)
    if (!hasFileMounts(target)) return readRaw(target)
    return withFileQueue(target, async (path, signal) => {
      await refreshFileMounts(path, signal)
      return readRaw(path)
    })
  }

  private async readFile(): Promise<unknown> {
    const raw = await this.readFileRaw()
    if (raw === null) {
      return raw
    }
    return this.readData(raw)
  }

  private async readOnce<B>(map: (value: A) => B): Promise<B | null> {
    const data = await this.readFile()
    if (!data) return null
    return map(this.validate(data))
  }

  private createFileWatchable<B>(
    effects: T.Effects,
    map: (value: A) => B,
    eq: (left: B | null, right: B | null) => boolean,
  ) {
    const doRead = async (): Promise<A | null> => {
      const data = await this.readFile()
      if (!data) return null
      return this.validate(data)
    }
    const filePath = this.path
    const fileHelper = this

    const wrappedMap = (raw: A | null): B | null => {
      if (raw === null) return null
      return map(raw)
    }

    return new (class extends MappedWatchable<A | null, B | null> {
      protected readonly label = 'FileHelper'

      protected async fetchRaw() {
        return doRead()
      }

      protected async *produceRaw(
        abort: AbortSignal,
      ): AsyncGenerator<A | null, void> {
        while (this.effects.isInContext && !abort.aborted) {
          if (await exists(filePath)) {
            const watch = watchPath(filePath, abort)
            try {
              yield await doRead()
              await watch.first.catch(e => {
                if (!abort.aborted) console.error(asError(e))
              })
            } finally {
              watch.stop()
            }
          } else {
            yield null
            await onCreated(filePath, abort).catch(e => {
              if (!abort.aborted) console.error(asError(e))
            })
          }
        }
      }

      protected onConstRegistered(value: B | null): (() => void) | void {
        if (!this.effects.constRetry) return
        const record: (typeof fileHelper.consts)[number] = [
          this.effects.constRetry,
          value,
          wrappedMap,
          eq,
        ]
        fileHelper.consts.push(record)
        return () => {
          fileHelper.consts = fileHelper.consts.filter(r => r !== record)
        }
      }
    })(effects, { map: wrappedMap, eq })
  }

  read(): ReadType<A>
  read<B>(
    map: (value: A) => B,
    eq?: (left: B | null, right: B | null) => boolean,
  ): ReadType<B>
  read(
    map?: (value: A) => any,
    eq?: (left: any, right: any) => boolean,
  ): ReadType<any> {
    map = map ?? ((a: A) => a)
    eq = eq ?? deepEqual
    return {
      once: () => this.readOnce(map),
      const: (effects: T.Effects) =>
        this.createFileWatchable(effects, map, eq).const(),
      watch: (effects: T.Effects, abort?: AbortSignal) =>
        this.createFileWatchable(effects, map, eq).watch(abort),
      onChange: (
        effects: T.Effects,
        callback: (
          value: A | null,
          error?: Error,
        ) => { cancel: boolean } | Promise<{ cancel: boolean }>,
      ) => this.createFileWatchable(effects, map, eq).onChange(callback),
      waitFor: (
        effects: T.Effects,
        pred: (value: A | null) => boolean,
        abort?: AbortSignal,
      ) => this.createFileWatchable(effects, map, eq).waitFor(pred, abort),
    }
  }

  async write(
    effects: T.Effects,
    data: T.AllowReadonly<A> | A,
    options: { allowWriteAfterConst?: boolean } = {},
  ) {
    const newData = this.validate(data)
    await withFileLock(this.path, (path, temp, signal) =>
      this.writeLocked(path, this.writeData(newData), temp, signal),
    )
    this.checkConsts(effects, newData, options)
    return null
  }

  private checkConsts(
    effects: T.Effects,
    newData: A,
    options: { allowWriteAfterConst?: boolean },
  ): void {
    if (!options.allowWriteAfterConst && effects.constRetry) {
      const records = this.consts.filter(([c]) => c === effects.constRetry)
      for (const record of records) {
        const [_, prev, map, eq] = record
        if (!eq(prev, map(newData))) {
          throw new Error(`Canceled: write after const: ${this.path}`)
        }
      }
    }
  }

  private async modify(
    effects: T.Effects,
    change: (
      raw: string | null,
      path: string,
      signal: AbortSignal,
    ) => Promise<A | null>,
    options: { allowWriteAfterConst?: boolean },
  ): Promise<null> {
    const written = await withFileLock(
      this.path,
      async (path, temp, signal) => {
        const raw = await readRaw(path)
        signal.throwIfAborted()
        const next = await change(raw, path, signal)
        if (next === null) return null
        const serialized = this.writeData(next)
        if (serialized === raw) {
          await refreshFileMounts(path, signal)
          return null
        }
        await this.writeLocked(path, serialized, temp, signal)
        return { data: next }
      },
    )
    if (written) this.checkConsts(effects, written.data, options)
    return null
  }

  async merge(
    effects: T.Effects,
    data: T.AllowReadonly<T.DeepPartial<A>>,
    options: { allowWriteAfterConst?: boolean } = {},
  ) {
    return this.modify(
      effects,
      async raw => {
        let fileData: any = raw === null ? null : this.readData(raw)
        try {
          fileData = this.validate(fileData)
        } catch (_) {}
        return this.validate(fileMerge({}, fileData, data))
      },
      options,
    )
  }

  async update(
    effects: T.Effects,
    change: (
      current: A | null,
    ) => T.AllowReadonly<A> | null | Promise<T.AllowReadonly<A> | null>,
    options: { allowWriteAfterConst?: boolean } = {},
  ): Promise<null> {
    return this.modify(
      effects,
      async (raw, path, signal) => {
        const data = raw === null ? null : this.readData(raw)
        const current = data ? this.validate(data) : null
        const next = await withUpdateDeadline(path, signal, () =>
          change(current),
        )
        return next === null ? null : this.validate(next)
      },
      options,
    )
  }

  withPath(path: ToPath): FileHelper<A> {
    return new FileHelperImpl<A>(
      toPath(path),
      this.writeData,
      this.readData,
      this.validate,
    )
  }
}

function rawTransformed<A extends Transformed, Raw, Transformed>(
  path: ToPath,
  toFile: (dataIn: Raw) => string,
  fromFile: (rawData: string) => Raw,
  validate: (data: Transformed) => A,
  transformers: Transformers<Raw, Transformed, A> | undefined,
): FileHelper<A> {
  return new FileHelperImpl<A>(
    toPath(path),
    inData =>
      toFile(
        filterUndefined(
          transformers ? transformers.onWrite(inData) : (inData as any as Raw),
        ),
      ),
    fileData => {
      if (transformers) {
        return transformers.onRead(fromFile(fileData))
      }
      return fromFile(fileData)
    },
    validate as (a: unknown) => A,
  )
}

function deepLooseParse<A>(shape: z.ZodType<A>): (data: unknown) => A {
  const loose = z.deepLoose(shape)
  return data => loose.parse(data)
}

interface FileHelperStatic {
  /** Creates a FileHelper for a custom format. */
  raw<A>(
    path: ToPath,
    toFile: (dataIn: A) => string,
    fromFile: (rawData: string) => unknown,
    validate: (data: unknown) => A,
  ): FileHelper<A>

  /** Creates a FileHelper for a text file. */
  string(path: ToPath): FileHelper<string>
  string<A extends string>(
    path: ToPath,
    shape: Validator<string, A>,
  ): FileHelper<A>
  string<A extends Transformed, Transformed = string>(
    path: ToPath,
    shape: Validator<Transformed, A>,
    transformers: Transformers<string, Transformed, A>,
  ): FileHelper<A>

  /** Creates a FileHelper for a JSON file. */
  json<A>(path: ToPath, shape: Validator<unknown, A>): FileHelper<A>
  json<A extends Transformed, Transformed = unknown>(
    path: ToPath,
    shape: Validator<unknown, A>,
    transformers: Transformers<unknown, Transformed, A>,
  ): FileHelper<A>

  /** Creates a FileHelper for a YAML file. */
  yaml<A extends Record<string, unknown>>(
    path: ToPath,
    shape: Validator<Record<string, unknown>, A>,
    options?: YAML.ParseOptions &
      YAML.DocumentOptions &
      YAML.SchemaOptions &
      YAML.ToJSOptions &
      YAML.CreateNodeOptions &
      YAML.ToStringOptions,
  ): FileHelper<A>
  yaml<A extends Transformed, Transformed = Record<string, unknown>>(
    path: ToPath,
    shape: Validator<Transformed, A>,
    options: YAML.ParseOptions &
      YAML.DocumentOptions &
      YAML.SchemaOptions &
      YAML.ToJSOptions &
      YAML.CreateNodeOptions &
      YAML.ToStringOptions,
    transformers: Transformers<Record<string, unknown>, Transformed, A>,
  ): FileHelper<A>

  /** Creates a FileHelper for a TOML file. */
  toml<A extends Record<string, unknown>>(
    path: ToPath,
    shape: Validator<Record<string, unknown>, A>,
  ): FileHelper<A>
  toml<A extends Transformed, Transformed = Record<string, unknown>>(
    path: ToPath,
    shape: Validator<Transformed, A>,
    transformers: Transformers<Record<string, unknown>, Transformed, A>,
  ): FileHelper<A>

  /** Creates a FileHelper for an INI file. */
  ini<A extends Record<string, unknown>>(
    path: ToPath,
    shape: Validator<Record<string, unknown>, A>,
    options?: INI.EncodeOptions & INI.DecodeOptions,
  ): FileHelper<A>
  ini<A extends Transformed, Transformed = Record<string, unknown>>(
    path: ToPath,
    shape: Validator<Transformed, A>,
    options: INI.EncodeOptions & INI.DecodeOptions,
    transformers: Transformers<Record<string, unknown>, Transformed, A>,
  ): FileHelper<A>

  /** Creates a FileHelper for a file of `KEY=VALUE` lines. */
  env<A extends Record<string, string>>(
    path: ToPath,
    shape: Validator<Record<string, string>, A>,
  ): FileHelper<A>
  env<A extends Transformed, Transformed = Record<string, string>>(
    path: ToPath,
    shape: Validator<Transformed, A>,
    transformers: Transformers<Record<string, string>, Transformed, A>,
  ): FileHelper<A>

  /** Creates a FileHelper for an XML file. */
  xml<A extends Record<string, unknown>>(
    path: ToPath,
    shape: Validator<Record<string, unknown>, A>,
    options?: { parser?: X2jOptions; builder?: XmlBuilderOptions },
  ): FileHelper<A>
  xml<A extends Transformed, Transformed = Record<string, unknown>>(
    path: ToPath,
    shape: Validator<Transformed, A>,
    options: { parser?: X2jOptions; builder?: XmlBuilderOptions },
    transformers: Transformers<Record<string, unknown>, Transformed, A>,
  ): FileHelper<A>
}

export const FileHelper: FileHelperStatic = {
  raw<A>(
    path: ToPath,
    toFile: (dataIn: A) => string,
    fromFile: (rawData: string) => unknown,
    validate: (data: unknown) => A,
  ): FileHelper<A> {
    return new FileHelperImpl<A>(
      toPath(path),
      inData => toFile(filterUndefined(inData)),
      fromFile,
      validate,
    )
  },

  string<A extends Transformed, Transformed = string>(
    path: ToPath,
    shape?: Validator<Transformed, A>,
    transformers?: Transformers<string, Transformed, A>,
  ): FileHelper<A> {
    return rawTransformed<A, string, Transformed>(
      path,
      inData => inData,
      inString => inString,
      data =>
        (shape || (z.string() as unknown as Validator<Transformed, A>)).parse(
          data,
        ),
      transformers,
    )
  },

  json<A extends Transformed, Transformed = unknown>(
    path: ToPath,
    shape: Validator<unknown, A>,
    transformers?: Transformers<unknown, Transformed, A>,
  ): FileHelper<A> {
    return rawTransformed<A, unknown, Transformed>(
      path,
      inData => JSON.stringify(inData, null, 2),
      inString => JSON.parse(inString),
      deepLooseParse(shape),
      transformers,
    )
  },

  yaml<A extends Transformed, Transformed = Record<string, unknown>>(
    path: ToPath,
    shape: Validator<Transformed, A>,
    options?: YAML.ParseOptions &
      YAML.DocumentOptions &
      YAML.SchemaOptions &
      YAML.ToJSOptions &
      YAML.CreateNodeOptions &
      YAML.ToStringOptions,
    transformers?: Transformers<Record<string, unknown>, Transformed, A>,
  ): FileHelper<A> {
    return rawTransformed<A, Record<string, unknown>, Transformed>(
      path,
      inData => YAML.stringify(inData, null, { indent: 2, ...options }),
      inString => YAML.parse(inString, options),
      deepLooseParse(shape),
      transformers,
    )
  },

  toml<A extends Transformed, Transformed = Record<string, unknown>>(
    path: ToPath,
    shape: Validator<Transformed, A>,
    transformers?: Transformers<Record<string, unknown>, Transformed, A>,
  ): FileHelper<A> {
    return rawTransformed<A, Record<string, unknown>, Transformed>(
      path,
      inData => TOML.stringify(inData as TOML.JsonMap),
      inString => TOML.parse(inString),
      deepLooseParse(shape),
      transformers,
    )
  },

  ini<A extends Transformed, Transformed = Record<string, unknown>>(
    path: ToPath,
    shape: Validator<Transformed, A>,
    options?: INI.EncodeOptions & INI.DecodeOptions,
    transformers?: Transformers<Record<string, unknown>, Transformed, A>,
  ): FileHelper<A> {
    return rawTransformed<A, Record<string, unknown>, Transformed>(
      path,
      inData => INI.stringify(inData, options),
      inString => INI.parse(inString, options),
      deepLooseParse(shape),
      transformers,
    )
  },

  env<A extends Transformed, Transformed = Record<string, string>>(
    path: ToPath,
    shape: Validator<Transformed, A>,
    transformers?: Transformers<Record<string, string>, Transformed, A>,
  ): FileHelper<A> {
    return rawTransformed<A, Record<string, string>, Transformed>(
      path,
      inData =>
        Object.entries(inData)
          .map(([k, v]) => `${k}=${v}`)
          .join('\n'),
      inString =>
        Object.fromEntries(
          inString
            .split('\n')
            .map(line => line.trim())
            .filter(line => !line.startsWith('#') && line.includes('='))
            .map(line => {
              const pos = line.indexOf('=')
              return [line.slice(0, pos), line.slice(pos + 1)]
            }),
        ),
      deepLooseParse(shape),
      transformers,
    )
  },

  xml<A extends Transformed, Transformed = Record<string, unknown>>(
    path: ToPath,
    shape: Validator<Transformed, A>,
    options?: { parser?: X2jOptions; builder?: XmlBuilderOptions },
    transformers?: Transformers<Record<string, unknown>, Transformed, A>,
  ): FileHelper<A> {
    const parser = new XMLParser(options?.parser)
    const builder = new XMLBuilder(options?.builder)
    return rawTransformed<A, Record<string, unknown>, Transformed>(
      path,
      inData => builder.build(inData),
      inString => parser.parse(inString),
      deepLooseParse(shape),
      transformers,
    )
  },
}

export default FileHelper
