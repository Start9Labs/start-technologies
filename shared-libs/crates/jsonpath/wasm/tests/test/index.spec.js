const jsonpath = require('jsonpath-wasm')
const assert = require('assert')

let jsonObj = {
  store: {
    book: [
      {
        category: 'reference',
        author: 'Nigel Rees',
        title: 'Sayings of the Century',
        price: 8.95,
      },
      {
        category: 'fiction',
        author: 'Evelyn Waugh',
        title: 'Sword of Honour',
        price: 12.99,
      },
      {
        category: 'fiction',
        author: 'Herman Melville',
        title: 'Moby Dick',
        isbn: '0-553-21311-3',
        price: 8.99,
      },
      {
        category: 'fiction',
        author: 'J. R. R. Tolkien',
        title: 'The Lord of the Rings',
        isbn: '0-395-19395-8',
        price: 22.99,
      },
    ],
    bicycle: {
      color: 'red',
      price: 19.95,
    },
  },
  expensive: 10,
}

let list = {
  '$.store.book[*].author': [
    'Nigel Rees',
    'Evelyn Waugh',
    'Herman Melville',
    'J. R. R. Tolkien',
  ],

  '$..author': [
    'Nigel Rees',
    'Evelyn Waugh',
    'Herman Melville',
    'J. R. R. Tolkien',
  ],

  '$.store.*': [
    [
      {
        category: 'reference',
        author: 'Nigel Rees',
        title: 'Sayings of the Century',
        price: 8.95,
      },
      {
        category: 'fiction',
        author: 'Evelyn Waugh',
        title: 'Sword of Honour',
        price: 12.99,
      },
      {
        category: 'fiction',
        author: 'Herman Melville',
        title: 'Moby Dick',
        isbn: '0-553-21311-3',
        price: 8.99,
      },
      {
        category: 'fiction',
        author: 'J. R. R. Tolkien',
        title: 'The Lord of the Rings',
        isbn: '0-395-19395-8',
        price: 22.99,
      },
    ],
    {
      color: 'red',
      price: 19.95,
    },
  ],

  '$.store..price': [8.95, 12.99, 8.99, 22.99, 19.95],

  '$..book[2]': [
    {
      category: 'fiction',
      author: 'Herman Melville',
      title: 'Moby Dick',
      isbn: '0-553-21311-3',
      price: 8.99,
    },
  ],

  '$..book[-2]': [
    {
      category: 'fiction',
      author: 'Herman Melville',
      title: 'Moby Dick',
      isbn: '0-553-21311-3',
      price: 8.99,
    },
  ],

  '$..book[0,1]': [
    {
      category: 'reference',
      author: 'Nigel Rees',
      title: 'Sayings of the Century',
      price: 8.95,
    },
    {
      category: 'fiction',
      author: 'Evelyn Waugh',
      title: 'Sword of Honour',
      price: 12.99,
    },
  ],

  '$..book[:2]': [
    {
      category: 'reference',
      author: 'Nigel Rees',
      title: 'Sayings of the Century',
      price: 8.95,
    },
    {
      category: 'fiction',
      author: 'Evelyn Waugh',
      title: 'Sword of Honour',
      price: 12.99,
    },
  ],

  '$..book[1:2]': [
    {
      category: 'fiction',
      author: 'Evelyn Waugh',
      title: 'Sword of Honour',
      price: 12.99,
    },
  ],

  '$..book[-2:]': [
    {
      category: 'fiction',
      author: 'Herman Melville',
      title: 'Moby Dick',
      isbn: '0-553-21311-3',
      price: 8.99,
    },
    {
      category: 'fiction',
      author: 'J. R. R. Tolkien',
      title: 'The Lord of the Rings',
      isbn: '0-395-19395-8',
      price: 22.99,
    },
  ],

  '$..book[2:]': [
    {
      category: 'fiction',
      author: 'Herman Melville',
      title: 'Moby Dick',
      isbn: '0-553-21311-3',
      price: 8.99,
    },
    {
      category: 'fiction',
      author: 'J. R. R. Tolkien',
      title: 'The Lord of the Rings',
      isbn: '0-395-19395-8',
      price: 22.99,
    },
  ],

  '$..book[?(@.isbn)]': [
    {
      category: 'fiction',
      author: 'Herman Melville',
      title: 'Moby Dick',
      isbn: '0-553-21311-3',
      price: 8.99,
    },
    {
      category: 'fiction',
      author: 'J. R. R. Tolkien',
      title: 'The Lord of the Rings',
      isbn: '0-395-19395-8',
      price: 22.99,
    },
  ],

  '$.store.book[?(@.price < 10)]': [
    {
      category: 'reference',
      author: 'Nigel Rees',
      title: 'Sayings of the Century',
      price: 8.95,
    },
    {
      category: 'fiction',
      author: 'Herman Melville',
      title: 'Moby Dick',
      isbn: '0-553-21311-3',
      price: 8.99,
    },
  ],

  '$..*': [
    {
      book: [
        {
          category: 'reference',
          author: 'Nigel Rees',
          title: 'Sayings of the Century',
          price: 8.95,
        },
        {
          category: 'fiction',
          author: 'Evelyn Waugh',
          title: 'Sword of Honour',
          price: 12.99,
        },
        {
          category: 'fiction',
          author: 'Herman Melville',
          title: 'Moby Dick',
          isbn: '0-553-21311-3',
          price: 8.99,
        },
        {
          category: 'fiction',
          author: 'J. R. R. Tolkien',
          title: 'The Lord of the Rings',
          isbn: '0-395-19395-8',
          price: 22.99,
        },
      ],
      bicycle: {
        color: 'red',
        price: 19.95,
      },
    },
    10,
    [
      {
        category: 'reference',
        author: 'Nigel Rees',
        title: 'Sayings of the Century',
        price: 8.95,
      },
      {
        category: 'fiction',
        author: 'Evelyn Waugh',
        title: 'Sword of Honour',
        price: 12.99,
      },
      {
        category: 'fiction',
        author: 'Herman Melville',
        title: 'Moby Dick',
        isbn: '0-553-21311-3',
        price: 8.99,
      },
      {
        category: 'fiction',
        author: 'J. R. R. Tolkien',
        title: 'The Lord of the Rings',
        isbn: '0-395-19395-8',
        price: 22.99,
      },
    ],
    {
      color: 'red',
      price: 19.95,
    },
    {
      category: 'reference',
      author: 'Nigel Rees',
      title: 'Sayings of the Century',
      price: 8.95,
    },
    {
      category: 'fiction',
      author: 'Evelyn Waugh',
      title: 'Sword of Honour',
      price: 12.99,
    },
    {
      category: 'fiction',
      author: 'Herman Melville',
      title: 'Moby Dick',
      isbn: '0-553-21311-3',
      price: 8.99,
    },
    {
      category: 'fiction',
      author: 'J. R. R. Tolkien',
      title: 'The Lord of the Rings',
      isbn: '0-395-19395-8',
      price: 22.99,
    },
    'reference',
    'Nigel Rees',
    'Sayings of the Century',
    8.95,
    'fiction',
    'Evelyn Waugh',
    'Sword of Honour',
    12.99,
    'fiction',
    'Herman Melville',
    'Moby Dick',
    '0-553-21311-3',
    8.99,
    'fiction',
    'J. R. R. Tolkien',
    'The Lord of the Rings',
    '0-395-19395-8',
    22.99,
    'red',
    19.95,
  ],

  '$..book[ ?( (@.price < 13 || $.store.bicycle.price < @.price) && @.price <=10 ) ]':
    [
      {
        category: 'reference',
        author: 'Nigel Rees',
        title: 'Sayings of the Century',
        price: 8.95,
      },
      {
        category: 'fiction',
        author: 'Herman Melville',
        title: 'Moby Dick',
        isbn: '0-553-21311-3',
        price: 8.99,
      },
    ],
}

describe('compile test', () => {
  it('basic', done => {
    let template = jsonpath.compile('$.a')
    let result = template({ a: 1 })
    if (result[0] === 1) {
      done()
    }
  })
})

describe('compatible behavior with Compiled::compile test', () => {
  it('basic', done => {
    assert.ok(jsonpath.compile('').includes('path error'), 'Error thrown')
    done()
  })
})

describe('selector test', () => {
  it('basic', done => {
    let selector = jsonpath.selector({ a: 1 })
    let result = selector('$.a')
    if (result[0] === 1) {
      done()
    }
  })
})

describe('select test', () => {
  it('basic', done => {
    let result = jsonpath.select({ a: 1 }, '$.a')
    if (result[0] === 1) {
      done()
    }
  })
})

describe('filter test', () => {
  function run(done, path, expected) {
    let result = jsonpath.select(jsonObj, path)
    if (JSON.stringify(result) === JSON.stringify(expected)) {
      done()
    }
  }

  for (var i in list) {
    it(i, done => {
      run(done, i, list[i])
    })
  }

  it('object equal', done => {
    let selector = new jsonpath.Selector()
    selector.path('$..[?(@.a == 1)]')
    selector.value({
      a: 1,
      b: { a: 1 },
      c: { a: 1 },
    })
    let result = selector.select()
    if (JSON.stringify(result) === JSON.stringify([{ a: 1 }, { a: 1 }])) {
      done()
    }
  })

  it('escaped single quote notation', done => {
    let result = jsonpath.select(
      { "single'quote": 'value' },
      "$['single\\'quote']",
    )
    if (JSON.stringify(result) === JSON.stringify(['value'])) {
      done()
    }
  })

  it('escaped double quote notation', done => {
    let result = jsonpath.select(
      { 'single"quote': 'value' },
      "$['single\"quote']",
    )
    if (JSON.stringify(result) === JSON.stringify(['value'])) {
      done()
    }
  })

  it('array range with step - $[::]', done => {
    let result = jsonpath.select(
      ['first', 'second', 'third', 'forth', 'fifth'],
      '$[::]',
    )
    if (
      JSON.stringify(result) ===
      JSON.stringify(['first', 'second', 'third', 'forth', 'fifth'])
    ) {
      done()
    }
  })

  it('array range with step - $[::2]', done => {
    let result = jsonpath.select(
      ['first', 'second', 'third', 'forth', 'fifth'],
      '$[::2]',
    )
    if (
      JSON.stringify(result) === JSON.stringify(['first', 'third', 'fifth'])
    ) {
      done()
    }
  })

  it('array range with step - $[1: :]', done => {
    let result = jsonpath.select(
      ['first', 'second', 'third', 'forth', 'fifth'],
      '$[1: :]',
    )
    if (
      JSON.stringify(result) ===
      JSON.stringify(['second', 'third', 'forth', 'fifth'])
    ) {
      done()
    }
  })

  it('array range with step - $[1:2:]', done => {
    let result = jsonpath.select(
      ['first', 'second', 'third', 'forth', 'fifth'],
      '$[1:2:]',
    )
    if (JSON.stringify(result) === JSON.stringify(['second'])) {
      done()
    }
  })

  it('array range with step - $[1::2]', done => {
    let result = jsonpath.select(
      ['first', 'second', 'third', 'forth', 'fifth'],
      '$[1::2]',
    )
    if (JSON.stringify(result) === JSON.stringify(['second', 'forth'])) {
      done()
    }
  })

  it('array range with step - $[0:3:1]', done => {
    let result = jsonpath.select(
      ['first', 'second', 'third', 'forth', 'fifth'],
      '$[0:3:1]',
    )
    if (
      JSON.stringify(result) === JSON.stringify(['first', 'second', 'third'])
    ) {
      done()
    }
  })

  it('array range with step - $[0:3:2]', done => {
    let result = jsonpath.select(
      ['first', 'second', 'third', 'forth', 'fifth'],
      '$[0:3:2]',
    )
    if (JSON.stringify(result) === JSON.stringify(['first', 'third'])) {
      done()
    }
  })

  it('array keys', done => {
    let result = jsonpath.select(
      {
        key1: 'value1',
        key2: 2,
      },
      "$['key1', 'key2']",
    )
    if (JSON.stringify(result) === JSON.stringify(['value1', 2])) {
      done()
    }
  })
})

describe('SelectorMut test', () => {
  it('delete', done => {
    let jsonObjNew = JSON.parse(JSON.stringify(jsonObj))
    let result = jsonpath.deleteValue(jsonObjNew, '$.store.book')
    if (
      JSON.stringify(result) ===
      JSON.stringify({
        store: {
          book: null,
          bicycle: {
            color: 'red',
            price: 19.95,
          },
        },
        expensive: 10,
      })
    ) {
      done()
    }
  })

  it('replaceWith', done => {
    let jsonObjNew = JSON.parse(JSON.stringify(jsonObj))
    let result = jsonpath.replaceWith(jsonObjNew, '$.store.book', v => {
      let ret = v[0]
      ret.price = 9
      return ret
    })
    if (
      JSON.stringify(result) ===
      JSON.stringify({
        store: {
          book: {
            category: 'reference',
            author: 'Nigel Rees',
            title: 'Sayings of the Century',
            price: 9,
          },
          bicycle: {
            color: 'red',
            price: 19.95,
          },
        },
        expensive: 10,
      })
    ) {
      done()
    }
  })

  it('SeletorMut delete', done => {
    let jsonObjNew = JSON.parse(JSON.stringify(jsonObj))
    let selector = new jsonpath.SelectorMut()
    selector.path('$.store.book')
    selector.value(jsonObjNew)
    selector.deleteValue()

    let result = selector.take()
    if (
      JSON.stringify(result) ===
      JSON.stringify({
        store: {
          book: null,
          bicycle: {
            color: 'red',
            price: 19.95,
          },
        },
        expensive: 10,
      })
    ) {
      done()
    }
  })

  it('SeletorMut replaceWith', done => {
    let jsonObjNew = JSON.parse(JSON.stringify(jsonObj))
    let selector = new jsonpath.SelectorMut()
    selector.path('$.store.book')
    selector.value(jsonObjNew)
    selector.replaceWith(v => {
      let ret = v[0]
      ret.price = 9
      return ret
    })

    let result = selector.take()
    if (
      JSON.stringify(result) ===
      JSON.stringify({
        store: {
          book: {
            category: 'reference',
            author: 'Nigel Rees',
            title: 'Sayings of the Century',
            price: 9,
          },
          bicycle: {
            color: 'red',
            price: 19.95,
          },
        },
        expensive: 10,
      })
    ) {
      done()
    }
  })
})

describe('Selector test', () => {
  it('select', done => {
    let selector = new jsonpath.Selector()
    selector.value(jsonObj)
    for (var i in list) {
      selector.path(i)
      if (JSON.stringify(list[i]) !== JSON.stringify(selector.select())) {
        throw `fail: ${i}`
      }
    }
    done()
  })
})

describe('README test', () => {
  it('jsonpath.Selector', done => {
    let jsonObj = {
      school: {
        friends: [
          { name: '친구1', age: 20 },
          { name: '친구2', age: 20 },
        ],
      },
      friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
    }

    let selector = new jsonpath.Selector()
    selector.value(jsonObj)

    {
      selector.path('$..[?(@.age >= 30)]')
      let jsonObj = selector.select()
      let resultObj = [{ name: '친구3', age: 30 }]
      if (JSON.stringify(jsonObj) !== JSON.stringify(resultObj)) {
        throw 'jsonpath.Selector: $..[?(@.age >= 30)]'
      }
    }

    {
      selector.path('$..[?(@.age == 20)]')
      let jsonObj = selector.select()
      let resultObj = [
        { name: '친구1', age: 20 },
        { name: '친구2', age: 20 },
      ]
      if (JSON.stringify(jsonObj) !== JSON.stringify(resultObj)) {
        throw 'jsonpath.Selector: $..[?(@.age >= 20)]'
      }
    }

    {
      selector.value({ friends: [{ name: '친구5', age: 20 }] })
      let jsonObj = selector.select()
      let resultObj = [{ name: '친구5', age: 20 }]
      if (JSON.stringify(jsonObj) !== JSON.stringify(resultObj)) {
        throw 'jsonpath.Selector: change value'
      }
    }

    done()
  })

  it('jsonpath.SelectorMut', done => {
    let jsonObj = {
      school: {
        friends: [
          { name: '친구1', age: 20 },
          { name: '친구2', age: 20 },
        ],
      },
      friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
    }

    let selector = new jsonpath.SelectorMut()
    selector.path('$..[?(@.age == 20)]')
    {
      selector.value(jsonObj)
      selector.deleteValue()

      let resultObj = {
        school: { friends: [null, null] },
        friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
      }
      if (JSON.stringify(selector.take()) !== JSON.stringify(resultObj)) {
        throw 'jsonpath.SelectorMut.deleteValue'
      }
    }

    {
      selector.value(jsonObj)
      selector.replaceWith(v => {
        v.age = v.age * 2
        return v
      })

      let resultObj = {
        school: {
          friends: [
            { name: '친구1', age: 40 },
            { name: '친구2', age: 40 },
          ],
        },
        friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
      }
      if (JSON.stringify(selector.take()) !== JSON.stringify(resultObj)) {
        throw 'jsonpath.SelectorMut.replaceWith'
      }
    }

    done()
  })

  it('jsonpath.select(json: string|object, jsonpath: string)', done => {
    let jsonObj = {
      school: {
        friends: [
          { name: '친구1', age: 20 },
          { name: '친구2', age: 20 },
        ],
      },
      friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
    }

    let ret = [
      { name: '친구3', age: 30 },
      { name: '친구1', age: 20 },
    ]

    let selectAsString = jsonpath.select(
      JSON.stringify(jsonObj),
      '$..friends[0]',
    )
    let selectAsObj = jsonpath.select(jsonObj, '$..friends[0]')

    if (
      JSON.stringify(ret) !== JSON.stringify(selectAsString) ||
      JSON.stringify(ret) !== JSON.stringify(selectAsObj)
    ) {
      throw 'jsonpath.select(json: string|object, jsonpath: string)'
    }

    done()
  })

  it('jsonpath.compile(jsonpath: string)', done => {
    let template = jsonpath.compile('$..friends[0]')

    let jsonObj = {
      school: {
        friends: [
          { name: '친구1', age: 20 },
          { name: '친구2', age: 20 },
        ],
      },
      friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
    }

    let ret = [
      { name: '친구3', age: 30 },
      { name: '친구1', age: 20 },
    ]

    let selectAsString = template(JSON.stringify(jsonObj))
    let selectAsObj = template(jsonObj)

    if (
      JSON.stringify(ret) !== JSON.stringify(selectAsString) ||
      JSON.stringify(ret) !== JSON.stringify(selectAsObj)
    ) {
      throw 'jsonpath.compile(jsonpath: string) 1'
    }

    let jsonObj2 = {
      school: {
        friends: [{ name: 'Millicent Norman' }, { name: 'Vincent Cannon' }],
      },
      friends: [{ age: 30 }, { age: 40 }],
    }

    let ret2 = [{ age: 30 }, { name: 'Millicent Norman' }]

    let selectAsString2 = template(JSON.stringify(jsonObj2))
    let selectAsObj2 = template(jsonObj2)

    if (
      JSON.stringify(ret2) !== JSON.stringify(selectAsString2) ||
      JSON.stringify(ret2) !== JSON.stringify(selectAsObj2)
    ) {
      throw 'jsonpath.compile(jsonpath: string) 2'
    }

    done()
  })

  it('jsonpath.selector(json: string|object)', done => {
    let jsonObj = {
      school: {
        friends: [
          { name: '친구1', age: 20 },
          { name: '친구2', age: 20 },
        ],
      },
      friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
    }

    let ret1 = [
      { name: '친구3', age: 30 },
      { name: '친구1', age: 20 },
    ]

    let ret2 = [{ name: '친구4' }, { name: '친구2', age: 20 }]

    let selector = jsonpath.selector(jsonObj)
    let select1 = selector('$..friends[0]')
    let select2 = selector('$..friends[1]')

    if (
      JSON.stringify(ret1) !== JSON.stringify(select1) ||
      JSON.stringify(ret2) !== JSON.stringify(select2)
    ) {
      throw 'jsonpath.selector(json: string|object)'
    }

    done()
  })

  it('jsonpath.deleteValue(json: string|object, path: string)', done => {
    let jsonObj = {
      school: {
        friends: [
          { name: '친구1', age: 20 },
          { name: '친구2', age: 20 },
        ],
      },
      friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
    }

    let _1 = jsonpath.deleteValue(jsonObj, '$..friends[0]')
    let result = jsonpath.deleteValue(_1, '$..friends[1]')

    if (
      JSON.stringify(result) ===
      JSON.stringify({
        school: { friends: [null, null] },
        friends: [null, null],
      })
    ) {
      done()
    }
  })

  it('jsonpath.replaceWith(json: string|object, path: string, fun: function(json: object) => json: object', done => {
    let jsonObj = {
      school: {
        friends: [
          { name: '친구1', age: 20 },
          { name: '친구2', age: 20 },
        ],
      },
      friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
    }

    let result = jsonpath.replaceWith(jsonObj, '$..friends[0]', v => {
      v.age = v.age * 2
      return v
    })

    if (
      JSON.stringify(result) ===
      JSON.stringify({
        school: {
          friends: [
            { name: '친구1', age: 40 },
            { name: '친구2', age: 20 },
          ],
        },
        friends: [{ name: '친구3', age: 60 }, { name: '친구4' }],
      })
    ) {
      done()
    }
  })
})

describe('ISSUE test', () => {
  it('Results do not match other implementations #6', done => {
    let result = jsonpath.select(['first', 'second'], '$[:]')
    if (JSON.stringify(result) === JSON.stringify(['first', 'second'])) {
      done()
    }
  })

  it('Invalid wildcard filter results #7', done => {
    function select(json, expected, paths) {
      for (var i = 0; i < paths.length; i++) {
        let result = jsonpath.select(json, paths[i])
        if (JSON.stringify(result) !== JSON.stringify(expected)) {
          throw Error('Error: ' + paths[i])
        }
      }
    }

    select(
      ['string', 42, { key: 'value' }, [0, 1]],
      ['string', 42, { key: 'value' }, [0, 1]],
      ['$.*', '$[*]'],
    )

    select(
      ['string', 42, { key: 'value' }, [0, 1]],
      ['string', 42, { key: 'value' }, [0, 1], 'value', 0, 1],
      ['$..*', '$..[*]'],
    )

    select(
      ['string', 42, { key: 'value' }, [0, 1]],
      ['value', 0, 1],
      ['$.*.*', '$[*].*', '$.*[*]', '$[*][*]'],
    )

    done()
  })

  it('Failure to match "$..[\'$ref\']", "$..[\'ref\']", but succeeds on "$..ref" #43', done => {
    function select(json, expected, paths) {
      for (var i = 0; i < paths.length; i++) {
        let result = jsonpath.select(json, paths[i])
        if (JSON.stringify(result) !== JSON.stringify(expected)) {
          throw Error('Error: ' + paths[i] + ', ' + result)
        }
      }
    }

    select(
      {
        Junk1:
          "This is a test to illustrate use of '$' in the attr for the expression $..['$ref'] ",
        $ref: 'Match Root',
        Subset1: [{ Junk2: 'Data...', $ref: 'Match Subset1' }],
        hierachy1: {
          'hierachy2.1': {
            'hierachy2.1.1': { $ref: 'Match 2.1.1' },
            'hierachy2.1.2': { ref: 'Match 2.1.2' },
            'hierachy2.1.3': { ref: 'No Match 2.1.3' },
            'hierachy2.1.4': { $ref: 'Match 2.1.4' },
            'hierachy2.1.5': { ref: 'No Match 2.1.5' },
          },
          'hierachy2.2': {
            'hierachy2.2.1': { ref: 'No Match 2.2.1' },
            'hierachy2.2.2': { $ref: 'Match 2.2.2' },
            'hierachy2.2.3': { ref: 'No Match 2.2.3' },
            'hierachy2.2.4': { ref: 'No Match 2.2.5' },
            'hierachy2.2.5': { $ref: 'Match 2.2.5' },
          },
          'hierachy2.3': {
            'hierachy2.3.1': { ref: 'No Match 2.3.1' },
            'hierachy2.3.2': { ref: 'No Match 2.3.2' },
            'hierachy2.3.3': { ref: 'No Match 2.3.3' },
            'hierachy2.3.4': { ref: 'No Match 2.3.4' },
            'hierachy2.3.5': { ref: 'No Match 2.3.5' },
            'hierachy2.3.6': {
              'hierachy2.3.6.1': { $ref: 'Match 2.3.6.1' },
              'hierachy2.3.6.2': { ref: 'No Match 2.3.6.2' },
              'hierachy2.3.6.3': { ref: 'No Match 2.3.6.3' },
              'hierachy2.3.6.4': { ref: 'No Match 2.3.6.4' },
              'hierachy2.3.6.5': { ref: 'No Match 2.3.6.5' },
            },
          },
        },
      },
      [
        'Match Root',
        'Match Subset1',
        'Match 2.1.1',
        'Match 2.1.4',
        'Match 2.2.2',
        'Match 2.2.5',
        'Match 2.3.6.1',
      ],
      ['$..$ref', "$..['$ref']"],
    )

    done()
  })
})
