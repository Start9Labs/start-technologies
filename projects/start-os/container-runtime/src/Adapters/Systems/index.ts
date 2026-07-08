import * as fs from 'node:fs/promises'
import { System } from '../../Interfaces/System'
import { EMBASSY_JS_LOCATION, SystemForEmbassy } from './SystemForEmbassy'
import { STARTOS_JS_LOCATION, SystemForStartOs } from './SystemForStartOs'
export async function getSystem(): Promise<System> {
  if (
    await fs.access(STARTOS_JS_LOCATION).then(
      () => true,
      () => false,
    )
  ) {
    return SystemForStartOs.of()
  } else if (
    await fs.access(EMBASSY_JS_LOCATION).then(
      () => true,
      () => false,
    )
  ) {
    return SystemForEmbassy.of()
  }
  throw new Error(`${STARTOS_JS_LOCATION} not found`)
}
