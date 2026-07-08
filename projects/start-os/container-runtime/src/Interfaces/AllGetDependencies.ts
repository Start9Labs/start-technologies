import { GetDependency } from './GetDependency'
import { System } from './System'

export type AllGetDependencies = GetDependency<'system', Promise<System>>
