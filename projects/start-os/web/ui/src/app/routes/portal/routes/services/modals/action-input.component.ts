import { Component, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  DialogService,
  getErrorMessage,
  i18nKey,
  i18nPipe,
} from '@start9labs/shared'
import { T, utils } from '@start9labs/start-core'
import {
  TuiButton,
  TuiDialogContext,
  TuiLoader,
  TuiNotification,
} from '@taiga-ui/core'
import { injectContext } from '@taiga-ui/polymorpheus'
import { compare } from 'fast-json-patch'
import { PatchDB } from 'patch-db-client'
import { catchError, EMPTY, endWith, firstValueFrom, from, map } from 'rxjs'
import {
  ActionButton,
  FormComponent,
} from 'src/app/routes/portal/components/form.component'
import { InvalidService } from 'src/app/routes/portal/components/form/containers/control.directive'
import { ActionService } from 'src/app/services/action.service'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { PrimaryStatus } from 'src/app/services/pkg-status-rendering.service'
import { getAllPackages, getManifest } from 'src/app/utils/get-package-data'

export type PackageActionData = {
  pkgInfo: {
    id: string
    title: string
    icon: string
    status: PrimaryStatus
  }
  actionInfo: {
    id: string
    metadata: T.ActionMetadata
  }
  prefill?: Record<string, unknown>
}

@Component({
  template: `
    <div class="service-title">
      <img [src]="pkgInfo.icon" alt="" />
      <h4>{{ pkgInfo.title }}</h4>
    </div>
    @if (error()) {
      <div tuiNotification appearance="negative">
        <div [innerHTML]="error()"></div>
      </div>
    }
    @if (res(); as res) {
      @if (warning) {
        <div tuiNotification appearance="warning">
          <div [innerHTML]="warning"></div>
        </div>
      }

      <app-form
        [spec]="res.spec"
        [value]="res.originalValue || {}"
        [buttons]="buttons"
        [operations]="res.operations || []"
      >
        <button
          tuiButton
          appearance="flat-grayscale"
          type="reset"
          [style.margin-right]="'auto'"
        >
          {{ 'Reset' | i18n }}
        </button>
      </app-form>
    } @else if (!error()) {
      <tui-loader size="l" textContent="loading" />
    }
  `,
  styles: `
    .service-title {
      display: inline-flex;
      align-items: center;
      margin-bottom: 1.5rem;

      img {
        height: 1.25rem;
        margin-right: 0.25rem;
        border-radius: 100%;
      }

      h4 {
        margin: 0;
      }
    }
  `,
  imports: [TuiNotification, TuiLoader, TuiButton, FormComponent, i18nPipe],
  providers: [InvalidService],
})
export class ActionInputModal {
  private readonly dialog = inject(DialogService)
  private readonly api = inject(ApiService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly actionService = inject(ActionService)
  private readonly context =
    injectContext<TuiDialogContext<void, PackageActionData>>()
  private readonly i18n = inject(i18nPipe)

  readonly actionId = this.context.data.actionInfo.id
  readonly warning = this.context.data.actionInfo.metadata.warning
  readonly pkgInfo = this.context.data.pkgInfo
  readonly prefill = this.context.data.prefill
  eventId: string | null = null

  buttons: ActionButton<any>[] = [
    {
      text: 'Submit',
      handler: value => this.execute(value),
    },
  ]

  readonly error = signal('')
  readonly res = toSignal(
    from(
      this.api.getActionInput({
        packageId: this.pkgInfo.id,
        actionId: this.actionId,
        prefill: this.prefill ?? null,
      }),
    ).pipe(
      map(res => {
        console.warn('MAP', res)
        const originalValue = res.value || {}
        this.eventId = res.eventId

        const operations = this.prefill
          ? compare(
              JSON.parse(JSON.stringify(originalValue)),
              utils.deepMerge(
                JSON.parse(JSON.stringify(originalValue)),
                this.prefill,
              ) as object,
            )
          : null

        return {
          spec: res.spec,
          originalValue,
          operations,
        }
      }),
      catchError(e => {
        console.error('catchError', e)
        this.error.set(String(getErrorMessage(e)))
        return EMPTY
      }),
    ),
  )

  async execute(input: object) {
    if (await this.checkConflicts(input)) {
      await this.actionService.execute(
        this.pkgInfo.id,
        this.eventId,
        this.actionId,
        input,
      )
      this.context.$implicit.complete()
    }
  }

  private async checkConflicts(input: object): Promise<boolean> {
    const packages = await getAllPackages(this.patch)

    const breakages = Object.keys(packages)
      .filter(
        id =>
          id !== this.pkgInfo.id &&
          Object.values(packages[id]!.tasks).some(
            ({ task, active }) =>
              !active &&
              task.severity === 'critical' &&
              task.packageId === this.pkgInfo.id &&
              task.actionId === this.actionId &&
              task.when?.condition === 'input-not-matches' &&
              task.input &&
              task.input.accept.every(accept => conflicts(accept, input)),
          ),
      )
      .map(id => id)

    if (!breakages.length) return true

    const message = `${this.i18n.transform('As a result of this change, the following services will no longer work properly and may crash')}:<ul>`
    const content =
      `${message}${breakages.map(id => `<li><b>${getManifest(packages[id]!).title}</b></li>`).join('')}</ul>` as i18nKey

    return firstValueFrom(
      this.dialog
        .openConfirm({
          label: 'Warning',
          data: { content, yes: 'Continue', no: 'Cancel' },
        })
        .pipe(endWith(false)),
    )
  }
}

// Mirrors the Rust backend's `conflicts()` function in core/src/service/action.rs.
// A key in the partial that is missing from the full input is NOT a conflict.
function conflicts(left: unknown, right: unknown): boolean {
  if (
    typeof left === 'object' &&
    left !== null &&
    !Array.isArray(left) &&
    typeof right === 'object' &&
    right !== null &&
    !Array.isArray(right)
  ) {
    const l = left as Record<string, unknown>
    const r = right as Record<string, unknown>
    return Object.keys(l).some(k => (k in r ? conflicts(l[k], r[k]) : false))
  }

  if (Array.isArray(left) && Array.isArray(right)) {
    return left.some(v => right.every(vr => conflicts(v, vr)))
  }

  return left !== right
}
