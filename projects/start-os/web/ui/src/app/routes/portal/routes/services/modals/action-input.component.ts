import { Component, DestroyRef, inject, signal } from '@angular/core'
import {
  DialogService,
  getErrorMessage,
  i18nKey,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { IST, T, utils } from '@start9labs/start-core'
import {
  TuiButton,
  TuiDialogContext,
  TuiLoader,
  TuiNotification,
} from '@taiga-ui/core'
import { TuiProgress } from '@taiga-ui/kit'
import { injectContext } from '@taiga-ui/polymorpheus'
import { compare, Operation } from 'fast-json-patch'
import { PatchDB } from 'patch-db-client'
import { endWith, firstValueFrom } from 'rxjs'
import {
  ActionButton,
  FormComponent,
} from 'src/app/routes/portal/components/form.component'
import { InvalidService } from 'src/app/routes/portal/components/form/containers/control.directive'
import { PkgIconDirective } from 'src/app/routes/portal/components/pkg-icon'
import {
  prepareActionFiles,
  uploadActionFiles,
} from 'src/app/services/action-files'
import { ActionService } from 'src/app/services/action.service'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { PrimaryStatus } from 'src/app/services/pkg-status-rendering.service'
import {
  getAllPackages,
  getLiveTasks,
  getManifest,
} from 'src/app/utils/get-package-data'

export type PackageActionData = {
  pkgInfo: {
    id: string
    title: string
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
      <img [pkgIcon]="pkgInfo.id" />
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
        [attr.inert]="submitting() ? '' : null"
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
    @for (upload of uploads(); track upload.id) {
      <p>
        <label>
          {{ upload.name }}
          <progress
            tuiProgressBar
            [max]="upload.size || 1"
            [value]="upload.complete ? upload.size || 1 : upload.loaded"
          ></progress>
        </label>
      </p>
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
  imports: [
    TuiNotification,
    TuiLoader,
    TuiButton,
    TuiProgress,
    FormComponent,
    i18nPipe,
    PkgIconDirective,
  ],
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
  private readonly tasks = inject(TaskService)
  private readonly abort = new AbortController()

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

  protected readonly submitting = signal(false)
  protected readonly uploads = signal<
    {
      id: string
      name: string
      size: number
      loaded: number
      complete: boolean
    }[]
  >([])
  readonly error = signal('')
  readonly res = signal<{
    spec: IST.InputSpec
    originalValue: Record<string, unknown>
    operations: Operation[] | null
  } | null>(null)

  constructor() {
    inject(DestroyRef).onDestroy(() => this.abort.abort())
    void this.load()
  }

  private async load(value?: Record<string, unknown>) {
    this.res.set(null)
    this.error.set('')
    try {
      const res = await this.api.rpc.package.action.getInput({
        packageId: this.pkgInfo.id,
        actionId: this.actionId,
        prefill: this.prefill ?? null,
      })
      if (!res)
        throw new Error(this.i18n.transform('Invalid response from server'))
      const originalValue = res.value || {}
      this.eventId = res.eventId
      this.res.set({
        spec: res.spec as IST.InputSpec,
        originalValue: value ?? originalValue,
        operations:
          !value && this.prefill
            ? compare(
                JSON.parse(JSON.stringify(originalValue)),
                utils.deepMerge(
                  JSON.parse(JSON.stringify(originalValue)),
                  this.prefill,
                ) as object,
              )
            : null,
      })
    } catch (e) {
      this.error.set(String(getErrorMessage(e)))
    }
  }

  async execute(input: Record<string, unknown>) {
    if (this.submitting()) return
    this.submitting.set(true)
    try {
      const { input: prepared, uploads } = prepareActionFiles(
        this.res()!.spec,
        input,
      )
      if (!(await this.checkConflicts(prepared))) return
      const uploaded = await this.tasks.runInline(async () => {
        if (new Set(uploads.map(upload => upload.id)).size !== uploads.length) {
          throw new Error(
            this.i18n.transform(
              'Each file must have its own upload destination.',
            ),
          )
        }
        this.uploads.set(
          uploads.map(({ id, file }) => ({
            id,
            name: file.name,
            size: file.size,
            loaded: 0,
            complete: false,
          })),
        )
        try {
          await uploadActionFiles(
            uploads,
            (id, file, signal, progress) =>
              this.api.uploadFile(id, file, {
                filename: file.name,
                signal,
                progress,
              }),
            this.abort.signal,
            (id, loaded, complete) =>
              this.uploads.update(uploads =>
                uploads.map(upload =>
                  upload.id === id ? { ...upload, loaded, complete } : upload,
                ),
              ),
          )
        } catch (error) {
          if (!this.abort.signal.aborted) throw error
        }
      })
      if (this.abort.signal.aborted) return
      if (
        uploaded &&
        (await this.actionService.execute(
          this.pkgInfo.id,
          this.eventId,
          this.actionId,
          prepared,
        ))
      ) {
        this.context.$implicit.complete()
      } else {
        await this.load(input)
        this.uploads.set([])
      }
    } finally {
      this.submitting.set(false)
    }
  }

  private async checkConflicts(input: object): Promise<boolean> {
    const packages = await getAllPackages(this.patch)

    const breakages = Object.keys(packages)
      .filter(
        id =>
          id !== this.pkgInfo.id &&
          getLiveTasks(packages[id]!).some(
            ([_, { task, active }]) =>
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
