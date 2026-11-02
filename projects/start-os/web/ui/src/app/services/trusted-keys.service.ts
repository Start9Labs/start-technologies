import { inject, Service, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { MarketplaceTrust, TrustContext } from '@start9labs/marketplace'
import {
  DialogService,
  i18nKey,
  sameUrl,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import { combineLatest, defaultIfEmpty, firstValueFrom, map } from 'rxjs'
import { ApiService } from './api/embassy-api.service'
import { DataModel } from './patch-db/data-model'

export interface RegistryOffer {
  url: string
  name: string
}

@Service()
export class TrustedKeysService extends MarketplaceTrust {
  private readonly api = inject(ApiService)
  private readonly dialogs = inject(DialogService)
  private readonly tasks = inject(TaskService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  readonly keys = toSignal(this.patch.watch$('serverInfo', 'trustedKeys'))
  readonly saving = signal(false)

  /** The connected registries offering each key, by key ID. */
  readonly offers = toSignal(
    combineLatest([
      this.patch.watch$('serverInfo', 'registryKeys'),
      this.patch.watch$('ui', 'registries'),
    ]).pipe(
      map(([snapshots, registries]) => {
        const offers: Record<string, RegistryOffer[]> = {}

        for (const [url, name] of Object.entries(registries)) {
          for (const [listed, ids] of Object.entries(snapshots)) {
            if (!sameUrl(url, listed)) continue
            for (const id of ids)
              (offers[id] ??= []).push({ url, name: name || url })
          }
        }

        return offers
      }),
    ),
    { initialValue: {} },
  )

  async rename(id: string, key: T.TrustedKey): Promise<void> {
    const name = await firstValueFrom(
      this.dialogs
        .openPrompt<string>({
          label: 'Edit label',
          data: {
            message: 'A name for this key',
            label: 'Label',
            initialValue: key.name,
            required: true,
            buttonText: 'Save',
          },
        })
        .pipe(defaultIfEmpty('')),
    )

    if (name.trim()) await this.update(id, { ...key, name })
  }

  async trust(
    id: string,
    key: T.TrustedKey,
    context: TrustContext,
    trusted: boolean,
  ): Promise<void> {
    if (
      context === 'os' &&
      trusted &&
      !(await this.confirm(
        'Trust for StartOS updates?',
        'StartOS installs any update this key signs, replacing the operating system on this server.',
        'Trust',
      ))
    ) {
      return
    }

    await this.update(id, { ...key, [context]: trusted })
  }

  async remove(id: string): Promise<void> {
    if (
      await this.confirm(
        'Remove key?',
        'Signatures from this key will no longer be trusted.',
        'Remove',
      )
    ) {
      await this.save(() => this.api.rpc.server.trustedKeys.remove({ id }))
    }
  }

  private async update(
    id: string,
    { name, marketplace, os }: T.TrustedKey,
  ): Promise<void> {
    await this.save(() =>
      this.api.rpc.server.trustedKeys.update({ id, name, marketplace, os }),
    )
  }

  private async save(task: () => Promise<unknown>): Promise<void> {
    this.saving.set(true)
    await this.tasks.run(task, 'Saving')
    this.saving.set(false)
  }

  private confirm(
    label: i18nKey,
    content: i18nKey,
    yes: i18nKey,
  ): Promise<boolean> {
    return firstValueFrom(
      this.dialogs
        .openConfirm({ label, data: { content, yes, no: 'Cancel' } })
        .pipe(defaultIfEmpty(false)),
    )
  }
}
