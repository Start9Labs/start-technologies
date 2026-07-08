import * as jpw from 'jsonpath-wasm'
import * as jp from 'jsonpath/jsonpath.js'

function msg(msg) {
  console.log(msg)
  let div = document.createElement('div')
  div.innerText = msg
  document.body.appendChild(div)
}

function run(message, iter, cb) {
  return new Promise(function (resolve, _reject) {
    let d = Date.now()
    for (let i = 0; i < iter; i++) {
      cb()
    }
    msg([message, Date.now() - d].join(', '))
    setTimeout(resolve, 0)
  })
}

let json = {
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

let path = '$..book[?(@.price<30 && @.category=="fiction")]'
let template = jpw.compile(path)
let selector = jpw.selector(json)

let iterCount = 2000

run('jsonpath', iterCount, function () {
  jp.query(json, path)
})
  .then(function () {
    return run('jsonpath-wasm- selector', iterCount, function () {
      selector(path)
    })
  })
  .then(function () {
    return run('jsonpath-wasm- compile', iterCount, function () {
      template(json)
    })
  })
  .then(function () {
    return run('jsonpath-wasm- select', iterCount, function () {
      jpw.select(json, path)
    })
  })
  .finally(function () {})
