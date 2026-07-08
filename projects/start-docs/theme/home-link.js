// Inject "← All Docs" link at top of sidebar, above the TOC
document.addEventListener('DOMContentLoaded', function () {
  var scrollbox = document.querySelector('.sidebar-scrollbox')
  if (!scrollbox) return

  var link = document.createElement('a')
  link.href = '/'
  link.innerHTML = '&#8592; All Docs'
  link.style.cssText =
    'display:block;padding:0.6em 1.2em;color:var(--sidebar-fg);text-decoration:none;font-size:0.85em;opacity:0.7;border-bottom:1px solid var(--sidebar-spacer);'
  link.onmouseenter = function () {
    link.style.opacity = '1'
  }
  link.onmouseleave = function () {
    link.style.opacity = '0.7'
  }

  scrollbox.insertBefore(link, scrollbox.firstChild)
})
