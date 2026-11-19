import { Component, computed, effect, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { i18nPipe, TaskService } from '@start9labs/shared'
import { TuiButton, TuiCell, TuiDialogContext, TuiTitle } from '@taiga-ui/core'
import { TuiAvatar, TuiConfirmService } from '@taiga-ui/kit'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'

import { FormDialogService } from 'src/app/services/form-dialog.service'
import { PkgIconDirective } from 'src/app/routes/portal/components/pkg-icon'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { getCurrentManifest } from 'src/app/utils/get-package-data'
import { GrantChoice, grantFor, GrantLabel, permission } from './grant'
import { SourceFields, sourceEdit } from './source-fields'
import { SOURCE_MANAGE_STYLES } from './manage.styles'

@Component({
  template: `
    @if (volume(); as volume) {
      <div tuiForm="l">
        <source-fields [edit]="edit" />
        <h3 tuiTitle="m">{{ 'Service access' | i18n }}</h3>
        @if (packages().length) {
          <div class="access">
            @for (pkg of packages(); track pkg.id) {
              <div tuiCell>
                <span tuiAvatar aria-hidden="true">
                  <img alt="" [pkgIcon]="pkg.id" />
                </span>
                <span tuiTitle>{{ pkg.title }}</span>
                <grant-choice
                  [label]="pkg.title"
                  [value]="access(pkg.id)"
                  (selected)="choose(pkg.id, $event)"
                />
              </div>
            }
          </div>
        } @else {
          <p>{{ 'No installed services can use shared volumes' | i18n }}</p>
        }
        <footer>
          <button
            tuiButton
            type="button"
            appearance="flat-destructive"
            iconStart="@tui.trash-2"
            class="remove"
            (click)="context.completeWith(true)"
          >
            {{ (volume.source.kind === 'local' ? 'Delete' : 'Remove') | i18n }}
          </button>
          <button
            tuiButton
            type="button"
            appearance="flat-grayscale"
            (click)="formDialog.close(context.$implicit)"
          >
            {{ 'Cancel' | i18n }}
          </button>
          <button
            tuiButton
            type="button"
            [disabled]="!changed() || edit.invalid()"
            (click)="save()"
          >
            {{ 'Save' | i18n }}
          </button>
        </footer>
      </div>
    }
  `,
  styles: [
    SOURCE_MANAGE_STYLES,
    `
      h3 {
        margin-block: 0.5rem 0;
      }

      .access [tuiCell] {
        position: relative;
        padding-inline: 0;

        [tuiAvatar] {
          --t-size: 2rem;
          inline-size: 2rem;
          block-size: 2rem;
        }

        &:not(:last-child)::after {
          content: '';
          position: absolute;
          inset: auto 0 0;
          height: 1px;
          background: var(--tui-background-neutral-1);
        }
      }
    `,
  ],
  imports: [
    GrantChoice,
    SourceFields,
    TuiButton,
    TuiCell,
    TuiAvatar,
    PkgIconDirective,
    TuiForm,
    TuiTitle,
    i18nPipe,
  ],
})
export class VolumeManage {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  protected readonly formDialog = inject(FormDialogService)
  private readonly confirm = inject(TuiConfirmService, { optional: true })

  protected readonly context =
    injectContext<TuiDialogContext<boolean, { id: string }>>()

  private readonly volumes = toSignal(this.patch.watch$('sharedVolumes'))
  private readonly allPackages = toSignal(this.patch.watch$('packageData'))
  private readonly choices = signal<Record<string, GrantLabel>>({})

  protected readonly volume = computed(
    () => this.volumes()?.[this.context.data.id],
  )
  protected readonly edit = sourceEdit(this.volume)
  protected readonly packages = computed(() =>
    Object.values(this.allPackages() || {})
      .map(getCurrentManifest)
      .filter(manifest => manifest.sharedVolumes),
  )
  protected readonly changed = computed(
    () =>
      this.edit.changed() ||
      Object.keys(this.choices()).some(id => this.isChanged(id)),
  )

  constructor() {
    effect(() =>
      this.changed()
        ? this.confirm?.markAsDirty()
        : this.confirm?.markAsPristine(),
    )
  }

  protected access(id: string): GrantLabel {
    return this.choices()[id] ?? permission(this.volume()?.grants[id])
  }

  protected choose(id: string, label: GrantLabel) {
    this.choices.update(choices => ({ ...choices, [id]: label }))
  }

  protected async save() {
    const id = this.context.data.id
    const grants = Object.keys(this.choices()).filter(pkg =>
      this.isChanged(pkg),
    )
    const saved = await this.tasks.run(async () => {
      await this.edit.save(id, this.api.rpc.volumes)
      for (const packageId of grants) {
        await this.api.rpc.volumes.setGrant({
          id,
          packageId,
          grant: grantFor(this.choices()[packageId]!),
        })
      }
    }, 'Saving')
    if (!saved) return
    this.confirm?.markAsPristine()
    this.context.$implicit.complete()
  }

  private isChanged(id: string) {
    return this.choices()[id] !== permission(this.volume()?.grants[id])
  }
}
