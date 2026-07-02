import {
  Component,
  computed,
  inject,
  INJECTOR,
  linkedSignal,
  model,
  WritableSignal,
} from '@angular/core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiButton } from '@taiga-ui/core'
import { TuiAutoColorPipe, TuiTimeline } from '@taiga-ui/kit'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { AddWindow } from 'src/app/components/window'
import { ScheduleWindow } from 'src/app/services/api/api.service'
import {
  formatTime12h,
  quarterHourToTime,
  timeToQuarterHour,
} from 'src/app/utils/schedule'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  selector: 'app-schedule',
  template: `
    @for (col of columns(); track col.day) {
      <tui-timeline
        #timeline
        orientation="vertical"
        [max]="96"
        [template]="gap"
      >
        <span class="day">{{ labels[col.day] | i18n }}</span>
        @for (block of col.blocks; track block.windowIndex + '-' + block.kind) {
          <label
            tuiTimelineItem
            [draggable]="false"
            [resizable]="false"
            [style.--color]="
              ((block.windowIndex + 1) * 123).toString() | tuiAutoColor
            "
            [value]="block.range"
            (click)="edit(block.windowIndex)"
          >
            @if (block.kind !== 'tail') {
              <span class="time start">{{ block.start }}</span>
            }
            @if (block.kind !== 'head') {
              <span class="time end">{{ block.end }}</span>
            }
          </label>
        }
        <ng-template #gap let-index>
          <div [style.width.%]="100" [style.overflow]="'hidden'">
            <button
              tuiIconButton
              iconStart="@tui.plus"
              size="s"
              type="button"
              appearance="secondary-grayscale"
              (click)="add(col.day, index, timeline.value())"
            >
              {{ 'Add' | i18n }}
            </button>
          </div>
        </ng-template>
      </tui-timeline>
    }
  `,
  styles: `
    :host {
      max-width: 50rem;
      display: grid;
      grid-auto-flow: column;
      grid-auto-columns: minmax(0, 1fr);
      height: 36rem;
      max-height: calc(100svh - 12rem);
      gap: 0.125rem;
      border-radius: var(--tui-radius-s);
      overflow: hidden;
    }

    tui-timeline {
      background: var(--tui-background-neutral-1);
      border-radius: var(--tui-radius-xs);
      border-inline-start: 2rem solid transparent;
    }

    [tuiTimelineItem] {
      background: var(--tui-background-neutral-1);
      clip-path: inset(1px 0 round 0.125rem);
      /* Blocks are not draggable/resizable; re-enable pointer events (the
         timeline item host sets pointer-events: none) so click to edit
         works. */
      pointer-events: auto;
      cursor: pointer;

      &::before {
        content: '';
        position: absolute;
        inset: 0;
        background: var(--color);
        opacity: 0.25;
      }
    }

    .day {
      writing-mode: horizontal-tb;
      position: absolute;
      top: -2rem;
      width: 100%;
      text-align: center;
      line-height: calc(2rem - 1px);
      border-radius: inherit;
      border-bottom-left-radius: 0;
      border-bottom-right-radius: 0;
      background: var(--tui-border-normal);
    }

    .time {
      position: absolute;
      writing-mode: horizontal-tb;
      width: 100%;
      text-align: center;
      line-height: 1.5em;
      backdrop-filter: brightness(1);

      :host-context([tuiTheme='dark']) & {
        backdrop-filter: brightness(0.3);
      }

      &.start {
        top: 0;
      }

      &.end {
        bottom: 0;
      }
    }

    button {
      inset: 50%;
      border-radius: 100%;
      transform: translate(-50%, -50%);
      visibility: hidden;
    }

    tui-timeline:hover button {
      visibility: visible;
    }

    :host-context(tui-root._mobile) {
      width: fit-content;
      grid-auto-columns: max-content;

      &:hover {
        tui-timeline:hover {
          width: 6rem;
        }

        tui-timeline:not(:hover) .day {
          font-size: 0;
        }
      }

      &:not(:hover) {
        tui-timeline:first-child {
          width: 6rem;
        }

        tui-timeline:not(:first-child) .day {
          font-size: 0;
        }
      }

      tui-timeline {
        width: 1.75rem;
      }
    }
  `,
  host: { '(change)': 'save()' },
  imports: [TuiTimeline, TuiAutoColorPipe, TuiButton, i18nPipe],
})
export class ScheduleComponent {
  public readonly windows = model<ScheduleWindow[]>([])

  // Display order: Mon–Sun; data order: Sun(0)–Sat(6)
  protected readonly order = [1, 2, 3, 4, 5, 6, 0] as const
  protected readonly labels = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']
  protected readonly dialogs = inject(TuiResponsiveDialogService)
  protected readonly injector = inject(INJECTOR)
  private readonly i18n = inject(i18nPipe)
  protected readonly source = computed(() =>
    this.windows().map(({ startTime, endTime, days }) => ({
      range: [from(startTime), from(endTime)] as const,
      days,
    })),
  )

  protected readonly view: WritableSignal<ReturnType<typeof this.source>> =
    linkedSignal({
      source: this.source,
      computation: (source, current) =>
        current?.value.length ? current.value : source,
    })

  // Per day-column render blocks. A window that genuinely spills past midnight
  // (range start >= end with end > 0 — start > end crosses midnight, start ==
  // end is a 24h window like 09:00-09:00) renders as two blocks for one logical
  // window: a "head" at the end of its own day(s) and a "tail" at the start of
  // the next day(s). Non-wrapping windows — and windows ending exactly at
  // midnight (end == 0), which close on their own day with no tail — stay a
  // single "whole" block. A "whole" block labels both its start (top) and end
  // (bottom). To read as one continuous overnight window rather than two, the
  // head omits its midnight end label and the tail omits its midnight start
  // label (see template) — leaving only the real start on the head and the real
  // end on the tail. All blocks are edited via the dialog (single click); none
  // are draggable or resizable.
  protected readonly columns = computed(() =>
    this.order.map(day => {
      const blocks: {
        windowIndex: number
        kind: 'whole' | 'head' | 'tail'
        range: readonly [number, number]
        start: string
        end: string
      }[] = []
      const block = (
        windowIndex: number,
        kind: 'whole' | 'head' | 'tail',
        range: readonly [number, number],
      ) => ({
        windowIndex,
        kind,
        range,
        start: format(to(range[0])),
        end: format(to(range[1])),
      })
      this.view().forEach((win, windowIndex) => {
        const [start, end] = win.range
        // start >= end spans past midnight, but a window ending *exactly* at
        // midnight (end == 0) closes on its own day — no tail spills into the
        // next column — so it stays a single 'whole' block keeping both its
        // start and end (12:00am) labels. Only a window whose end lands after
        // midnight (end > 0) splits into a head here + a next-day tail.
        const wrap = start >= end
        const wrapsNextDay = wrap && end > 0
        if (win.days[day]) {
          blocks.push(
            block(
              windowIndex,
              wrapsNextDay ? 'head' : 'whole',
              wrap ? ([start, 96] as const) : win.range,
            ),
          )
        }
        // The previous day's wrapping window spills its tail into this column.
        if (wrapsNextDay && win.days[(day + 6) % 7]) {
          blocks.push(block(windowIndex, 'tail', [0, end] as const))
        }
      })
      return { day, blocks }
    }),
  )

  // All other windows as ScheduleWindows, for the dialog's overlap check.
  private others(exclude?: number): ScheduleWindow[] {
    return this.view()
      .filter((_, i) => i !== exclude)
      .map(({ range, days }) => ({
        startTime: to(range[0]),
        endTime: to(range[1]),
        days,
      }))
  }

  protected edit(index: number) {
    this.dialogs
      .open<ScheduleWindow | null>(
        new PolymorpheusComponent(AddWindow, this.injector),
        {
          label: this.i18n.transform('Edit Blackout Window'),
          data: {
            startTime: to(this.view()[index].range[0]),
            endTime: to(this.view()[index].range[1]),
            days: this.view()[index].days,
            others: this.others(index),
            edit: true,
          },
        },
      )
      .subscribe(value => {
        this.view.update(windows =>
          windows
            .map<any>((item, i) =>
              i === index
                ? value && {
                    range: [from(value.startTime), from(value.endTime)],
                    days: value.days,
                  }
                : item,
            )
            .filter(Boolean),
        )
        this.save()
      })
  }

  protected add(
    day: number,
    index: number,
    ranges: (readonly [number, number])[],
  ) {
    const days = this.order.map(() => false)
    const start =
      ranges.map(([_, end]) => end).sort((a, b) => a - b)[index - 1] || 0

    days[day] = true
    this.dialogs
      .open<ScheduleWindow>(
        new PolymorpheusComponent(AddWindow, this.injector),
        {
          label: this.i18n.transform('Add Blackout Window'),
          data: {
            startTime: to(start),
            endTime: to(start + 4),
            days,
            others: this.others(),
            edit: false,
          },
        },
      )
      .subscribe(({ days, startTime, endTime }) => {
        this.view.update(windows =>
          windows.concat({ range: [from(startTime), from(endTime)], days }),
        )
        this.save()
      })
  }

  protected save() {
    this.windows.set(
      this.view().map(({ range, days }) => ({
        startTime: to(range[0]),
        endTime: to(range[1]),
        days,
      })),
    )
  }
}

const to = quarterHourToTime
const from = timeToQuarterHour
const format = formatTime12h
