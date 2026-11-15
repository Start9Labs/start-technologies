import { Component, inject } from '@angular/core'
import { TuiButton, TuiCell, TuiNotification, TuiTitle } from '@taiga-ui/core'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { i18nKey } from 'src/app/i18n/i18n.providers'
import { fill } from 'src/app/i18n/validation-errors'
import { ConfigFinding, WanConflict } from 'src/app/services/api/api.service'
import { ConfigCheckService } from 'src/app/services/config-check.service'
import { WanOverlapService } from 'src/app/services/wan-overlap.service'
import { LUCI_URL } from 'src/app/utils/luci'

const PROBLEMS: Record<ConfigFinding['problem'], i18nKey> = {
  syntax: 'StartWRT cannot read this file.',
  value: '{option} is set to {value}, which StartWRT cannot read.',
  missing: '{option} is missing.',
  unsupported:
    '{option} is set to {value}, which StartWRT cannot show or keep.',
  unnamed: 'This section has no name.',
  absent: 'This section is missing.',
  extra:
    'StartWRT shows one of these per radio. This one was added outside StartWRT.',
  unmanaged:
    'This section also has {option} {value}, set outside StartWRT. StartWRT shows the first value and leaves this one as it is.',
}

function key(finding: ConfigFinding): string {
  const { config, line, sectionType, section, option, problem } = finding

  return [config, line, sectionType, section, option, problem].join(':')
}

function describe(
  i18n: i18nPipe,
  { problem, option, found }: ConfigFinding,
): string {
  return fill(i18n.transform(PROBLEMS[problem]), {
    option: option ?? '',
    value: found ?? '',
  })
}

function locate(
  i18n: i18nPipe,
  { config, line, section, sectionType }: ConfigFinding,
): string {
  return [
    `/etc/config/${config}`,
    line && fill(i18n.transform('line {line}'), { line }),
    [sectionType, section].filter(Boolean).join(' '),
  ]
    .filter(Boolean)
    .join(' · ')
}

@Component({
  selector: 'app-custom-config',
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2>{{ 'Custom configuration' | i18n }}</h2>
      </hgroup>
    </header>
    <article class="g-form" tuiCardLarge>
      <p>
        {{
          'This page has settings that were changed outside StartWRT, for example in LuCI, in a way StartWRT cannot show or keep. To keep them, manage them in LuCI.'
            | i18n
        }}
      </p>
      @if (check.loadError(); as error) {
        <div tuiNotification appearance="negative">{{ error }}</div>
      }
      @for (finding of check.blocking(); track key(finding)) {
        <div tuiCell>
          <div tuiTitle>
            {{ describe(finding) }}
            <div tuiSubtitle>{{ locate(finding) }}</div>
            @if (finding.detail) {
              <div tuiSubtitle>{{ finding.detail }}</div>
            }
          </div>
        </div>
      }
      <footer>
        <a tuiButton [href]="luciUrl" target="_blank">
          {{ 'Launch LuCI Interface' | i18n }}
        </a>
        <button tuiButton appearance="secondary" (click)="check.recheck()">
          {{ 'Check again' | i18n }}
        </button>
      </footer>
    </article>
  `,
  styles: `
    :host {
      margin: 1rem 1.5rem;
    }

    footer {
      display: flex;
      flex-wrap: wrap;
      gap: 0.5rem;
    }
  `,
  host: { class: 'g-page' },
  imports: [
    TuiButton,
    TuiCardLarge,
    TuiCell,
    TuiHeader,
    TuiNotification,
    TuiTitle,
    i18nPipe,
  ],
})
export class CustomConfig {
  private readonly i18n = inject(i18nPipe)

  protected readonly check = inject(ConfigCheckService)
  protected readonly luciUrl = LUCI_URL
  protected readonly key = key

  protected describe(finding: ConfigFinding): string {
    return describe(this.i18n, finding)
  }

  protected locate(finding: ConfigFinding): string {
    return locate(this.i18n, finding)
  }
}

@Component({
  selector: 'app-config-notices',
  template: `
    @for (conflict of wan.conflicts(); track conflict.profile) {
      <div tuiNotification appearance="warning">
        <div tuiTitle>
          {{ wanConflict(conflict) }}
          <div tuiSubtitle>{{ wanFix(conflict.suggestion) }}</div>
        </div>
      </div>
    }
    @for (finding of check.notices(); track key(finding)) {
      <div tuiNotification appearance="warning">
        <div tuiTitle>
          {{ describe(finding) }}
          <div tuiSubtitle>{{ locate(finding) }}</div>
        </div>
      </div>
    }
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      gap: 0.5rem;
      margin: 1rem 1.5rem 0;

      &:empty {
        display: none;
      }
    }
  `,
  imports: [TuiNotification, TuiTitle],
})
export class ConfigNotices {
  private readonly i18n = inject(i18nPipe)

  protected readonly check = inject(ConfigCheckService)
  protected readonly wan = inject(WanOverlapService)
  protected readonly key = key

  protected describe(finding: ConfigFinding): string {
    return describe(this.i18n, finding)
  }

  protected locate(finding: ConfigFinding): string {
    return locate(this.i18n, finding)
  }

  protected wanConflict({ profile, subnet, wan }: WanConflict): string {
    return fill(
      this.i18n.transform(
        'Security Profile {profile} uses {subnet}, which overlaps your Internet connection (WAN {wan}).',
      ),
      { profile, subnet, wan },
    )
  }

  protected wanFix(suggestion: string | null): string {
    return suggestion
      ? fill(
          this.i18n.transform('Change its subnet, for example to {subnet}.'),
          { subnet: suggestion },
        )
      : this.i18n.transform('Change the Network Block in LAN Settings.')
  }
}
