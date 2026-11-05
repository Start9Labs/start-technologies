import type { IST } from '@start9labs/start-core'

export type ActionUpload = {
  id: string
  file: File
}

export function prepareActionFiles(
  spec: IST.InputSpec,
  input: Record<string, unknown>,
): { input: Record<string, unknown>; uploads: ActionUpload[] } {
  const uploads: ActionUpload[] = []

  function field(spec: IST.ValueSpec, value: unknown): unknown {
    if (spec.type === 'file') {
      if (spec.uploadId === null) return value
      if (value === null || value === undefined) return null
      uploads.push({ id: spec.uploadId, file: value as File })
      return spec.uploadId
    }
    if (spec.type === 'object') {
      return object(spec.spec, value as Record<string, unknown>)
    }
    if (spec.type === 'union') {
      const union = value as {
        selection: string
        value: Record<string, unknown>
      }
      const variant = spec.variants[union.selection]
      return variant
        ? { ...union, value: object(variant.spec, union.value) }
        : value
    }
    if (spec.type === 'list' && spec.spec.type === 'object') {
      const itemSpec = spec.spec.spec
      return (value as Record<string, unknown>[]).map(item =>
        object(itemSpec, item),
      )
    }
    return value
  }

  function object(spec: IST.InputSpec, value: Record<string, unknown>) {
    return Object.fromEntries(
      Object.entries(value).map(([key, value]) => [
        key,
        spec[key] ? field(spec[key], value) : value,
      ]),
    )
  }

  return { input: object(spec, input), uploads }
}

export async function uploadActionFiles(
  uploads: ActionUpload[],
  upload: (
    id: string,
    file: File,
    signal: AbortSignal,
    progress: (loaded: number) => void,
  ) => Promise<void>,
  signal: AbortSignal,
  progress: (id: string, loaded: number, complete: boolean) => void,
): Promise<void> {
  const abort = new AbortController()
  const combined = AbortSignal.any([signal, abort.signal])
  await Promise.allSettled(
    uploads.map(async ({ id, file }) => {
      try {
        combined.throwIfAborted()
        await upload(id, file, combined, loaded => progress(id, loaded, false))
        progress(id, file.size, true)
      } catch (error) {
        abort.abort(error)
        throw error
      }
    }),
  )
  if (abort.signal.aborted) throw abort.signal.reason
}
