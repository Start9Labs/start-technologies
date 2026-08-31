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
import { Watchable } from '@start9labs/start-core/util/Watchable'
import { PathBase } from './Volume'

const previousPath = /(.+?)\/([^/]*)$/

const exists = (path: string) =>
  fs.access(path).then(
    () => true,
    () => false,
  )

async function onCreated(path: string) {
  if (path === '/') return
  if (!path.startsWith('/')) path = `${process.cwd()}/${path}`
  if (await exists(path)) {
    return
  }
  const split = path.split('/')
  const filename = split.pop()
  const parent = split.join('/')
  await onCreated(parent)
  const ctrl = new AbortController()
  const watch = fs.watch(parent, { persistent: false, signal: ctrl.signal })
  if (await exists(path)) {
    ctrl.abort()
    return
  }
  if (
    await fs.access(path).then(
      () => true,
      () => false,
    )
  ) {
    ctrl.abort()
    return
  }
  for await (let event of watch) {
    if (event.filename === filename) {
      ctrl.abort('finished')
      return
    }
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
 * Bidirectional transformers for converting between the raw file format and
 * the application-level data type. Used with FileHelper factory methods.
 *
 * @typeParam Raw - The native type the file format parses to (e.g. `Record<string, unknown>` for JSON)
 * @typeParam Transformed - The application-level type after transformation
 */
export type Transformers<
  Raw = unknown,
  Transformed = unknown,
  Validated extends Transformed = Transformed,
> = {
  /** Transform raw parsed data into the application type */
  onRead: (value: Raw) => Transformed
  /** Transform application data back into the raw format for writing */
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
  ) => Promise<A | null>
}

/**
 * @description Use this class to read/write an underlying configuration file belonging to the upstream service.
 *
 *   These type definitions should reflect the underlying file as closely as possible. For example, if the service does not require a particular value, it should be marked as optional(), even if your package requires it.
 *
 *   It is recommended to use onMismatch() whenever possible. This provides an escape hatch in case the user edits the file manually and accidentally sets a value to an unsupported type.
 *
 *   Officially supported file types are json, yaml, and toml. Other files types can use "raw"
 *
 *   Choose between officially supported file formats (), or a custom format (raw).
 *
 * @example
 * Below are a few examples
 *
 * ```
 * import { matches, FileHelper } from '@start9labs/start-sdk'
 * const { arrayOf, boolean, literal, literals, object, natural, string } = matches
 *
 * export const jsonFile = FileHelper.json('./inputSpec.json', object({
 *   passwords: arrayOf(string).onMismatch([])
 *   type: literals('private', 'public').optional().onMismatch(undefined)
 * }))
 *
 * export const tomlFile = FileHelper.toml('./inputSpec.toml', object({
 *   url: literal('https://start9.com').onMismatch('https://start9.com')
 *   public: boolean.onMismatch(true)
 * }))
 *
 * export const yamlFile = FileHelper.yaml('./inputSpec.yml', object({
 *   name: string.optional().onMismatch(undefined)
 *   age: natural.optional().onMismatch(undefined)
 * }))
 *
 * export const bitcoinConfFile = FileHelper.raw(
 *   './service.conf',
 *   (obj: CustomType) => customConvertObjToFormattedString(obj),
 *   (str) => customParseStringToTypedObj(str),
 * )
 * ```
 */
export interface FileHelper<A> {
  readonly path: string
  readonly writeData: (dataIn: A) => string
  readonly readData: (stringValue: string) => unknown
  readonly validate: (value: unknown) => A
  read(): ReadType<A>
  read<B>(
    map: (value: A) => B,
    eq?: (left: B | null, right: B | null) => boolean,
  ): ReadType<B>
  write(
    effects: T.Effects,
    data: T.AllowReadonly<A> | A,
    options?: { allowWriteAfterConst?: boolean },
  ): Promise<null>
  merge(
    effects: T.Effects,
    data: T.AllowReadonly<T.DeepPartial<A>>,
    options?: { allowWriteAfterConst?: boolean },
  ): Promise<null>
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

  private async writeFileRaw(data: string): Promise<null> {
    const parent = previousPath.exec(this.path)
    if (parent) {
      await fs.mkdir(parent[1], { recursive: true })
    }

    await fs.writeFile(this.path, data)

    return null
  }

  /** Renders to the file format. A key whose value is `undefined` is dropped, so
   * merging `undefined` removes it rather than serializing the word. */
  private serialize(data: A): string {
    return this.writeData(filterUndefined(data))
  }

  /**
   * Accepts structured data and overwrites the existing file on disk.
   */
  private async writeFile(data: A): Promise<null> {
    return await this.writeFileRaw(this.serialize(data))
  }

  private async readFileRaw(): Promise<string | null> {
    if (!(await exists(this.path))) {
      return null
    }
    return await fs.readFile(this.path).then(data => data.toString('utf-8'))
  }

  private async readFile(): Promise<unknown> {
    const raw = await this.readFileRaw()
    if (raw === null) {
      return raw
    }
    return this.readData(raw)
  }

  /**
   * Reads the file from disk and converts it to structured data.
   */
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

    return new (class extends Watchable<A | null, B | null> {
      protected readonly label = 'FileHelper'

      protected async fetch() {
        return doRead()
      }

      protected async *produce(
        abort: AbortSignal,
      ): AsyncGenerator<A | null, void> {
        while (this.effects.isInContext && !abort.aborted) {
          if (await exists(filePath)) {
            const ctrl = new AbortController()
            const onAbort = () => ctrl.abort()
            abort.addEventListener('abort', onAbort, { once: true })
            try {
              const watch = fs.watch(filePath, {
                persistent: false,
                signal: ctrl.signal,
              })
              yield await doRead()
              await Promise.resolve()
                .then(async () => {
                  for await (const _ of watch) {
                    ctrl.abort()
                    return null
                  }
                })
                .catch(e => console.error(asError(e)))
            } finally {
              abort.removeEventListener('abort', onAbort)
            }
          } else {
            yield null
            await onCreated(filePath).catch(e => console.error(asError(e)))
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

  /**
   * Create a reactive reader for this file.
   *
   * Returns an object with multiple read strategies:
   * - `once()` - Read the file once and return the parsed value
   * - `const(effects)` - Read once but re-read when the file changes (for use with constRetry)
   * - `watch(effects)` - Async generator yielding new values on each file change
   * - `onChange(effects, callback)` - Fire a callback on each file change
   * - `waitFor(effects, predicate)` - Block until the file value satisfies a predicate
   *
   * @param map - Optional transform function applied after validation
   * @param eq - Optional equality function to deduplicate watch emissions
   */
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
      waitFor: (effects: T.Effects, pred: (value: A | null) => boolean) =>
        this.createFileWatchable(effects, map, eq).waitFor(pred),
    }
  }

  /**
   * Accepts full structured data and overwrites the existing file on disk if it exists.
   */
  async write(
    effects: T.Effects,
    data: T.AllowReadonly<A> | A,
    options: { allowWriteAfterConst?: boolean } = {},
  ) {
    const newData = this.validate(data)
    await this.writeFile(newData)
    if (!options.allowWriteAfterConst && effects.constRetry) {
      const records = this.consts.filter(([c]) => c === effects.constRetry)
      for (const record of records) {
        const [_, prev, map, eq] = record
        if (!eq(prev, map(newData))) {
          throw new Error(`Canceled: write after const: ${this.path}`)
        }
      }
    }
    return null
  }

  /**
   * Accepts partial structured data and performs a merge with the existing file on disk.
   */
  async merge(
    effects: T.Effects,
    data: T.AllowReadonly<T.DeepPartial<A>>,
    options: { allowWriteAfterConst?: boolean } = {},
  ) {
    const fileDataRaw = await this.readFileRaw()
    let fileData: any = fileDataRaw === null ? null : this.readData(fileDataRaw)
    try {
      fileData = this.validate(fileData)
    } catch (_) {}
    const mergeData = this.validate(fileMerge({}, fileData, data))
    const toWrite = this.serialize(mergeData)
    if (toWrite !== fileDataRaw) {
      await this.writeFile(mergeData)
      if (!options.allowWriteAfterConst && effects.constRetry) {
        const records = this.consts.filter(([c]) => c === effects.constRetry)
        for (const record of records) {
          const [_, prev, map, eq] = record
          if (!eq(prev, map(mergeData))) {
            throw new Error(`Canceled: write after const: ${this.path}`)
          }
        }
      }
    }
    return null
  }

  /**
   * We wanted to be able to have a fileHelper, and just modify the path later in time.
   * Like one behavior of another dependency or something similar.
   */
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
  return FileHelper.raw<A>(
    path,
    inData => {
      if (transformers) {
        return toFile(transformers.onWrite(inData))
      }
      return toFile(inData as any as Raw)
    },
    fileData => {
      if (transformers) {
        return transformers.onRead(fromFile(fileData))
      }
      return fromFile(fileData)
    },
    validate as (a: unknown) => A,
  )
}

// Deep-loosen a file-model shape so unknown keys present in the on-disk file
// survive validation (and the merge round-trip) instead of being stripped.
// Computed once per FileHelper construction.
function deepLooseParse<A>(shape: z.ZodType<A>): (data: unknown) => A {
  const loose = z.deepLoose(shape)
  return data => loose.parse(data)
}

interface FileHelperStatic {
  /** Create a File Helper for an arbitrary file type. */
  raw<A>(
    path: ToPath,
    toFile: (dataIn: A) => string,
    fromFile: (rawData: string) => unknown,
    validate: (data: unknown) => A,
  ): FileHelper<A>

  /** Create a File Helper for a text file */
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

  /** Create a File Helper for a .json file. */
  json<A>(path: ToPath, shape: Validator<unknown, A>): FileHelper<A>
  json<A extends Transformed, Transformed = unknown>(
    path: ToPath,
    shape: Validator<unknown, A>,
    transformers: Transformers<unknown, Transformed, A>,
  ): FileHelper<A>

  /** Create a File Helper for a .yaml file */
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

  /** Create a File Helper for a .toml file */
  toml<A extends Record<string, unknown>>(
    path: ToPath,
    shape: Validator<Record<string, unknown>, A>,
  ): FileHelper<A>
  toml<A extends Transformed, Transformed = Record<string, unknown>>(
    path: ToPath,
    shape: Validator<Transformed, A>,
    transformers: Transformers<Record<string, unknown>, Transformed, A>,
  ): FileHelper<A>

  /** Create a File Helper for a .ini file. */
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

  /** Create a File Helper for a .env file (KEY=VALUE format, one per line). */
  env<A extends Record<string, string>>(
    path: ToPath,
    shape: Validator<Record<string, string>, A>,
  ): FileHelper<A>
  env<A extends Transformed, Transformed = Record<string, string>>(
    path: ToPath,
    shape: Validator<Transformed, A>,
    transformers: Transformers<Record<string, string>, Transformed, A>,
  ): FileHelper<A>

  /** Create a File Helper for an .xml file. */
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
    return new FileHelperImpl<A>(toPath(path), toFile, fromFile, validate)
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
