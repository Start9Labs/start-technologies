import {
  ChangeDetectionStrategy,
  Component,
  computed,
  input,
} from '@angular/core'
import {
  ConvertBytesPipe,
  i18nPipe,
  LeafProgress,
  LeafProgressPipe,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiExpand } from '@taiga-ui/core'
import { TuiProgress } from '@taiga-ui/kit'
import { InstallingProgressPipe } from '../pipes/install-progress.pipe'

@Component({
  selector: 'service-progress-phase',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @let leaf = progress() | leafProgress;
    @let percent = leaf | installingProgress;
    <div class="row">
      <b>{{ $any(name()) | i18n }}</b>
      @if (leaf === null) {
        <span>{{ 'waiting' | i18n }}</span>
      } @else if (leaf === true) {
        <span>{{ 'complete' | i18n }}!</span>
      } @else if (leaf === false || leaf.total === null) {
        <span>{{ 'in progress' | i18n }}...</span>
      } @else {
        <span>{{ percent }}%</span>
      }
    </div>
    <progress
      tuiProgressBar
      size="m"
      [max]="100"
      [class.g-positive]="leaf === true"
      [attr.value]="isIndeterminate(leaf) ? undefined : percent"
    ></progress>
    @if (showCounts() && leaf && leaf !== true && leaf.units) {
      @if (leaf.units === 'bytes') {
        <p>
          {{ leaf.done | convertBytes }}
          @if (leaf.total !== null) {
            / {{ leaf.total | convertBytes }}
          }
        </p>
      } @else if (leaf.done > 0 || leaf.total !== null) {
        <p>{{ leaf.done }} / {{ leaf.total ?? '?' }}</p>
      }
    }
    <tui-expand [expanded]="subphases().length > 0">
      <div class="children">
        @for (phase of subphases(); track phase.name) {
          <service-progress-phase
            [name]="phase.name"
            [progress]="phase.progress"
            [showCounts]="showCounts()"
          />
        }
      </div>
    </tui-expand>
  `,
  styles: `
    :host {
      display: block;
    }

    .row {
      display: flex;
      justify-content: space-between;
      gap: 0.5rem;
    }

    .row span {
      text-transform: capitalize;
    }

    b {
      color: var(--tui-text-primary);
    }

    progress {
      margin: 0.375rem 0 0.75rem;
    }

    .children {
      padding-inline-start: 1rem;
      border-inline-start: 1px solid var(--tui-border-normal);
    }
  `,
  imports: [
    ServiceProgressPhaseComponent,
    TuiProgress,
    TuiExpand,
    InstallingProgressPipe,
    LeafProgressPipe,
    ConvertBytesPipe,
    i18nPipe,
  ],
})
export class ServiceProgressPhaseComponent {
  readonly name = input.required<string>()
  readonly progress = input.required<T.Progress>()
  public readonly showCounts = input(false)

  readonly subphases = computed<readonly T.NamedProgress[]>(
    (p = this.progress()) =>
      p !== null && typeof p === 'object' && 'overall' in p ? p.phases : [],
  )

  isIndeterminate(leaf: LeafProgress): boolean {
    return (
      leaf === false || (leaf !== null && leaf !== true && leaf.total === null)
    )
  }
}
