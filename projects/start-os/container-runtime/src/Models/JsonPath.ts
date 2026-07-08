import { z } from '@start9labs/start-sdk'

type NestedPath<A extends string, B extends string> = `/${A}/${string}/${B}`
type NestedPaths = NestedPath<'actions', 'run' | 'getInput'>
// prettier-ignore
type UnNestPaths<A> =
  A extends `${infer A}/${infer B}` ? [...UnNestPaths<A>, ... UnNestPaths<B>] :
  [A]

export function unNestPath<A extends string>(a: A): UnNestPaths<A> {
  return a.split('/') as UnNestPaths<A>
}
function isNestedPath(path: string): path is NestedPaths {
  const paths = path.split('/')
  if (paths.length !== 4) return false
  if (paths[1] === 'actions' && (paths[3] === 'run' || paths[3] === 'getInput'))
    return true
  return false
}
export const jsonPath = z.union([
  z.enum([
    '/packageInit',
    '/packageUninit',
    '/backup/create',
    '/backup/restore',
  ]),
  z.string().refine(isNestedPath),
])

export type JsonPath = z.infer<typeof jsonPath>
