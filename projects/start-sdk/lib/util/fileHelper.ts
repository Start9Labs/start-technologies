import * as TOML from '@iarna/toml'
import {
  XMLBuilder,
  XMLParser,
  type X2jOptions,
  type XmlBuilderOptions,
} from 'fast-xml-parser'
import * as INI from 'ini'
import { randomBytes } from 'node:crypto'
import * as fs from 'node:fs/promises'
import { posix, resolve as resolvePath } from 'node:path'
import * as YAML from 'yaml'
import { z } from '@start9labs/start-core/zExport'
import * as T from '@start9labs/start-core/types'
import { asError, deepEqual } from '@start9labs/start-core/util'
import { MappedWatchable } from '@start9labs/start-core/util/Watchable'
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

const writeQueues = new Map<string, Promise<void>>()

/**
 * Runs `op` after every write already queued for `path`. Every procedure of a
 * package runs in one runtime process, so an in-process queue serializes all
 * of its writers to a file.
 */
function queueWrite<T>(path: string, op: () => Promise<T>): Promise<T> {
  const key = resolvePath(path)
  const result = (writeQueues.get(key) ?? Promise.resolve()).then(op)
  const tail = result.then(
    () => {},
    () => {},
  )
  writeQueues.set(key, tail)
  tail.then(() => {
    if (writeQueues.get(key) === tail) writeQueues.delete(key)
  })
  return result
}

const unescapeMountinfo = (field: string) =>
  field.replace(/\\([0-7]{3})/g, (_, octal) =>
    String.fromCharCode(parseInt(octal, 8)),
  )

/**
 * Whether the file at `path`, on device `device` (`major:minor`), is the
 * source of a bind mount listed in `mountinfo` (the format of
 * `/proc/self/mountinfo`).
 */
export function isBindMountSource(
  mountinfo: string,
  path: string,
  device: string,
): boolean {
  const mounts = mountinfo
    .split('\n')
    .map(line => line.split(' '))
    .filter(fields => fields.length > 4 && fields[2] === device)
    .map(fields => ({
      root: unescapeMountinfo(fields[3]),
      mountPoint: unescapeMountinfo(fields[4]),
    }))
  const holding = mounts
    .filter(
      m =>
        m.mountPoint === '/' ||
        path === m.mountPoint ||
        path.startsWith(`${m.mountPoint}/`),
    )
    .reduce<
      (typeof mounts)[number] | null
    >((best, m) => (!best || m.mountPoint.length >= best.mountPoint.length ? m : best), null)
  if (!holding) return false
  const root = posix.join(
    holding.root,
    holding.mountPoint === '/' ? path : path.slice(holding.mountPoint.length),
  )
  return mounts.some(m => m !== holding && m.root === root)
}

function deviceId(dev: bigint): string {
  const major = ((dev >> 8n) & 0xfffn) | ((dev >> 32n) & ~0xfffn)
  const minor = (dev & 0xffn) | ((dev >> 12n) & ~0xffn)
  return `${major}:${minor}`
}

/**
 * Replaces the contents of `path` so that a reader sees either the old
 * contents or the new ones, never a truncated file: the data is written and
 * flushed to a temporary file beside it, which is then renamed over it.
 *
 * A bind mount pins the inode it was made from, so a file mounted into a
 * subcontainer (`type: 'file'`) is overwritten in place instead; renaming over
 * it would leave the mount showing the old contents.
 */
async function replaceFile(path: string, data: string): Promise<void> {
  const target = await fs.realpath(path).catch(() => null)
  if (target === null) {
    const link = await fs.lstat(path).catch(() => null)
    if (link?.isSymbolicLink()) return fs.writeFile(path, data)
  }
  const dest = target ?? path
  const stat = await fs.stat(dest, { bigint: true }).catch(() => null)
  if (stat) {
    const mountinfo = await fs
      .readFile('/proc/self/mountinfo', 'utf-8')
      .catch(() => '')
    if (isBindMountSource(mountinfo, dest, deviceId(stat.dev))) {
      return fs.writeFile(dest, data)
    }
  }

  const slash = dest.lastIndexOf('/')
  const tmp = `${dest.slice(0, slash + 1)}.${dest.slice(slash + 1)}.${randomBytes(6).toString('hex')}.tmp`
  try {
    await fs.writeFile(tmp, data, { flag: 'wx', flush: true })
    if (stat) {
      await fs.chmod(tmp, Number(stat.mode & 0o7777n))
      const created = await fs.stat(tmp)
      if (
        created.uid !== Number(stat.uid) ||
        created.gid !== Number(stat.gid)
      ) {
        await fs.chown(tmp, Number(stat.uid), Number(stat.gid))
      }
    }
    await fs.rename(tmp, dest)
  } catch (e) {
    await fs.rm(tmp, { force: true })
    const code = (e as NodeJS.ErrnoException).code
    if (['EACCES', 'EBUSY', 'EPERM', 'EXDEV'].includes(code ?? '')) {
      return fs.writeFile(dest, data)
    }
    throw e
  }
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
  update(
    effects: T.Effects,
    change: (
      current: A | null,
    ) => T.AllowReadonly<A> | A | null | Promise<T.AllowReadonly<A> | A | null>,
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

    await replaceFile(this.path, data)

    return null
  }

  /**
   * Accepts structured data and overwrites the existing file on disk.
   */
  private async writeFile(data: A): Promise<null> {
    return await this.writeFileRaw(this.writeData(data))
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

  private checkConsts(
    effects: T.Effects,
    written: A,
    options: { allowWriteAfterConst?: boolean },
  ) {
    if (!options.allowWriteAfterConst && effects.constRetry) {
      const records = this.consts.filter(([c]) => c === effects.constRetry)
      for (const record of records) {
        const [_, prev, map, eq] = record
        if (!eq(prev, map(written))) {
          throw new Error(`Canceled: write after const: ${this.path}`)
        }
      }
    }
  }

  /**
   * Reads the file, passes its raw contents to `change`, and writes the
   * result if it differs from what is on disk, all in this path's write queue.
   */
  private async modify(
    effects: T.Effects,
    change: (raw: string | null) => Promise<A | null>,
    options: { allowWriteAfterConst?: boolean },
  ) {
    const written = await queueWrite(this.path, async () => {
      const raw = await this.readFileRaw()
      const next = await change(raw)
      if (next === null) return null
      const toWrite = this.writeData(next)
      if (toWrite === raw) return null
      await this.writeFileRaw(toWrite)
      return { data: next }
    })
    if (written) this.checkConsts(effects, written.data, options)
    return null
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
    await queueWrite(this.path, () => this.writeFile(newData))
    this.checkConsts(effects, newData, options)
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

  /**
   * Replaces the file with what `change` returns for its current contents —
   * the value `read().once()` would return, or `null` if the file is missing.
   * Returning `null` leaves the file as it is. Writers to one path run one at
   * a time, so no other write to this file lands between the read and the
   * write. `change` must not write this file itself: that write would wait for
   * this one to finish.
   */
  async update(
    effects: T.Effects,
    change: (
      current: A | null,
    ) => T.AllowReadonly<A> | A | null | Promise<T.AllowReadonly<A> | A | null>,
    options: { allowWriteAfterConst?: boolean } = {},
  ) {
    return this.modify(
      effects,
      async raw => {
        const data = raw === null ? null : this.readData(raw)
        const next = await change(data ? this.validate(data) : null)
        return next === null ? null : this.validate(next)
      },
      options,
    )
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
