import { Component, inject } from '@angular/core'
import { i18nPipe, MokEnrollmentSteps } from '@start9labs/shared'
import { TuiButton, TuiDialogContext, TuiIcon } from '@taiga-ui/core'
import { injectContext } from '@taiga-ui/polymorpheus'
import { StateService } from '../services/state.service'

@Component({
  standalone: true,
  imports: [TuiButton, TuiIcon, i18nPipe, MokEnrollmentSteps],
  template: `
    @if (!stateService.kiosk) {
      <div class="animation-container">
        <div class="port">
          <div class="port-inner"></div>
        </div>
        <div class="cable">
          <div class="cable-connector"></div>
          <div class="cable-body"></div>
        </div>
      </div>
      <p>
        {{
          'Connect a monitor and keyboard to your server before rebooting.'
            | i18n
        }}
      </p>
    } @else {
      <div class="icon-container">
        <tui-icon icon="@tui.monitor" class="monitor-icon" />
      </div>
      <p>
        {{ 'Keep your monitor connected for the next reboot.' | i18n }}
      </p>
    }

    <div class="mok-info">
      <p>
        {{
          'Your system has Secure Boot enabled, which requires all kernel modules to be signed with a trusted key. Some hardware drivers — such as those for NVIDIA GPUs — are not signed by the default distribution key. Enrolling the StartOS signing key allows your firmware to trust these modules so your hardware can be fully utilized.'
            | i18n
        }}
      </p>
      <mok-enrollment-steps>
        {{ 'When prompted, enter your StartOS password' | i18n }}
      </mok-enrollment-steps>
    </div>

    <footer>
      <button tuiButton (click)="context.completeWith(true)">
        {{ 'Ok' | i18n }}
      </button>
    </footer>
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      align-items: center;
      text-align: center;
    }

    .icon-container {
      margin-bottom: 1rem;
    }

    .monitor-icon {
      width: 3rem;
      height: 3rem;
      color: var(--tui-status-info);
    }

    .animation-container {
      position: relative;
      width: 160px;
      height: 69px;
    }

    .port {
      position: absolute;
      left: 20px;
      top: 50%;
      transform: translateY(-50%);
      width: 28px;
      height: 18px;
      background: var(--tui-background-neutral-1);
      border: 2px solid var(--tui-border-normal);
      border-radius: 2px;
    }

    .port-inner {
      position: absolute;
      top: 3px;
      left: 3px;
      right: 3px;
      bottom: 3px;
      background: var(--tui-background-neutral-2);
      border-radius: 1px;
    }

    .cable {
      position: absolute;
      top: 50%;
      transform: translateY(-50%);
      display: flex;
      align-items: center;
      animation: slide-in 2s ease-in-out 0.5s infinite;
      left: 130px;
    }

    .cable-connector {
      width: 18px;
      height: 12px;
      background: var(--tui-text-secondary);
      border-radius: 1px;
    }

    .cable-body {
      width: 50px;
      height: 6px;
      background: var(--tui-text-tertiary);
      border-radius: 0 3px 3px 0;
    }

    @keyframes slide-in {
      0% {
        left: 130px;
        opacity: 0;
      }
      5% {
        left: 130px;
        opacity: 1;
      }
      60% {
        left: 32px;
        opacity: 1;
      }
      80% {
        left: 32px;
        opacity: 1;
      }
      100% {
        left: 32px;
        opacity: 0;
      }
    }

    .mok-info {
      text-align: left;
      margin-top: 0.5rem;

      p {
        margin: 0 0 0.75rem;
        color: var(--tui-text-secondary);
      }
    }

    p {
      margin: 0 0 1rem;
    }

    footer {
      display: flex;
      justify-content: center;
    }
  `,
})
export class MokEnrollmentDialog {
  protected readonly context = injectContext<TuiDialogContext<boolean>>()
  readonly stateService = inject(StateService)
}
