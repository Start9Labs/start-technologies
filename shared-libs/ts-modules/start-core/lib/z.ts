import { z as zod } from 'zod'

export * from 'zod'

const loose = (schema: any): any => {
  const def = schema._zod?.def
  if (!def) return schema
  switch (def.type) {
    case 'optional':
      return loose(def.innerType).optional()
    case 'nullable':
      return loose(def.innerType).nullable()
    case 'object':
      return zod.looseObject(
        Object.fromEntries(
          Object.entries(schema.shape).map(([key, value]) => [
            key,
            loose(value),
          ]),
        ),
      )
    case 'array':
      return zod.array(loose(def.element))
    case 'union':
      return zod.union(def.options.map(loose))
    case 'intersection':
      return zod.intersection(loose(def.left), loose(def.right))
    case 'record':
      return zod.record(def.keyType, loose(def.valueType))
    case 'tuple':
      return zod.tuple(def.items.map(loose))
    case 'lazy':
      return zod.lazy(() => loose(def.getter()))
    default:
      return schema
  }
}

/** Every object at every level keeps unknown keys. */
export function deepLoose<T>(schema: zod.ZodType<T>): zod.ZodType<T> {
  return loose(schema)
}
