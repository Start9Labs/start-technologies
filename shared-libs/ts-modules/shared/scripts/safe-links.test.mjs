import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { DOCUMENT, ElementRef, Injector, runInInjectionContext } from '@angular/core'
import { WaMutationObserverService } from '@ng-web-apis/mutation-observer'
import { Subject } from 'rxjs'
import './typescript-loader.mjs'

const { SafeLinksDirective } =
  await import('../src/directives/safe-links.directive.ts')

const previousNode = Object.getOwnPropertyDescriptor(globalThis, 'Node')
Object.defineProperty(globalThis, 'Node', {
  configurable: true,
  value: { ELEMENT_NODE: 1 },
})
test.after(() => {
  if (previousNode) Object.defineProperty(globalThis, 'Node', previousNode)
  else delete globalThis.Node
})

test('different ports and protocols open separately while same-origin links stay local', () => {
  const links = [
    'https://start.local/help',
    'https://start.local:8080/',
    'http://start.local/',
    'https://other.local/',
  ].map(url => {
    const address = new URL(url)
    return {
      origin: address.origin,
      hostname: address.hostname,
      target: '',
      attributes: {},
      classList: new Set(),
      setAttribute(name, value) {
        this.attributes[name] = value
      },
    }
  })
  const outside = {
    ...links[3],
    attributes: {},
    classList: new Set(),
  }
  const mutations = new Subject()
  const injector = Injector.create({
    providers: [
      {
        provide: DOCUMENT,
        useValue: {
          location: new URL('https://start.local/'),
          links: [...links, outside],
        },
      },
      {
        provide: ElementRef,
        useValue: new ElementRef({ contains: link => links.includes(link) }),
      },
      { provide: WaMutationObserverService, useValue: mutations },
    ],
  })
  try {
    runInInjectionContext(injector, () => new SafeLinksDirective())
    mutations.next([])
    assert.equal(links[0].target, '')
    assert.equal(outside.target, '')
    for (const link of links.slice(1)) {
      assert.equal(link.target, '_blank')
      assert.equal(link.attributes.rel, 'noopener noreferrer')
      assert.equal(link.classList.has('g-external-link'), true)
    }
  } finally {
    injector.destroy()
  }
})

test('links inside the host stop propagation without preventing navigation', () => {
  const link = {}
  const ancestorLink = {}
  const directive = Object.assign(Object.create(SafeLinksDirective.prototype), {
    el: { contains: node => node === link },
  })

  for (const [target, stopped] of [
    [{ nodeType: 1, closest: () => link }, true],
    [{ nodeType: 1, closest: () => ancestorLink }, false],
    [{ nodeType: 1, closest: () => null }, false],
    [{ nodeType: 3 }, false],
  ]) {
    const event = new Event('click', { bubbles: true, cancelable: true })
    Object.defineProperty(event, 'target', { value: target })
    directive.onClick(event)
    assert.equal(event.cancelBubble, stopped)
    assert.equal(event.defaultPrevented, false)
  }
})
