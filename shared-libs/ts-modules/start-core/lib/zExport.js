'use strict'
Object.defineProperty(exports, '__esModule', { value: true })

const zod_1 = require('zod')
const zod_deep_partial_1 = require('zod-deep-partial')

// Recursively make all ZodObjects in a schema loose (preserve extra keys at every nesting level).
// Uses _zod.def.type duck-typing instead of instanceof to avoid issues with mismatched zod versions.
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

// Add deepPartial and deepLoose to z at runtime
zod_1.z.deepPartial = a => deepLoose((0, zod_deep_partial_1.zodDeepPartial)(a))
zod_1.z.deepLoose = deepLoose

// Override z.object to produce loose objects by default (extra keys are preserved, not stripped).
const _origObject = zod_1.z.object
const _patchedObject = (...args) => _origObject(...args).loose()

// z.object is a non-configurable getter on the zod namespace, so we can't override it directly.
// Shadow it by exporting a new object with _z as prototype and our patched object on the instance.
const z = Object.create(zod_1.z, {
  object: {
    value: _patchedObject,
    writable: true,
    configurable: true,
    enumerable: true,
  },
})

exports.z = z
