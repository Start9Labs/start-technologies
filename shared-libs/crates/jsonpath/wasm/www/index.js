import * as jsonpath from 'jsonpath-wasm'

function getTextarea() {
  return document.querySelector('#json-example')
}

function getJsonpathInput() {
  return document.querySelector('#jsonpath-input')
}

function getReadBtn() {
  return document.querySelector('#read-json')
}

function getReadResult() {
  return document.querySelector('#read-result')
}

function getLinks() {
  return document.querySelectorAll('.path>a')
}

function initData(url) {
  return fetch(url)
    .then(res => res.text())
    .then(jsonStr => (getTextarea().value = jsonStr))
    .catch(console.error)
}

function initEvent() {
  getJsonpathInput().onkeyup = function (e) {
    var charCode = typeof e.which === 'number' ? e.which : e.keyCode
    if (charCode == 13) {
      read()
    }
  }

  getReadBtn().onclick = function () {
    read()
  }

  getLinks().forEach(function (anchor) {
    anchor.href = '#' + encodeURIComponent(anchor.textContent)
  })

  function read() {
    let ret = jsonpath.select(getTextarea().value, getJsonpathInput().value)
    if (typeof ret === 'string') {
      getReadResult().innerText = ret
    } else {
      getReadResult().innerText = JSON.stringify(ret, null, 2)
    }
  }
}

function readPath() {
  let query = location.href.substring(location.href.indexOf('#') + 1)
  let path = decodeURIComponent(query)
  getJsonpathInput().value = path
  forceClick(getReadBtn())
}

function readPathParam() {
  if (location.href.indexOf('#') > -1) {
    readPath()
  }
}

function forceClick(ctrl) {
  let doc = ctrl.ownerDocument
  let event = doc.createEvent('MouseEvents')
  event.initEvent('click', true, true)
  event.synthetic = true
  ctrl.dispatchEvent(event, true)
}

window.onpopstate = readPath

initData('data/example.json').then(initEvent).then(readPathParam)
