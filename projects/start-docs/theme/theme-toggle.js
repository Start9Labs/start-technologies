;(function () {
  // --- THEME LOGIC ---
  const LIGHT = 'light'
  const DARK = 'ayu'
  const STORAGE_KEY = 'mdbook-theme'

  function getStoredTheme() {
    return (
      localStorage.getItem(STORAGE_KEY) ||
      (window.matchMedia('(prefers-color-scheme: dark)').matches ? DARK : LIGHT)
    )
  }

  function applyTheme(theme) {
    document.documentElement.classList.remove(LIGHT, DARK)
    document.documentElement.classList.add(theme)
    localStorage.setItem(STORAGE_KEY, theme)
  }

  function switchTheme() {
    const next = getStoredTheme() === LIGHT ? DARK : LIGHT
    applyTheme(next)
  }

  function insertToggleButton() {
    const menuBar = document.getElementById('menu-bar')
    if (!menuBar) return
    const toggleButton = document.createElement('button')
    toggleButton.id = 'custom-theme-toggle'
    toggleButton.type = 'button'
    toggleButton.innerHTML =
      '<i class="fa fa-sun-o light-mode" aria-hidden="true"></i><i class="fa fa-moon-o dark-mode" aria-hidden="true"></i>'
    toggleButton.addEventListener('click', switchTheme)

    const rightButtons = menuBar.querySelector('.right-buttons')
    ;(rightButtons || menuBar).prepend(toggleButton)
  }

  // --- EXTERNAL LINK LOGIC ---
  function patchExternalLinks() {
    document.querySelectorAll('a[href^="http"]').forEach(link => {
      if (!link.href.startsWith(window.location.origin)) {
        link.setAttribute('target', '_blank')
        link.setAttribute('rel', 'noopener noreferrer')
      }
    })
  }

  // --- INIT ---
  document.addEventListener('DOMContentLoaded', function () {
    applyTheme(getStoredTheme())
    insertToggleButton()
    patchExternalLinks()
  })
})()
