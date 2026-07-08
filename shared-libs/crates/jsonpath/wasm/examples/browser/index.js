import * as jsonpath from 'jsonpath-wasm'

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

const path = '$..friends[0]'

let ret1 = jsonpath.select(jsonObj, path)
let ret2 = jsonpath.compile(path)(jsonObj)
let ret3 = jsonpath.selector(jsonObj)(path)

let selector = new jsonpath.Selector()
selector.path(path)
selector.value(jsonObj)
let ret4 = selector.select()

console.log(
  JSON.stringify(ret) == JSON.stringify(ret1),
  JSON.stringify(ret) == JSON.stringify(ret2),
  JSON.stringify(ret) == JSON.stringify(ret3),
  JSON.stringify(ret) == JSON.stringify(ret4),
)
