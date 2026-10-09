import { inject, Injectable } from '@angular/core'
import { Router } from '@angular/router'
import { T } from '@start9labs/start-core'
import { ApiService } from './api.service'

export type SetupType = 'fresh' | 'restore' | 'attach' | 'transfer'

export type RecoverySource =
  | {
      type: 'migrate'
      guid: string
    }
  | {
      type: 'backup'
      target:
        | { type: 'disk'; logicalname: string }
        | {
            type: 'cifs'
            hostname: string
            path: string
            username: string
            password: string | null
          }
      serverId: string
      password: string // Plaintext until `executeSetup` encrypts it.
      scheduled: boolean
    }

@Injectable({
  providedIn: 'root',
})
export class StateService {
  private readonly api = inject(ApiService)
  private readonly router = inject(Router)

  // Initialized from the browser hostname during app startup.
  kiosk = false

  language = ''
  keyboard = ''

  // Populated by OS installation or a resumed incomplete setup.
  dataDriveGuid = ''
  attach = false
  mokEnrolled = false

  // A pre-installed system fixes this to its boot disk.
  osDrive = ''

  setupType?: SetupType
  recoverySource?: RecoverySource

  // Kiosk callers must collect the keyboard before calling this.
  async navigateAfterLocale(): Promise<void> {
    if (this.dataDriveGuid) {
      if (this.attach) {
        this.setupType = 'attach'
        await this.router.navigate(['/password'])
      } else {
        await this.router.navigate(['/home'])
      }
    } else {
      await this.router.navigate(['/drives'])
    }
  }

  async attachDrive(password: string | null): Promise<void> {
    await this.api.attach({
      guid: this.dataDriveGuid,
      password: password ? await this.api.encrypt(password) : null,
      kiosk: this.kiosk,
    })
  }

  // Fresh setup requires a password; restore and transfer allow null.
  async executeSetup(password: string | null, hostname: string): Promise<void> {
    let recoverySource: T.RecoverySource<T.EncryptedWire> | null = null

    if (this.recoverySource) {
      if (this.recoverySource.type === 'migrate') {
        recoverySource = this.recoverySource
      } else {
        recoverySource = {
          type: 'backup',
          target: this.recoverySource.target,
          serverId: this.recoverySource.serverId,
          password: await this.api.encrypt(this.recoverySource.password),
          scheduled: this.recoverySource.scheduled,
        }
      }
    }

    await this.api.execute({
      guid: this.dataDriveGuid,
      password: password ? await this.api.encrypt(password) : null,
      hostname,
      recoverySource,
      kiosk: this.kiosk,
    })
  }

  reset(): void {
    this.language = ''
    this.keyboard = ''
    this.dataDriveGuid = ''
    this.attach = false
    this.mokEnrolled = false
    this.osDrive = ''
    this.setupType = undefined
    this.recoverySource = undefined
  }
}
