import { Pipe } from '@angular/core'
import { T } from '@start9labs/start-core'

@Pipe({ name: 'registryTitle' })
export class RegistryTitlePipe {
  transform(url: string, identity: T.RegistryIdentity | null) {
    return identity?.name || new URL(url).hostname
  }
}
