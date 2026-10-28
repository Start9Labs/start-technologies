const dropId: unique symbol = Symbol('id')
export type DropRef = { [dropId]: number }

export abstract class Drop {
  private static weak: { [id: number]: Drop } = {}
  private static registry = new FinalizationRegistry((id: number) => {
    const weak = Drop.weak[id]
    if (weak) weak.drop()
  })
  private static idCtr: number = 0
  private dropId?: number
  private dropRef?: DropRef | WeakRef<DropRef>
  protected constructor() {
    this.dropId = Drop.idCtr++
    this.dropRef = { [dropId]: this.dropId }
    const weak = this.weak()
    Drop.weak[this.dropId] = weak
    Drop.registry.register(this.dropRef, this.dropId, this.dropRef)

    return new Proxy(this, {
      set(target: any, prop, value) {
        if (prop === 'dropRef' || prop == 'dropId') return false
        target[prop] = value
        ;(weak as any)[prop] = value
        return true
      },
    })
  }
  protected register() {}
  private weak(): this {
    const weak = Object.assign(Object.create(Object.getPrototypeOf(this)), this)
    if (this.dropRef) weak.ref = new WeakRef(this.dropRef)
    return weak
  }
  abstract onDrop(): void
  drop(): void {
    if (!this.dropRef || !this.dropId) return
    this.onDrop()
    this.leak()
  }
  leak(): this {
    if (!this.dropRef || !this.dropId) return this
    Drop.registry.unregister(this.dropRef)
    delete Drop.weak[this.dropId]
    delete this.dropRef
    delete this.dropId
    return this
  }
}

export class DropPromise<T> implements Promise<T> {
  private static dropFns: { [id: number]: () => void } = {}
  private static registry = new FinalizationRegistry((id: number) => {
    const drop = DropPromise.dropFns[id]
    if (drop) {
      drop()
      delete DropPromise.dropFns[id]
    }
  })
  private static idCtr: number = 0
  private dropId: number
  private dropRef: DropRef;
  [Symbol.toStringTag] = 'DropPromise'
  private constructor(
    private readonly promise: Promise<T>,
    dropFnOrRef?: (() => void) | DropRef,
  ) {
    if (dropFnOrRef && dropId in dropFnOrRef) {
      this.dropId = dropFnOrRef[dropId]
      this.dropRef = dropFnOrRef
      return
    }
    this.dropId = DropPromise.idCtr++
    this.dropRef = { [dropId]: this.dropId }
    if (dropFnOrRef) DropPromise.dropFns[this.dropId] = dropFnOrRef
    DropPromise.registry.register(this.dropRef, this.dropId, this.dropRef)
  }
  static of<T>(promise: Promise<T>, dropFn?: () => void): DropPromise<T> {
    return new DropPromise(promise, dropFn)
  }
  static ref<T>(promise: Promise<T>, dropRef: DropRef): DropPromise<T> {
    return new DropPromise(promise, dropRef)
  }
  /** A subscriber holds the drop until the promise settles. */
  private retain() {
    const dropRef = this.dropRef
    this.promise.then(
      () => dropRef,
      () => dropRef,
    )
  }
  then<TResult1 = T, TResult2 = never>(
    onfulfilled?:
      | ((value: T) => TResult1 | PromiseLike<TResult1>)
      | null
      | undefined,
    onrejected?:
      | ((reason: any) => TResult2 | PromiseLike<TResult2>)
      | null
      | undefined,
  ): Promise<TResult1 | TResult2> {
    this.retain()
    return DropPromise.ref(
      this.promise.then(onfulfilled, onrejected),
      this.dropRef,
    )
  }
  catch<TResult = never>(
    onrejected?:
      | ((reason: any) => TResult | PromiseLike<TResult>)
      | null
      | undefined,
  ): Promise<T | TResult> {
    this.retain()
    return DropPromise.ref(this.promise.catch(onrejected), this.dropRef)
  }
  finally(onfinally?: (() => void) | null | undefined): Promise<T> {
    this.retain()
    return DropPromise.ref(this.promise.finally(onfinally), this.dropRef)
  }
}

export class DropGenerator<
  T = unknown,
  TReturn = any,
  TNext = unknown,
> implements AsyncGenerator<T, TReturn, TNext> {
  private static dropFns: { [id: number]: () => void } = {}
  private static registry = new FinalizationRegistry((id: number) => {
    const drop = DropGenerator.dropFns[id]
    if (drop) {
      drop()
      delete DropGenerator.dropFns[id]
    }
  })
  private static idCtr: number = 0
  private dropId: number
  private dropRef: DropRef;
  [Symbol.asyncIterator] = () => this
  private constructor(
    private readonly generator: AsyncGenerator<T, TReturn, TNext>,
    dropFn?: () => void,
  ) {
    this.dropId = DropGenerator.idCtr++
    this.dropRef = { [dropId]: this.dropId }
    if (dropFn) DropGenerator.dropFns[this.dropId] = dropFn
    DropGenerator.registry.register(this.dropRef, this.dropId, this.dropRef)
  }
  static of<T, TReturn, TNext>(
    generator: AsyncGenerator<T, TReturn, TNext>,
    dropFn?: () => void,
  ): DropGenerator<T, TReturn, TNext> {
    return new DropGenerator(generator, dropFn)
  }
  next(...args: [] | [TNext]): Promise<IteratorResult<T, TReturn>> {
    return DropPromise.ref(this.generator.next(...args), this.dropRef)
  }
  return(
    value: TReturn | PromiseLike<TReturn>,
  ): Promise<IteratorResult<T, TReturn>> {
    return DropPromise.ref(this.generator.return(value), this.dropRef)
  }
  throw(e: any): Promise<IteratorResult<T, TReturn>> {
    return DropPromise.ref(this.generator.throw(e), this.dropRef)
  }
  async [Symbol.asyncDispose](): Promise<void> {
    await this.return(undefined as TReturn)
  }
}
