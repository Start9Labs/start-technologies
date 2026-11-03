import { Component, computed, inject, input } from '@angular/core'
import { tuiGenerateId } from '@taiga-ui/cdk'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { fill } from 'src/app/i18n/validation-errors'

const CORE = 6
const WALL = 4
const STEP = 28
const CARD = 140
const GAP = 200
const ROUTER = 86
const GLOBE = 26
// Ellipse aspect of a tube's cross-section.
const FACE = 0.32

// Indexed by distance from the Internet-facing end, so a VPN keeps its color on every page.
const COLORS = [16, 12, 10, 8, 6, 14].map(
  i => `var(--tui-chart-categorical-${String(i).padStart(2, '0')})`,
)

@Component({
  selector: 'vpn-chain',
  template: `
    @let l = layout();
    <div>
      <svg
        [attr.viewBox]="'0 0 ' + l.width + ' ' + l.height"
        [style.max-inline-size.px]="l.width"
        [style.min-inline-size.px]="l.width * 0.7"
      >
        <defs>
          <linearGradient [attr.id]="shade" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stop-color="white" stop-opacity="0.5" />
            <stop offset="0.3" stop-color="white" stop-opacity="0.1" />
            <stop offset="0.55" stop-color="black" stop-opacity="0" />
            <stop offset="1" stop-color="black" stop-opacity="0.35" />
          </linearGradient>
        </defs>
        @for (s of l.servers; track s.label) {
          <rect
            class="card"
            rx="12"
            [attr.x]="s.x - ${CARD / 2}"
            [attr.y]="l.cardY"
            width="${CARD}"
            [attr.height]="l.cardHeight"
            [style.stroke]="s.color"
          />
          <text [attr.x]="s.x" [attr.y]="l.cardY + 24">{{ s.label }}</text>
        }
        @for (t of l.tubes; track $index) {
          <g [style.--c]="t.color">
            <path class="body" [attr.d]="t.body" />
            <path [attr.d]="t.body" [attr.fill]="'url(#' + shade + ')'" />
            <path class="cut" [attr.d]="t.walls" />
            <path class="cut" [attr.d]="t.face" />
          </g>
        }
        @for (s of l.servers; track s.label) {
          <g class="server">
            <rect
              rx="6"
              [attr.x]="s.x - 20"
              [attr.y]="l.cy - s.r - 6"
              width="40"
              [attr.height]="s.r * 2 + 12"
            />
            @for (dy of [-7, 0, 7]; track dy) {
              <circle r="2" [attr.cx]="s.x - 9" [attr.cy]="l.cy + dy" />
              <line
                [attr.x1]="s.x - 3"
                [attr.x2]="s.x + 11"
                [attr.y1]="l.cy + dy"
                [attr.y2]="l.cy + dy"
              />
            }
          </g>
        }
        <g class="router">
          <line
            x1="26"
            x2="26"
            [attr.y1]="l.cy - l.router + 2"
            [attr.y2]="l.cy - l.router - 24"
          />
          <line
            x1="${ROUTER - 16}"
            x2="${ROUTER - 16}"
            [attr.y1]="l.cy - l.router + 2"
            [attr.y2]="l.cy - l.router - 24"
          />
          <rect
            x="8"
            rx="8"
            [attr.y]="l.cy - l.router"
            width="${ROUTER - 8}"
            [attr.height]="l.router * 2"
          />
          <circle cx="24" r="2.5" [attr.cy]="l.cy - 6" />
          <circle cx="24" r="2.5" [attr.cy]="l.cy + 6" />
        </g>
        <text x="${(ROUTER + 8) / 2}" [attr.y]="l.cy + l.router + 22">
          {{ 'Router' | i18n }}
        </text>
        <g class="globe">
          <circle r="${GLOBE}" [attr.cx]="l.gx" [attr.cy]="l.cy" />
          <ellipse rx="11" ry="${GLOBE}" [attr.cx]="l.gx" [attr.cy]="l.cy" />
          <line
            [attr.x1]="l.gx - ${GLOBE}"
            [attr.x2]="l.gx + ${GLOBE}"
            [attr.y1]="l.cy"
            [attr.y2]="l.cy"
          />
          <line
            [attr.x1]="l.gx"
            [attr.x2]="l.gx"
            [attr.y1]="l.cy - ${GLOBE}"
            [attr.y2]="l.cy + ${GLOBE}"
          />
        </g>
        <text [attr.x]="l.gx" [attr.y]="l.cy + ${GLOBE + 22}">
          {{ 'Internet' | i18n }}
        </text>
      </svg>
    </div>
    <ul>
      @for (s of l.servers; track s.label) {
        <li>
          <i [style.background]="s.color"></i>
          {{ s.legend }}
        </li>
      }
      <li>
        <i class="core"></i>
        {{ 'Your traffic' | i18n }}
      </li>
    </ul>
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      font-weight: normal;
    }

    div {
      align-self: stretch;
      overflow-x: auto;
    }

    svg {
      display: block;
      inline-size: 100%;
      block-size: auto;
    }

    text {
      fill: var(--tui-text-primary);
      font-size: 14px;
      font-weight: bold;
      text-anchor: middle;
    }

    .card {
      fill: var(--tui-background-base);
      stroke-width: 2;
    }

    .body {
      fill: var(--c);
    }

    .cut {
      fill: color-mix(in srgb, var(--c), black 25%);
    }

    .server,
    .router {
      rect {
        fill: var(--tui-text-primary);
      }

      circle {
        fill: var(--tui-status-positive);
      }

      line {
        stroke: var(--tui-background-base);
        stroke-width: 2.5;
        stroke-linecap: round;
      }
    }

    .router line {
      stroke: var(--tui-text-primary);
      stroke-width: 6;
    }

    .globe {
      circle {
        fill: var(--tui-status-info);
      }

      ellipse,
      line {
        fill: none;
        stroke: white;
        stroke-width: 2;
      }
    }

    ul {
      align-self: flex-start;
      display: flex;
      flex-wrap: wrap;
      gap: 0.25rem 1.25rem;
      margin: 0.5rem 0 0;
      padding: 0;
      list-style: none;
    }

    li {
      display: flex;
      align-items: center;
      gap: 0.5rem;
    }

    i {
      inline-size: 1.5rem;
      block-size: 0.625rem;
      border-radius: 1rem;
    }

    .core {
      background: var(--tui-text-tertiary);
    }
  `,
  imports: [i18nPipe],
})
export class VPNChain {
  private readonly i18n = inject(i18nPipe)

  protected readonly shade = tuiGenerateId()

  /** The VPN, then each VPN it connects through. */
  readonly chain = input.required<readonly string[]>()

  protected readonly layout = computed(() => {
    // The first VPN's endpoint is reached through the second, so the last VPN is the outermost tunnel.
    const labels = [...this.chain()].reverse()
    const n = labels.length
    const outer = CORE + n * WALL
    const cy = outer + 54
    const first = ROUTER + 16 + n * STEP + 60 + CARD / 2
    const gx = n ? first + (n - 1) * GAP + CARD / 2 + 80 : ROUTER + 140
    const start = ROUTER - 10
    const servers = labels.map((label, p) => {
      const r = CORE + (n - p) * WALL
      const x = first + p * GAP
      // The front half is cut away from the router to here, outer tubes farther.
      const cut = ROUTER + 16 + (n - p) * STEP
      const hole = r - WALL
      return {
        label,
        r,
        x,
        body: `M${cut} ${cy - r}H${x}V${cy + r}H${cut}A${r * FACE} ${r} 0 0 0 ${cut} ${cy - r}Z`,
        color: COLORS[p % COLORS.length],
        walls:
          `M${start} ${cy - r}H${cut}V${cy - hole}H${start}Z` +
          `M${start} ${cy + hole}H${cut}V${cy + r}H${start}Z`,
        face:
          `M${cut} ${cy - r}A${r * FACE} ${r} 0 0 1 ${cut} ${cy + r}` +
          `V${cy + hole}A${hole * FACE} ${hole} 0 0 0 ${cut} ${cy - hole}Z`,
        legend: fill(this.i18n.transform('{vpn} tunnel'), { vpn: label }),
      }
    })

    return {
      cy,
      gx,
      servers,
      router: Math.max(20, outer + 6),
      cardY: cy - outer - 36,
      cardHeight: outer * 2 + 50,
      tubes: [
        {
          body: `M${start} ${cy - CORE}H${gx}V${cy + CORE}H${start}Z`,
          color: 'var(--tui-text-tertiary)',
          walls: '',
          face: '',
        },
        ...[...servers].reverse(),
      ],
      width: gx + GLOBE + 34,
      height: cy + Math.max(outer + 34, GLOBE + 34, 50),
    }
  })
}
