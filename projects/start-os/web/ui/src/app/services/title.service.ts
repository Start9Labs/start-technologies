import {
  Directive,
  EmbeddedViewRef,
  inject,
  Injectable,
  OnDestroy,
  OnInit,
  signal,
  TemplateRef,
  ViewContainerRef,
} from '@angular/core'
import { tuiButtonOptionsProvider } from '@taiga-ui/core'

@Injectable({
  providedIn: 'root',
})
export class TitleService {
  private vcr?: ViewContainerRef

  readonly corner = signal<TemplateRef<unknown> | null>(null)

  register(vcr: ViewContainerRef) {
    this.vcr = vcr
  }

  add(template: TemplateRef<any>) {
    return this.vcr?.createEmbeddedView(template)
  }
}

@Directive({
  selector: 'ng-template[title]',
  providers: [tuiButtonOptionsProvider({ appearance: '' })],
})
export class TitleDirective implements OnInit, OnDestroy {
  private readonly template = inject(TemplateRef)
  private readonly service = inject(TitleService)
  private view?: EmbeddedViewRef<any>

  ngOnInit() {
    this.view = this.service.add(this.template)
  }

  ngOnDestroy() {
    this.view?.destroy()
  }
}

@Directive({ selector: 'ng-template[corner]' })
export class CornerDirective implements OnInit, OnDestroy {
  private readonly template = inject(TemplateRef)
  private readonly service = inject(TitleService)

  ngOnInit() {
    this.service.corner.set(this.template)
  }

  ngOnDestroy() {
    if (this.service.corner() === this.template) this.service.corner.set(null)
  }
}
