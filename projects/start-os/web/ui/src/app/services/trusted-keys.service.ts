import { inject, Service, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { MarketplaceTrust, Signer, TrustContext } from '@start9labs/marketplace'
import {
  DialogService,
  i18nKey,
  sameUrl,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import { combineLatest, defaultIfEmpty, firstValueFrom, map } from 'rxjs'
import { TRUSTED_KEY_DETAILS } from 'src/app/components/trusted-keys/details'
import { parsePemPublicKey } from 'src/app/utils/public-keys'
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
    { initialValue: {} as Record<string, RegistryOffer[]> },
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

    if (name.trim()) {
      await this.save(() =>
        this.tasks.run(() => this.update(id, { ...key, name }), 'Saving'),
      )
    }
  }

  async trust(
    id: string,
    key: T.TrustedKey,
    context: TrustContext,
    trusted: boolean,
  ): Promise<void> {
    await this.save(() =>
      this.tasks.runInline(() =>
        this.update(id, { ...key, [context]: trusted }),
      ),
    )
  }

  async add(id: string, name: string, context: TrustContext): Promise<void> {
    await this.save(() =>
      this.tasks.runInline(() =>
        this.api.rpc.server.trustedKeys.add({
          name,
          publicKeys: [parsePemPublicKey(id)],
          trustFor: context,
        }),
      ),
    )
  }

  viewKey(id: string): void {
    this.dialogs.openComponent(TRUSTED_KEY_DETAILS, { data: id }).subscribe()
  }

  viewSigner({ id }: Signer): void {
    this.viewKey(id)
  }

  async remove(id: string): Promise<void> {
    if (
      await this.confirm(
        'Remove key?',
        'Signatures from this key will no longer be trusted.',
        'Remove',
      )
    ) {
      await this.save(() =>
        this.tasks.run(
          () => this.api.rpc.server.trustedKeys.remove({ id }),
          'Saving',
        ),
      )
    }
  }

  private update(
    id: string,
    { name, marketplace, os }: T.TrustedKey,
  ): Promise<unknown> {
    return this.api.rpc.server.trustedKeys.update({ id, name, marketplace, os })
  }

  private async save(task: () => Promise<unknown>): Promise<void> {
    this.saving.set(true)
    try {
      await task()
    } finally {
      this.saving.set(false)
    }
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
