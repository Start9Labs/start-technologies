import { bootstrapApplication } from '@angular/platform-browser'
import { TuiRoot } from '@taiga-ui/core'
import { Component } from '@angular/core'
import { RouterOutlet } from '@angular/router'

import { appConfig } from './app/app.config'

@Component({
  selector: 'app-root',
  imports: [RouterOutlet, TuiRoot],
  template: '<tui-root><router-outlet /></tui-root>',
  styles: `
    :host {
      height: 100%;
      display: block;
    }

    tui-root {
      height: 100%;
    }
  `,
})
class App {}

bootstrapApplication(App, appConfig).catch(console.error)
