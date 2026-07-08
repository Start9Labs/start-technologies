// Transform <div class="yt-video" data-id="VIDEO_ID"></div> into full embeds
document.addEventListener('DOMContentLoaded', function () {
  document.querySelectorAll('.yt-video[data-id]').forEach(function (el) {
    var id = el.getAttribute('data-id')
    var title = el.getAttribute('data-title') || 'YouTube video'
    el.innerHTML =
      '<iframe src="https://www.youtube.com/embed/' +
      id +
      '" ' +
      'title="' +
      title +
      '" ' +
      'allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share" ' +
      'allowfullscreen loading="lazy"></iframe>'
  })
})
