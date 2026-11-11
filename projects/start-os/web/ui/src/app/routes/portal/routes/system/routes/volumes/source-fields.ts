import {
  Component,
  computed,
  input,
  linkedSignal,
  signal,
  Signal,
  inject,
} from '@angular/core'
import { FormsModule } from '@angular/forms'
import { ConvertBytesPipe, CopyService, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiIcon, TuiInput } from '@taiga-ui/core'
import { TuiPassword, TuiTextarea, TuiTooltip } from '@taiga-ui/kit'

import { ApiClient } from 'src/app/services/api/api.types'
import { VOLUME_STATUSES, VOLUME_TYPES } from './types'

export type SourceEntry = Pick<
  T.SharedVolume,
  'name' | 'source' | 'availability' | 'available' | 'error'
>
export type SourceClient = Pick<ApiClient['volumes'], 'rename' | 'updateCifs'>

export function sourceEdit(entry: Signal<SourceEntry | undefined>) {
  const cifs = computed(() => {
    const source = entry()?.source
    return source?.kind === 'cifs' ? source : null
  })
  const storedName = computed(() => entry()?.name ?? '')
  const name = linkedSignal(storedName)
  const hostname = linkedSignal(() => cifs()?.hostname ?? '')
  const path = linkedSignal(() => cifs()?.path ?? '')
  const username = linkedSignal(() => cifs()?.username ?? '')
  const password = signal('')
  const renamed = computed(() => name().trim() !== entry()?.name)
  const cifsChanged = computed(
    () =>
      !!cifs() &&
      (hostname().trim() !== cifs()?.hostname ||
        path().trim() !== cifs()?.path ||
        username().trim() !== (cifs()?.username ?? '') ||
        !!password()),
  )

  return {
    entry,
    name,
    hostname,
    path,
    username,
    password,
    changed: computed(() => renamed() || cifsChanged()),
    invalid: computed(
      () =>
        !name().trim() || (!!cifs() && (!hostname().trim() || !path().trim())),
    ),
    async save(id: string, client: SourceClient) {
      if (renamed()) await client.rename({ id, name: name().trim() })
      if (cifsChanged())
        await client.updateCifs({
          id,
          hostname: hostname().trim(),
          path: path().trim(),
          username: username().trim(),
          password: password() || undefined,
        })
    },
  }
}

export type SourceEdit = ReturnType<typeof sourceEdit>

@Component({
  selector: 'source-fields',
  template: `
    @let state = edit();
    @if (state.entry(); as entry) {
      <tui-textfield>
        <label tuiLabel>{{ 'Name' | i18n }}</label>
        <input
          tuiInput
          [ngModel]="state.name()"
          (ngModelChange)="state.name.set($event)"
        />
      </tui-textfield>
      <tui-textfield [iconStart]="types[entry.source.kind].icon">
        <label tuiLabel>{{ 'Type' | i18n }}</label>
        <input
          tuiInput
          [readonly]="true"
          [ngModel]="types[entry.source.kind].label | i18n"
        />
      </tui-textfield>
      @switch (entry.source.kind) {
        @case ('disk') {
          @if (driveLabel(); as label) {
            <tui-textfield>
              <label tuiLabel>{{ 'Label' | i18n }}</label>
              <input tuiInput [readonly]="true" [ngModel]="label" />
            </tui-textfield>
          }
          <tui-textfield>
            <label tuiLabel>{{ 'Drive' | i18n }}</label>
            <input
              tuiInput
              [readonly]="true"
              [ngModel]="entry.source.logicalname"
            />
            <button
              tuiIconButton
              appearance="icon"
              size="s"
              type="button"
              tabindex="-1"
              iconStart="@tui.copy"
              (pointerdown.stop)="(0)"
              (click)="copy.copy(entry.source.logicalname)"
            >
              {{ 'Copy' | i18n }}
            </button>
          </tui-textfield>
          <tui-textfield>
            <label tuiLabel>{{ 'Path' | i18n }}</label>
            <input
              tuiInput
              [readonly]="true"
              [ngModel]="'/' + entry.source.path"
            />
          </tui-textfield>
        }
        @case ('cifs') {
          <tui-textfield>
            <label tuiLabel>{{ 'Hostname' | i18n }}</label>
            <input
              tuiInput
              autocapitalize="off"
              [ngModel]="state.hostname()"
              (ngModelChange)="state.hostname.set($event)"
            />
          </tui-textfield>
          <tui-textfield>
            <label tuiLabel>{{ 'Path' | i18n }}</label>
            <input
              tuiInput
              autocapitalize="off"
              [ngModel]="state.path()"
              (ngModelChange)="state.path.set($event)"
            />
          </tui-textfield>
          <tui-textfield>
            <label tuiLabel>{{ 'Username' | i18n }}</label>
            <input
              tuiInput
              autocomplete="off"
              autocapitalize="off"
              [ngModel]="state.username()"
              (ngModelChange)="state.username.set($event)"
            />
          </tui-textfield>
          <tui-textfield>
            <label tuiLabel>{{ 'Password' | i18n }}</label>
            <input
              tuiInput
              type="password"
              autocomplete="new-password"
              [ngModel]="state.password()"
              (ngModelChange)="state.password.set($event)"
            />
            <tui-icon tuiPassword />
            <tui-icon
              [tuiTooltip]="'Leave blank to keep the current value.' | i18n"
            />
          </tui-textfield>
        }
      }
      <tui-textfield>
        <label tuiLabel>{{ 'Status' | i18n }}</label>
        <input
          tuiInput
          [readonly]="true"
          [class.g-positive]="entry.availability === 'available'"
          [class.g-negative]="entry.availability === 'unavailable'"
          [ngModel]="statuses[entry.availability] | i18n"
        />
      </tui-textfield>
      @if (entry.error; as error) {
        <tui-textfield>
          <label tuiLabel class="g-negative">{{ 'Error' | i18n }}</label>
          <textarea
            tuiTextarea
            class="g-negative"
            [readonly]="true"
            [min]="1"
            [max]="6"
            [ngModel]="error"
          ></textarea>
        </tui-textfield>
      }
      @if (entry.available !== null) {
        <tui-textfield>
          <label tuiLabel>{{ 'Free' | i18n }}</label>
          <input
            tuiInput
            [readonly]="true"
            [ngModel]="entry.available | convertBytes"
          />
        </tui-textfield>
      }
    }
  `,
  styles: `
    :host {
      display: contents;
    }
  `,
  imports: [
    ConvertBytesPipe,
    FormsModule,
    TuiButton,
    TuiIcon,
    TuiInput,
    TuiPassword,
    TuiTextarea,
    TuiTooltip,
    i18nPipe,
  ],
})
export class SourceFields {
  protected readonly copy = inject(CopyService)
  protected readonly types = VOLUME_TYPES
  protected readonly statuses = VOLUME_STATUSES

  public readonly edit = input.required<SourceEdit>()

  protected readonly driveLabel = computed(() => {
    const source = this.edit().entry()?.source
    return source?.kind === 'disk' ? source.label : null
  })
}
