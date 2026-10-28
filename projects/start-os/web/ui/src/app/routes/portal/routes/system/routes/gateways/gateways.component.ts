import { Component, inject } from '@angular/core'
import { RouterLink } from '@angular/router'
import { DocsLinkDirective, i18nPipe, TaskService } from '@start9labs/shared'
import { ISB } from '@start9labs/start-core'
import { TuiButton } from '@taiga-ui/core'

import { FormComponent } from 'src/app/routes/portal/components/form.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormDialogService } from 'src/app/services/form-dialog.service'
import { GatewayService } from 'src/app/services/gateway.service'
import { TitleDirective } from 'src/app/services/title.service'
import { configBuilderToSpec } from 'src/app/utils/configBuilderToSpec'

import { GatewaysTableComponent } from './table.component'

@Component({
  template: `
    <ng-container *title>
      <div>
        <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </a>
        {{ 'Gateways' | i18n }}
        <a
          tuiIconButton
          size="xs"
          docsLink
          path="/start-os/gateways.html"
          appearance="icon"
          iconStart="@tui.book-open-text"
        ></a>
      </div>
    </ng-container>

    <section class="g-card">
      <header>
        {{ 'Gateways' | i18n }}
        <a
          tuiIconButton
          size="xs"
          docsLink
          path="/start-os/gateways.html"
          appearance="icon"
          iconStart="@tui.book-open-text"
        >
          {{ 'Documentation' | i18n }}
        </a>
        <button
          tuiButton
          size="xs"
          [style.margin]="'0 0.5rem 0 auto'"
          iconStart="@tui.plus"
          (click)="add()"
        >
          {{ 'Add' | i18n }}
        </button>
      </header>
      <gateways-table />
    </section>
  `,
  styles: `
    :host-context(tui-root._mobile) .g-card > header > [docsLink] {
      display: none;
    }
  `,
  providers: [GatewayService],
  imports: [
    RouterLink,
    TuiButton,
    GatewaysTableComponent,
    TitleDirective,
    i18nPipe,
    DocsLinkDirective,
  ],
})
export default class GatewaysComponent {
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly formDialog = inject(FormDialogService)
  private readonly i18n = inject(i18nPipe)

  async add() {
    const spec = ISB.InputSpec.of({
      name: ISB.Value.text({
        name: this.i18n.transform('Name'),
        description: this.i18n.transform(
          'A name to easily identify the gateway',
        ),
        required: true,
        default: null,
        placeholder: 'StartTunnel 1',
      }),
      config: ISB.Value.union({
        name: this.i18n.transform('WireGuard Config File'),
        default: 'paste',
        variants: ISB.Variants.of({
          paste: {
            name: this.i18n.transform('Copy/Paste'),
            spec: ISB.InputSpec.of({
              file: ISB.Value.textarea({
                name: this.i18n.transform('File Contents'),
                default: null,
                required: true,
                minRows: 8,
                maxRows: 8,
              }),
            }),
          },
          select: {
            name: this.i18n.transform('Upload'),
            spec: ISB.InputSpec.of({
              file: ISB.Value.file({
                name: this.i18n.transform('File'),
                required: true,
                extensions: ['.conf'],
              }),
            }),
          },
        }),
      }),
    })

    this.formDialog.open(FormComponent, {
      label: 'Add Wireguard Gateway',
      data: {
        spec: await configBuilderToSpec(spec),
        buttons: [
          {
            text: this.i18n.transform('Save'),
            handler: async (input: typeof spec._TYPE) =>
              this.tasks.run(
                async () =>
                  await this.api.rpc.net.tunnel.add({
                    name: input.name,
                    config:
                      input.config.selection === 'paste'
                        ? input.config.value.file
                        : await (input.config.value.file as any as File).text(),
                    type: null, // let StartOS auto-detect from the config
                    setAsDefaultOutbound: false,
                  }),
                'Saving',
              ),
          },
        ],
      },
    })
  }
}
