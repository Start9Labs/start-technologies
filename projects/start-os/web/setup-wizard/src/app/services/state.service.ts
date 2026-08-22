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
      password: string // plaintext, will be encrypted before sending
    }

@Injectable({
  providedIn: 'root',
})
export class StateService {
  private readonly api = inject(ApiService)
  private readonly router = inject(Router)

  // Determined at app init
  kiosk = false

  // Set during install flow, or loaded from status response
  language = ''
  keyboard = ''

  // From install response or status response (incomplete)
  dataDriveGuid = ''
  attach = false
  mokEnrolled = false

  // Set when the device is pre-installed: the OS drive is fixed to the disk
  // the running OS booted from, and the wizard only asks for a data drive.
  osDrive = ''

  // Set during setup flow
  setupType?: SetupType
  recoverySource?: RecoverySource

  /**
   * Navigate to the appropriate step after language/keyboard selection.
   * Keyboard selection is only needed in kiosk mode.
   */
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

  /**
   * Called for attach flow (existing data drive)
   */
  async attachDrive(password: string | null): Promise<void> {
    await this.api.attach({
      guid: this.dataDriveGuid,
      password: password ? await this.api.encrypt(password) : null,
      kiosk: this.kiosk,
    })
  }

  // A password is required for a fresh install and optional for restore and transfer.
  async executeSetup(password: string | null, hostname: string): Promise<void> {
    let recoverySource: T.RecoverySource<T.EncryptedWire> | null = null

    if (this.recoverySource) {
      if (this.recoverySource.type === 'migrate') {
        recoverySource = this.recoverySource
      } else {
        // backup type - need to encrypt the backup password
        recoverySource = {
          type: 'backup',
          target: this.recoverySource.target,
          serverId: this.recoverySource.serverId,
          password: await this.api.encrypt(this.recoverySource.password),
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

  /**
   * Reset state for a fresh start
   */
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
