import { T } from '@start9labs/start-sdk'

const CallbackIdCell = { inc: 1 }

const callbackRegistry = new FinalizationRegistry(
  async (options: { cbs: Map<number, Function>; effects: T.Effects }) => {
    await options.effects.clearCallbacks({
      only: Array.from(options.cbs.keys()),
    })
  },
)

export class CallbackHolder {
  constructor(private effects?: T.Effects) {}

  private callbacks = new Map<number, Function>()
  private onLeaveContextCallbacks: Function[] = []
  private children: Map<string, CallbackHolder> = new Map()
  private newId() {
    return CallbackIdCell.inc++
  }
  addCallback(callback?: Function) {
    if (!callback) {
      return
    }
    const id = this.newId()
    console.error('adding callback', id)
    this.callbacks.set(id, callback)
    if (this.effects)
      callbackRegistry.register(this, {
        cbs: this.callbacks,
        effects: this.effects,
      })
    return id
  }
  child(name: string): CallbackHolder {
    this.removeChild(name)
    const child = new CallbackHolder(this.effects)
    this.children.set(name, child)
    return child
  }

  getChild(name: string): CallbackHolder | null {
    return this.children.get(name) || null
  }

  removeChild(name: string) {
    const child = this.children.get(name)
    if (child) {
      child.leaveContext()
      this.children.delete(name)
    }
  }
  private getCallback(index: number): Function | undefined {
    let callback = this.callbacks.get(index)
    if (callback) this.callbacks.delete(index)
    else {
      for (let [_, child] of this.children) {
        callback = child.getCallback(index)
        if (callback) return callback
      }
    }
    return callback
  }
  callCallback(index: number, args: any[]): Promise<unknown> {
    const callback = this.getCallback(index)
    if (!callback) return Promise.resolve()
    return Promise.resolve()
      .then(() => callback(...args))
      .catch(e => console.error('callback failed', e))
  }
  onLeaveContext(fn: Function) {
    this.onLeaveContextCallbacks.push(fn)
  }
  leaveContext() {
    for (let [_, child] of this.children) {
      child.leaveContext()
    }
    this.children = new Map()
    for (let fn of this.onLeaveContextCallbacks) {
      try {
        fn()
      } catch (e) {
        console.warn(e)
      }
    }
    this.onLeaveContextCallbacks = []
  }
}
