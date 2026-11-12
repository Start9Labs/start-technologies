import {
  Component,
  computed,
  inject,
  input,
  linkedSignal,
  signal,
} from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  i18nKey,
  i18nPipe,
  LocalizePipe,
  TaskService,
} from '@start9labs/shared'
import { T, utils } from '@start9labs/start-core'
import {
  TuiButton,
  TuiCell,
  TuiDataList,
  TuiDropdown,
  TuiTitle,
  tuiButtonOptionsProvider,
} from '@taiga-ui/core'
import { TuiBadge, TuiButtonLoading } from '@taiga-ui/kit'
import {
  TuiCardLarge,
  TuiHeader,
  tuiCardOptionsProvider,
} from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'

import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import {
  PrimaryRendering,
  renderPkgStatus,
} from 'src/app/services/pkg-status-rendering.service'
import { getManifest } from 'src/app/utils/get-package-data'

function stageKey(stage: T.HttpProxyStage): string {
  return JSON.stringify([stage.packageId, stage.profileId])
}

@Component({
  selector: 'section[httpProxyChain]',
  template: `
    <header tuiHeader="h6">
      <hgroup tuiTitle>
        <h3>{{ 'HTTP proxy chain' | i18n }}</h3>
        <span tuiSubtitle>
          {{ 'Applies to every address and interface on this binding.' | i18n }}
        </span>
      </hgroup>
    </header>
    <p class="g-secondary">
      {{
        'Proxy services can read and modify traffic, including credentials.'
          | i18n
      }}
    </p>
    <p class="g-secondary">
      {{
        'Unavailable proxies block traffic. Applying changes disconnects existing clients.'
          | i18n
      }}
    </p>
    @for (stage of draft(); track key(stage); let index = $index) {
      @let profile = profiles().get(key(stage));
      <div tuiCell="s">
        <span tuiTitle>
          <b>{{ profile?.packageName ?? stage.packageId }}</b>
          <span tuiSubtitle>
            {{ profile?.name ?? stage.profileId | localize }}
          </span>
        </span>
        <span tuiBadge [appearance]="profile ? 'neutral' : 'negative'">
          {{ profile?.status ?? 'Profile unavailable' | i18n }}
        </span>
        <button
          tuiIconButton
          type="button"
          iconStart="@tui.arrow-up"
          [disabled]="saving() || index === 0"
          (click)="move(index, index - 1)"
        >
          {{ 'Move up' | i18n }}
        </button>
        <button
          tuiIconButton
          type="button"
          iconStart="@tui.arrow-down"
          [disabled]="saving() || index === draft().length - 1"
          (click)="move(index, index + 1)"
        >
          {{ 'Move down' | i18n }}
        </button>
        <button
          tuiIconButton
          type="button"
          iconStart="@tui.trash"
          [disabled]="saving()"
          (click)="remove(index)"
        >
          {{ 'Remove' | i18n }}
        </button>
      </div>
    } @empty {
      <p>{{ 'No proxy stages' | i18n }}</p>
    }
    <footer class="g-buttons">
      <button
        tuiButton
        tuiDropdown
        tuiDropdownAuto
        type="button"
        appearance="secondary"
        iconStart="@tui.plus"
        [disabled]="saving() || !available().length"
      >
        {{ 'Add proxy' | i18n }}
        <tui-data-list *tuiDropdown="let close" (click)="close()">
          @for (profile of available(); track key(profile)) {
            <button tuiOption (click)="add(profile)">
              {{ profile.packageName }} — {{ profile.name | localize }}
            </button>
          }
        </tui-data-list>
      </button>
      @if (dirty()) {
        <button
          tuiButton
          type="button"
          appearance="flat-grayscale"
          [disabled]="saving()"
          (click)="draft.set(saved())"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button
          tuiButton
          type="button"
          appearance="primary"
          [loading]="saving()"
          (click)="save()"
        >
          {{ 'Save' | i18n }}
        </button>
      }
    </footer>
    @if (!profiles().size) {
      <small class="g-secondary">
        {{ 'No proxy profiles available' | i18n }}
      </small>
    }
  `,
  hostDirectives: [TuiCardLarge],
  providers: [
    tuiCardOptionsProvider({
      space: 'compact',
      appearance: 'outline-grayscale',
    }),
    tuiButtonOptionsProvider({ size: 'xs', appearance: 'flat-grayscale' }),
  ],
  imports: [
    i18nPipe,
    LocalizePipe,
    TuiButton,
    TuiCell,
    TuiTitle,
    TuiHeader,
    TuiBadge,
    TuiButtonLoading,
    TuiDataList,
    TuiDropdown,
  ],
})
export class HttpProxyChain {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly packages = toSignal(
    inject<PatchDB<DataModel>>(PatchDB).watch$('packageData'),
  )

  protected readonly saved = computed(() => this.binding().proxyChain, {
    equal: utils.deepEqual,
  })
  protected readonly draft = linkedSignal(() => this.saved())
  protected readonly saving = signal(false)
  protected readonly dirty = computed(
    () => !utils.deepEqual(this.draft(), this.saved()),
  )
  protected readonly key = stageKey
  protected readonly profiles = computed(() => {
    const profiles = new Map<
      string,
      T.HttpProxyStage & {
        name: T.LocaleString
        packageName: string
        status: i18nKey
      }
    >()
    for (const [packageId, pkg] of Object.entries(this.packages() ?? {})) {
      const manifest = getManifest(pkg)
      if (!manifest.plugins.includes('http-proxy-v0')) continue
      for (const profile of pkg.plugin.httpProxy?.profiles ?? []) {
        const choice = {
          packageId,
          profileId: profile.id,
          name: profile.name,
          packageName: manifest.title,
          status: PrimaryRendering[renderPkgStatus(pkg).primary].display,
        }
        profiles.set(stageKey(choice), choice)
      }
    }
    return profiles
  })
  protected readonly available = computed(() => {
    const selected = new Set(this.draft().map(stageKey))
    return [...this.profiles().values()].filter(
      profile => !selected.has(stageKey(profile)),
    )
  })

  public readonly address = input.required<T.AddressInfo>({
    alias: 'httpProxyChain',
  })
  public readonly packageId = input.required<string>()
  public readonly binding = input.required<T.BindInfo>()

  protected add(stage: T.HttpProxyStage): void {
    this.draft.update(chain => [
      ...chain,
      { packageId: stage.packageId, profileId: stage.profileId },
    ])
  }

  protected remove(index: number): void {
    this.draft.update(chain => chain.filter((_, i) => i !== index))
  }

  protected move(from: number, to: number): void {
    this.draft.update(chain => {
      const next = [...chain]
      const [stage] = next.splice(from, 1)
      next.splice(to, 0, stage!)
      return next
    })
  }

  protected async save(): Promise<void> {
    if (this.saving() || !this.dirty()) return
    const address = this.address()
    this.saving.set(true)
    try {
      await this.tasks.runInline(() =>
        this.api.rpc.package.host.binding.setHttpProxyChain({
          package: this.packageId(),
          host: address.hostId ?? '',
          internalPort: address.internalPort,
          chain: this.draft(),
        }),
      )
    } finally {
      this.saving.set(false)
    }
  }
}
