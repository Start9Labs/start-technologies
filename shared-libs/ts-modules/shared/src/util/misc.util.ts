export function isObject(val: any): boolean {
  return val && typeof val === 'object' && !Array.isArray(val)
}

export function isEmptyObject(obj: object): boolean {
  return obj === undefined || !Object.keys(obj).length
}

export function pauseFor(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms))
}

export function debounce(delay: number = 300): MethodDecorator {
  return function (
    target: any,
    propertyKey: string | symbol,
    descriptor: PropertyDescriptor,
  ) {
    const timeoutKey = Symbol()

    const original = descriptor.value

    descriptor.value = function (this: any, ...args: any[]) {
      clearTimeout(this[timeoutKey])
      this[timeoutKey] = setTimeout(() => original.apply(this, args), delay)
    }

    return descriptor
  }
}

export function sameUrl(
  u1: string | null | undefined,
  u2: string | null | undefined,
): boolean {
  return toUrl(u1) === toUrl(u2)
}

export function isValidHttpUrl(url: string): boolean {
  try {
    const _ = new URL(url)
    return true
  } catch (_) {
    return false
  }
}

export function toUrl(text: string | null | undefined): string {
  try {
    const url = new URL(text as string)
    return url.toString()
  } catch {
    return ''
  }
}

export function registryUrl(input: string): string {
  const trimmed = input.trim()
  const hasScheme = /^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed)
  const url = new URL(hasScheme ? trimmed : `https://${trimmed}`)
  if (
    !['http:', 'https:'].includes(url.protocol) ||
    url.username ||
    url.password ||
    url.search ||
    url.hash
  ) {
    throw new Error('Expected a registry HTTP or HTTPS URL')
  }
  if (!hasScheme && url.hostname.endsWith('.onion')) url.protocol = 'http:'
  url.pathname = url.pathname.replace(/\/+$/, '') + '/'
  return url.toString()
}

export type WithId<T> = T & { id: string }

export type OptionalProperty<T, K extends keyof T> = Omit<T, K> &
  Partial<Pick<T, K>>
