/**
 * Whether `container` directly owns a tab with the given name.
 *
 * Scoped to `:scope > .mdbook-tabs > .mdbook-tab` so a *nested* tabs container
 * (e.g. `{{#tabs}}` inside a tab's content) doesn't count toward its parent.
 *
 * @param {Element} container
 * @param {string} name
 * @returns {boolean}
 */
const containerOwnsTab = (container, name) => {
  for (const tab of container.querySelectorAll(
    ':scope > .mdbook-tabs > .mdbook-tab',
  )) {
    if (tab instanceof HTMLElement && tab.dataset.tabname === name) {
      return true
    }
  }
  return false
}

/**
 * Switch `container` to the tab named `name`.
 *
 * No-op (returns false) if the container has no such tab. This is important for
 * `global` tab groups: a selection made on another page where the group has a
 * different set of tabs must NOT blank this block or activate the wrong tab —
 * instead we leave the pre-rendered first tab (or current selection) intact.
 *
 * @param {Element} container
 * @param {string} name
 * @returns {boolean} whether the switch was applied
 */
const changeTab = (container, name) => {
  if (!containerOwnsTab(container, name)) {
    return false
  }

  for (const child of container.children) {
    if (!(child instanceof HTMLElement)) {
      continue
    }

    if (child.classList.contains('mdbook-tabs')) {
      for (const tab of child.children) {
        if (tab instanceof HTMLElement) {
          tab.classList.toggle('active', tab.dataset.tabname === name)
        }
      }
    } else if (child.classList.contains('mdbook-tab-content')) {
      child.classList.toggle('hidden', child.dataset.tabname !== name)
    }
  }

  return true
}

/**
 * Update URL query params to reflect current tab selections.
 * Only includes params for tab globals present on the page.
 */
const updateTabUrlParams = () => {
  const params = new URLSearchParams(window.location.search)
  const seen = new Set()

  document
    .querySelectorAll('.mdbook-tabs-container[data-tabglobal]')
    .forEach(container => {
      const global = container.dataset.tabglobal
      if (!global || seen.has(global)) return
      seen.add(global)

      const active = container.querySelector(
        ':scope > .mdbook-tabs > .mdbook-tab.active',
      )
      if (active && active.dataset.tabname) {
        params.set(global, active.dataset.tabname)
      }
    })

  const search = params.toString() ? '?' + params.toString() : ''
  history.replaceState(
    null,
    '',
    window.location.pathname + search + window.location.hash,
  )
}

document.addEventListener('DOMContentLoaded', () => {
  for (const tab of document.querySelectorAll('.mdbook-tab')) {
    tab.addEventListener('click', () => {
      if (
        !(tab instanceof HTMLElement) ||
        !tab.parentElement ||
        !tab.parentElement.parentElement
      ) {
        return
      }

      const container = tab.parentElement.parentElement
      const name = tab.dataset.tabname
      const global = container.dataset.tabglobal

      changeTab(container, name)

      if (global) {
        localStorage.setItem(`mdbook-tabs-${global}`, name)

        for (const globalContainer of document.querySelectorAll(
          `.mdbook-tabs-container[data-tabglobal="${global}"]`,
        )) {
          changeTab(globalContainer, name)
        }
      }

      updateTabUrlParams()
    })
  }

  const containers = document.querySelectorAll(
    '.mdbook-tabs-container[data-tabglobal]',
  )

  // Phase 1: restore from localStorage. changeTab() ignores containers that
  // don't own the stored tab, so a selection made on another page (where the
  // group has different tabs) won't blank or mis-select this one.
  for (const container of containers) {
    const name = localStorage.getItem(
      `mdbook-tabs-${container.dataset.tabglobal}`,
    )
    if (name) {
      changeTab(container, name)
    }
  }

  // Phase 2: URL params override localStorage (used by deep links such as
  // `/start-os/trust-ca.html?platform=Mac`). Only persist a param that
  // actually applied to at least one container on this page.
  for (const [key, value] of new URLSearchParams(window.location.search)) {
    if (!/^[\w-]+$/.test(key)) {
      continue
    }
    let applied = false
    for (const container of document.querySelectorAll(
      `.mdbook-tabs-container[data-tabglobal="${key}"]`,
    )) {
      if (changeTab(container, value)) {
        applied = true
      }
    }
    if (applied) {
      localStorage.setItem(`mdbook-tabs-${key}`, value)
    }
  }

  // Phase 3: reflect the resolved state back into the URL.
  if (document.querySelector('.mdbook-tabs-container[data-tabglobal]')) {
    updateTabUrlParams()
  }
})
