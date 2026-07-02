import { Component, inject, signal } from '@angular/core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiButton, TuiTitle } from '@taiga-ui/core'
import { TUI_CONFIRM, TuiBlock } from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { filter } from 'rxjs'
import { ActionService } from 'src/app/services/action.service'
import { ApiService } from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <section class="g-form" tuiCardLarge>
      <header tuiHeader="body-l">
        <h3 tuiTitle>{{ 'Create Backup' | i18n }}</h3>
      </header>
      <p tuiDescription>
        {{
          'Download a backup of all router settings, including security profiles, WiFi configuration, SSL certificates, and system preferences.'
            | i18n
        }}
      </p>
      <footer>
        <button
          tuiButton
          appearance="outline"
          iconStart="@tui.download"
          (click)="download()"
        >
          {{ 'Download Backup' | i18n }}
        </button>
      </footer>
    </section>
    <section class="g-form" tuiCardLarge>
      <header tuiHeader="body-l">
        <h3 tuiTitle>{{ 'Restore Backup' | i18n }}</h3>
      </header>
      <p tuiDescription>
        {{
          'Upload a previously created backup to restore all settings. The device will reboot after restoring.'
            | i18n
        }}
      </p>
      <footer>
        <label tuiBlock="m" appearance="outline" iconStart="@tui.file">
          <b>
            {{ selectedFile() ? selectedFile()!.name : ('Choose File' | i18n) }}
          </b>
          <input
            tuiBlock
            appearance=""
            type="file"
            accept=".tar.gz,.gz"
            (change)="onFileSelected($event)"
          />
        </label>
        <button
          tuiButton
          iconStart="@tui.hard-drive-upload"
          [style.margin-inline-start.rem]="1"
          [disabled]="!selectedFile() || uploading()"
          (click)="restore()"
        >
          {{ uploading() ? ('Restoring...' | i18n) : ('Restore' | i18n) }}
        </button>
      </footer>
    </section>
  `,
  styles: `
    [tuiBlock] {
      vertical-align: bottom;
    }
  `,
  host: { class: 'g-page' },
  imports: [TuiCardLarge, TuiButton, TuiHeader, TuiTitle, TuiBlock, i18nPipe],
})
export default class Backup {
  private readonly api = inject(ApiService)
  private readonly actions = inject(ActionService)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly i18n = inject(i18nPipe)

  protected readonly uploading = signal(false)
  protected readonly selectedFile = signal<File | null>(null)

  protected onFileSelected(event: Event) {
    const input = event.target as HTMLInputElement
    this.selectedFile.set(input.files?.[0] ?? null)
  }

  protected async download() {
    await this.actions.run(
      async () => {
        const { guid, filename } = await this.api.backupCreate()
        const res = await fetch(`/rest/rpc/${guid}`)
        if (!res.ok)
          throw new Error(
            `${this.i18n.transform('Download failed:')} ${res.statusText}`,
          )
        const blob = await res.blob()
        const url = URL.createObjectURL(blob)
        const a = document.createElement('a')
        a.href = url
        a.download = filename
        a.click()
        URL.revokeObjectURL(url)
      },
      {
        loading: this.i18n.transform('Creating backup...'),
        success: this.i18n.transform('Backup downloaded'),
      },
    )
  }

  protected restore() {
    this.dialogs
      .open(TUI_CONFIRM, {
        label: this.i18n.transform('Restore Backup'),
        data: {
          content: this.i18n.transform(
            'All current settings will be overwritten and the device will reboot. Are you sure you want to continue?',
          ),
          yes: this.i18n.transform('Restore & Reboot'),
          no: this.i18n.transform('Cancel'),
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() => this.doRestore())
  }

  private async doRestore() {
    const file = this.selectedFile()
    if (!file) return

    this.uploading.set(true)
    const success = await this.actions.run(
      async () => {
        const { upload } = await this.api.backupRestore()
        const arrayBuffer = await file.arrayBuffer()
        const res = await fetch(`/rest/rpc/${upload}`, {
          method: 'POST',
          body: arrayBuffer,
        })
        if (!res.ok) {
          const text = await res.text()
          throw new Error(text || res.statusText)
        }
        // Device reboots after restoring; the global ConnectionService
        // indicator detects the drop and handles the reconnect.
      },
      {
        loading: this.i18n.transform('Restoring backup...'),
        success: this.i18n.transform('Backup restored'),
        restart: true,
      },
    )
    this.uploading.set(false)
    if (success) this.selectedFile.set(null)
  }
}
