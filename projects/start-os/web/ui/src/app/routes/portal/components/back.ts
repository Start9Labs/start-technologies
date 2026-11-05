import { Location } from '@angular/common'
import { Directive, inject } from '@angular/core'
import { ActivatedRoute, Router } from '@angular/router'

/** Returns to the previous in-app page, or to the parent route when there is none. */
@Directive({
  selector: 'button[appBack]',
  host: { type: 'button', '(click)': 'back()' },
})
export class BackDirective {
  private readonly location = inject(Location)
  private readonly router = inject(Router)
  private readonly route = inject(ActivatedRoute)

  protected back(): void {
    const state = this.location.getState() as { navigationId?: number } | null

    if ((state?.navigationId ?? 0) > 1) {
      this.location.back()
    } else {
      this.router.navigate(['..'], { relativeTo: this.route })
    }
  }
}
