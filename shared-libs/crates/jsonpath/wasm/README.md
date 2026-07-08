# jsonpath-wasm

[![Build Status](https://travis-ci.org/freestrings/jsonpath.svg?branch=master)](https://travis-ci.org/freestrings/jsonpath)

It is Webassembly version of [jsonpath_lib](https://github.com/freestrings/jsonpath) that is [JsonPath](https://goessner.net/articles/JsonPath/) engine written in Rust.

## APIs

<details><summary><b>npm package</b></summary>

```javascript
// browser
import * as jsonpath from 'jsonpath-wasm'
// NodeJs
const jsonpath = require('jsonpath-wasm')
```

</details>

<details><summary><b>Javascript - jsonpath.Selector class</b></summary>

`wasm-bindgen` 리턴 타입 제약 때문에 빌더 패턴은 지원하지 않는다.

It does not support `builder-pattern` due to the `return type` restriction of `wasm-bindgen`.

```javascript
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

let selector = new jsonpath.Selector()
selector.path('$..friends[0]')
selector.value(jsonObj)

let retObj = selector.select()

console.log(JSON.stringify(ret) == JSON.stringify(retObj))

// => true
```

</details>

<details><summary><b>Javascript - jsonpath.SelectorMut class</b></summary>

빌더 패턴 제약은 `Selector class`와 동일하다.

```javascript
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
  console.log(JSON.stringify(selector.take()) !== JSON.stringify(resultObj))

  // => true
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
  console.log(JSON.stringify(selector.take()) !== JSON.stringify(resultObj))

  // => true
}
```

</details>

<details><summary><b>Javascript - jsonpath.select(json: string|object, jsonpath: string)</b></summary>

```javascript
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

let selectAsString = jsonpath.select(JSON.stringify(jsonObj), '$..friends[0]')
let selectAsObj = jsonpath.select(jsonObj, '$..friends[0]')

console.log(JSON.stringify(ret) == JSON.stringify(selectAsString), JSON.stringify(ret) == JSON.stringify(selectAsObj))

// => true, true
```

</details>

<details><summary><b>Javascript - jsonpath.compile(jsonpath: string)</b></summary>

```javascript
let error = jsonpath.compile('')
console.log(typeof error, error) //string 'path error'

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

console.log(JSON.stringify(ret) == JSON.stringify(selectAsString), JSON.stringify(ret) == JSON.stringify(selectAsObj))

// => true, true

let jsonObj2 = {
  school: {
    friends: [{ name: 'Millicent Norman' }, { name: 'Vincent Cannon' }],
  },
  friends: [{ age: 30 }, { age: 40 }],
}

let ret2 = [{ age: 30 }, { name: 'Millicent Norman' }]

let selectAsString2 = template(JSON.stringify(jsonObj2))
let selectAsObj2 = template(jsonObj2)

console.log(JSON.stringify(ret2) == JSON.stringify(selectAsString2), JSON.stringify(ret2) == JSON.stringify(selectAsObj2))

// => true, true
```

</details>

<details><summary><b>Javascript - jsonpath.selector(json: string|object)</b></summary>

```javascript
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
// or as json string
// let selector = jsonpath.selector(JSON.stringify(jsonObj));

let select1 = selector('$..friends[0]')
let select2 = selector('$..friends[1]')

console.log(JSON.stringify(ret1) == JSON.stringify(select1), JSON.stringify(ret2) == JSON.stringify(select2))

// => true, true
```

</details>

<details><summary><b>Javascript - jsonpath.deleteValue(json: string|object, path: string)</b></summary>

```javascript
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

console.log(
  JSON.stringify(result) !==
    JSON.stringify({
      school: { friends: [null, null] },
      friends: [null, null],
    }),
)

// => true
```

</details>

<details><summary><b>Javascript - jsonpath.replaceWith(json: string|object, path: string, fun: function(json: object) => json: object</b></summary>

```javascript
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

console.log(
  JSON.stringify(result) ===
    JSON.stringify({
      school: {
        friends: [
          { name: '친구1', age: 40 },
          { name: '친구2', age: 20 },
        ],
      },
      friends: [{ name: '친구3', age: 60 }, { name: '친구4' }],
    }),
)

// => true
```

</details>

[Javascript - Other Examples](https://github.com/freestrings/jsonpath/wiki/Javascript-examples)
