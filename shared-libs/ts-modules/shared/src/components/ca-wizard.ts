import {
  Component,
  DOCUMENT,
  inject,
  InjectionToken,
  input,
  signal,
} from '@angular/core'
import { TuiButton, TuiNotification, TuiTitle } from '@taiga-ui/core'
import { TuiAvatar, TuiBadge, tuiBadgeOptionsProvider } from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader, TuiList } from '@taiga-ui/layout'
import { DocsLinkDirective } from '../directives/docs-link.directive'
import { i18nPipe } from '../i18n/i18n.pipe'
import { i18nKey } from '../i18n/i18n.providers'
import { ROOT_CA_DOWNLOAD_HREF } from '../util/is-ios'

/** Resolves once an HTTPS request to this host succeeds. */
export const CA_TRUST_CHECK = new InjectionToken<() => Promise<unknown>>('', {
  factory: () => {
    const { host } = inject(DOCUMENT).location

    return () => fetch(`https://${host}${ROOT_CA_DOWNLOAD_HREF}`)
  },
})

const PRODUCTS = {
  'start-os': {
    docs: '/start-os/trust-ca.html',
    repeat:
      'You will need to repeat this on every device you use to connect to your server.',
    download:
      'Your server uses its Root CA to generate SSL/TLS certificates for itself and installed services. These certificates are then used to encrypt network traffic with your client devices.',
    trust:
      'Follow instructions for your OS. By trusting your Root CA, your device can verify the authenticity of encrypted communications with your server.',
  },
  'start-wrt': {
    docs: '/start-wrt/trust-ca.html',
    repeat:
      'You will need to repeat this on every device you use to connect to your router.',
    download:
      'Your router uses its Root CA to generate SSL/TLS certificates for itself. These certificates are then used to encrypt network traffic with your client devices.',
    trust:
      'Follow instructions for your OS. By trusting your Root CA, your device can verify the authenticity of encrypted communications with your router.',
  },
} satisfies Record<
  string,
  { docs: string; repeat: i18nKey; download: i18nKey; trust: i18nKey }
>

@Component({
  selector: 'ca-wizard',
  template: `
    @let copy = products[product()];
    @if (!caTrusted()) {
      <div tuiCardLarge>
        <span size="xxl" tuiAvatar="@tui.lock"></span>
        <header tuiHeader>
          <hgroup tuiTitle>
            <h1>{{ 'Trust your Root CA' | i18n }}</h1>
            <p tuiSubtitle>
              {{
                'Download and trust your Root Certificate Authority to establish a secure (HTTPS) connection.'
                  | i18n
              }}
            </p>
          </hgroup>
        </header>
        <div tuiNotification appearance="warning">
          {{ copy.repeat | i18n }}
        </div>
        <ol tuiList="m">
          <li>
            <b>{{ 'Download your Root CA' | i18n }}</b>
            -
            {{ copy.download | i18n }}
            <br />
            <a tuiBadge iconEnd="@tui.download" [href]="rootCaHref">
              {{ 'Download' | i18n }}
            </a>
          </li>
          <li>
            <b>{{ 'Trust your Root CA' | i18n }}</b>
            -
            {{ copy.trust | i18n }}
            <br />
            <a
              tuiBadge
              docsLink
              iconEnd="@tui.external-link"
              [path]="copy.docs"
            >
              {{ 'View instructions' | i18n }}
            </a>
          </li>
          <li>
            <b>{{ 'Test' | i18n }}</b>
            -
            {{
              'Refresh the page. If refreshing the page does not work, you may need to quit and re-open your browser, then revisit this page.'
                | i18n
            }}
            <br />
            <button
              tuiBadge
              appearance="positive"
              iconEnd="@tui.refresh-cw"
              (click)="document.location.reload()"
            >
              {{ 'Refresh' | i18n }}
            </button>
          </li>
        </ol>
        <footer>
          <a
            tuiBadge
            appearance="secondary-grayscale"
            iconEnd="@tui.external-link"
            [href]="httpsUrl"
          >
            {{ 'Skip' | i18n }}
          </a>
          <div>
            <small>({{ 'not recommended' | i18n }})</small>
          </div>
        </footer>
      </div>
    } @else {
      <div tuiCardLarge>
        <span size="xxl" tuiAvatar="@tui.shield" appearance="positive"></span>
        <header tuiHeader>
          <hgroup tuiTitle>
            <h1>{{ 'Root CA Trusted!' | i18n }}</h1>
            <p tuiSubtitle>
              {{
                'You have successfully trusted your Root CA and may now log in securely.'
                  | i18n
              }}
            </p>
          </hgroup>
        </header>
        <footer>
          <a tuiButton iconEnd="@tui.external-link" [href]="httpsUrl">
            {{ 'Go to login' | i18n }}
          </a>
        </footer>
      </div>
    }
  `,
  styles: `
    :host {
      display: contents;
    }

    [tuiTitle] {
      text-align: center;
      min-width: 100%;
    }

    [tuiBadge] {
      margin-block-start: 0.5rem;
      cursor: pointer;
    }

    footer,
    [tuiAvatar] {
      align-self: center;
      text-align: center;
    }
  `,
  providers: [tuiBadgeOptionsProvider({ size: 'xl', appearance: 'primary' })],
  imports: [
    DocsLinkDirective,
    i18nPipe,
    TuiAvatar,
    TuiBadge,
    TuiButton,
    TuiCardLarge,
    TuiHeader,
    TuiList,
    TuiNotification,
    TuiTitle,
  ],
})
export class CaWizard {
  protected readonly document = inject(DOCUMENT)
  protected readonly products = PRODUCTS
  protected readonly rootCaHref = ROOT_CA_DOWNLOAD_HREF
  protected readonly httpsUrl = `https://${this.document.location.host}`
  protected readonly caTrusted = signal(false)

  readonly product = input.required<keyof typeof PRODUCTS>()

  constructor() {
    inject(CA_TRUST_CHECK)().then(
      () => this.caTrusted.set(true),
      () => {},
    )
  }
}
