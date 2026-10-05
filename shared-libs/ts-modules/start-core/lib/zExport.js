'use strict'
Object.defineProperty(exports, '__esModule', { value: true })

const zod_1 = require('zod')
const zod_deep_partial_1 = require('zod-deep-partial')

// Duck-types on _zod.def.type rather than instanceof, which fails across zod instances.
function deepLoose(schema) {
  const def = schema._zod?.def
  if (!def) return schema
  switch (def.type) {
    case 'optional':
      return deepLoose(def.innerType).optional()
    case 'nullable':
      return deepLoose(def.innerType).nullable()
    case 'object': {
      const newShape = {}
      for (const key in schema.shape) {
        newShape[key] = deepLoose(schema.shape[key])
      }
      return zod_1.z.looseObject(newShape)
    }
    case 'array':
      return zod_1.z.array(deepLoose(def.element))
    case 'union':
      return zod_1.z.union(def.options.map(o => deepLoose(o)))
    case 'intersection':
      return zod_1.z.intersection(deepLoose(def.left), deepLoose(def.right))
    case 'record':
      return zod_1.z.record(def.keyType, deepLoose(def.valueType))
    case 'tuple':
      return zod_1.z.tuple(def.items.map(i => deepLoose(i)))
    case 'lazy':
      return zod_1.z.lazy(() => deepLoose(def.getter()))
    default:
      return schema
  }
}

zod_1.z.deepPartial = a => deepLoose((0, zod_deep_partial_1.zodDeepPartial)(a))
zod_1.z.deepLoose = deepLoose

exports.z = zod_1.z
