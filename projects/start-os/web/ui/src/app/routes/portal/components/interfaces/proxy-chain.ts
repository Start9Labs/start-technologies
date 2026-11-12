import {
  Component,
  computed,
  inject,
  input,
  linkedSignal,
  signal,
} from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import {
  DialogService,
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
  TuiDialogContext,
  TuiDropdown,
  TuiTitle,
  tuiButtonOptionsProvider,
  tuiIconStart,
} from '@taiga-ui/core'
import { TuiBadge, TuiButtonLoading, TuiSwitch } from '@taiga-ui/kit'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { of } from 'rxjs'

import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import {
  PrimaryRendering,
  renderPkgStatus,
} from 'src/app/services/pkg-status-rendering.service'
import { getManifest } from 'src/app/utils/get-package-data'

interface ChainContext {
  packageId: string
  address: T.AddressInfo
  gateway: T.HttpProxyGateway
  gatewayName: string
}

function stageKey(stage: T.HttpProxyStage): string {
  return JSON.stringify([stage.packageId, stage.profileId])
}

@Component({
  selector: 'button[httpProxyChain]',
  template: `
    {{ 'HTTP proxy chain' | i18n }}
  `,
  host: { tuiIconButton: '', type: 'button', '(click)': 'open()' },
  hostDirectives: [TuiButton],
  providers: [
    tuiButtonOptionsProvider({ size: 'xs', appearance: 'secondary' }),
  ],
  imports: [i18nPipe],
})
export class HttpProxyChainButton {
  private readonly dialogs = inject(DialogService)
  private readonly icon = tuiIconStart(computed(() => '@tui.settings'))

  public readonly gateway = input.required<T.HttpProxyGateway>({
    alias: 'httpProxyChain',
  })
  public readonly gatewayName = input.required<string>()
  public readonly address = input.required<T.AddressInfo>()
  public readonly packageId = input('')

  protected open(): void {
    this.dialogs
      .openComponent(HTTP_PROXY_CHAIN, {
        size: 'l',
        data: {
          packageId: this.packageId(),
          address: this.address(),
          gateway: this.gateway(),
          gatewayName: this.gatewayName(),
        } satisfies ChainContext,
      })
      .subscribe()
  }
}

@Component({
  template: `
    <form tuiForm="m" (submit.prevent)="save()">
      <header tuiHeader="h6">
        <hgroup tuiTitle>
          <h2 [id]="context.id">{{ 'HTTP proxy chain' | i18n }}</h2>
          <span tuiSubtitle>{{ context.data.gatewayName }}</span>
        </hgroup>
      </header>
      <p class="g-secondary">
        {{
          'Applies to every address and interface on this gateway and binding.'
            | i18n
        }}
      </p>
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
      @if (context.data.gateway.kind === 'network') {
        <label tuiCell>
          <span tuiTitle>
            <b>{{ 'Public only' | i18n }}</b>
            <span tuiSubtitle>
              {{ 'Private clients bypass this chain.' | i18n }}
            </span>
          </span>
          <input
            tuiSwitch
            type="checkbox"
            name="publicOnly"
            [ngModel]="draft().publicOnly"
            [disabled]="saving()"
            (ngModelChange)="setPublicOnly($event)"
          />
        </label>
      }
      @for (stage of draft().chain; track key(stage); let index = $index) {
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
            [disabled]="saving() || index === draft().chain.length - 1"
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
      @if (!profiles().size) {
        <small class="g-secondary">
          {{ 'No proxy profiles available' | i18n }}
        </small>
      }
      <footer>
        <button
          tuiButton
          type="button"
          appearance="flat-grayscale"
          [disabled]="saving()"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button
          tuiButton
          appearance="primary"
          [disabled]="!dirty() || !binding()?.options?.proxy"
          [loading]="saving()"
        >
          {{ 'Save' | i18n }}
        </button>
      </footer>
    </form>
  `,
  providers: [
    tuiButtonOptionsProvider({ size: 'xs', appearance: 'flat-grayscale' }),
  ],
  imports: [
    FormsModule,
    i18nPipe,
    LocalizePipe,
    TuiButton,
    TuiCell,
    TuiTitle,
    TuiHeader,
    TuiForm,
    TuiBadge,
    TuiButtonLoading,
    TuiDataList,
    TuiDropdown,
    TuiSwitch,
  ],
})
class HttpProxyChainDialog {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  protected readonly context =
    injectContext<TuiDialogContext<void, ChainContext>>()
  private readonly packages = toSignal(this.patch.watch$('packageData'))
  private readonly isServer =
    !this.context.data.packageId || this.context.data.packageId === 'start-os'
  private readonly osHost = toSignal(
    this.isServer
      ? this.patch.watch$('serverInfo', 'network', 'host')
      : of(undefined),
  )
  protected readonly binding = computed(() => {
    const { packageId, address } = this.context.data
    const host = this.isServer
      ? this.osHost()
      : this.packages()?.[packageId]?.hosts[address.hostId ?? '']
    return host?.bindings[address.internalPort]
  })
  protected readonly saved = computed<T.HttpProxyChain>(
    () =>
      this.binding()?.proxyChains.find(policy =>
        utils.deepEqual(policy.gateway, this.context.data.gateway),
      ) ?? {
        gateway: this.context.data.gateway,
        chain: [],
        publicOnly: false,
      },
    { equal: utils.deepEqual },
  )
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
    const selected = new Set(this.draft().chain.map(stageKey))
    return [...this.profiles().values()].filter(
      profile => !selected.has(stageKey(profile)),
    )
  })

  protected setPublicOnly(publicOnly: boolean): void {
    this.draft.update(policy => ({ ...policy, publicOnly }))
  }

  protected add(stage: T.HttpProxyStage): void {
    this.draft.update(policy => ({
      ...policy,
      chain: [
        ...policy.chain,
        { packageId: stage.packageId, profileId: stage.profileId },
      ],
    }))
  }

  protected remove(index: number): void {
    this.draft.update(policy => ({
      ...policy,
      chain: policy.chain.filter((_, i) => i !== index),
    }))
  }

  protected move(from: number, to: number): void {
    this.draft.update(policy => {
      const chain = [...policy.chain]
      const [stage] = chain.splice(from, 1)
      chain.splice(to, 0, stage!)
      return { ...policy, chain }
    })
  }

  protected async save(): Promise<void> {
    if (this.saving() || !this.dirty() || !this.binding()?.options.proxy) return
    const { packageId, address } = this.context.data
    const params = { internalPort: address.internalPort, policy: this.draft() }
    this.saving.set(true)
    try {
      const saved = await this.tasks.runInline(() =>
        this.isServer
          ? this.api.rpc.server.host.binding.setHttpProxyChain(params)
          : this.api.rpc.package.host.binding.setHttpProxyChain({
              package: packageId,
              host: address.hostId ?? '',
              ...params,
            }),
      )
      if (saved) this.context.completeWith(undefined)
    } finally {
      this.saving.set(false)
    }
  }
}

export const HTTP_PROXY_CHAIN = new PolymorpheusComponent(HttpProxyChainDialog)
