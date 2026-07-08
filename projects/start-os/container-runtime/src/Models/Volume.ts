import * as fs from 'node:fs/promises'

export const BACKUP = 'backup'
export class Volume {
  readonly path: string
  constructor(
    readonly volumeId: string,
    _path = '',
  ) {
    if (volumeId.toLowerCase() === BACKUP) {
      this.path = `/media/startos/backup${!_path ? '' : `/${_path}`}`
    } else {
      this.path = `/media/startos/volumes/${volumeId}${!_path ? '' : `/${_path}`}`
    }
  }
  async exists() {
    return fs.stat(this.path).then(
      () => true,
      () => false,
    )
  }
}
