'use strict'
Object.defineProperty(exports, '__esModule', { value: true })

const zod_1 = require('zod')
const zod_deep_partial_1 = require('zod-deep-partial')

// Duck-types on _zod.def.type rather than instanceof, which fails across zod instances.
function deepLoose(schema) {
  const def = schema._zod?.def
  if (!def) return schema
  let result
  switch (def.type) {
    case 'optional':
      result = deepLoose(def.innerType).optional()
      break
    case 'nullable':
      result = deepLoose(def.innerType).nullable()
      break
    case 'object': {
      const newShape = {}
      for (const key in schema.shape) {
        newShape[key] = deepLoose(schema.shape[key])
      }
      result = zod_1.z.looseObject(newShape)
      break
    }
    case 'array':
      result = zod_1.z.array(deepLoose(def.element))
      break
    case 'union':
      result = zod_1.z.union(def.options.map(o => deepLoose(o)))
      break
    case 'intersection':
      result = zod_1.z.intersection(deepLoose(def.left), deepLoose(def.right))
      break
    case 'record':
      result = zod_1.z.record(def.keyType, deepLoose(def.valueType))
      break
    case 'tuple':
      result = zod_1.z.tuple(def.items.map(i => deepLoose(i)))
      break
    case 'lazy':
      result = zod_1.z.lazy(() => deepLoose(def.getter()))
      break
    default:
      return schema
  }
  return result
}

zod_1.z.deepPartial = a => deepLoose((0, zod_deep_partial_1.zodDeepPartial)(a))
zod_1.z.deepLoose = deepLoose

exports.z = zod_1.z
