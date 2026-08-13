/**
 * Rungu Embed Widget Loader
 *
 * Usage (on any website):
 *   <script src="https://your-rungu-host/embed.js" data-rungu-slug="my-project"></script>
 *
 * The script injects an iframe pointing to /embed/{slug} and auto-resizes
 * its height based on postMessage events from the embedded board.
 *
 * Optional attributes:
 *   data-rungu-slug  — project slug (required)
 *   data-rungu-width — iframe width, default "100%"
 *   data-rungu-height — initial height, default "400px"
 */
(function () {
  'use strict';

  // Find the currently executing script tag.
  // We can't rely on a fixed id, so we scan for the last script with data-rungu-slug.
  var scripts = document.getElementsByTagName('script');
  var currentScript = null;
  for (var i = 0; i < scripts.length; i++) {
    if (scripts[i].hasAttribute('data-rungu-slug')) {
      currentScript = scripts[i];
    }
  }
  if (!currentScript) return;

  var slug = currentScript.getAttribute('data-rungu-slug');
  if (!slug) {
    console.error('[Rungu] Missing data-rungu-slug attribute on script tag');
    return;
  }

  // Resolve the Rungu server origin from the script's own src URL.
  var src = currentScript.getAttribute('src') || '';
  var origin;
  try {
    origin = new URL(src, window.location.href).origin;
  } catch (e) {
    origin = window.location.origin;
  }

  var width = currentScript.getAttribute('data-rungu-width') || '100%';
  var initialHeight = currentScript.getAttribute('data-rungu-height') || '400px';

  // Create the iframe.
  var iframe = document.createElement('iframe');
  iframe.setAttribute('src', origin + '/embed/' + encodeURIComponent(slug));
  iframe.setAttribute('width', width);
  iframe.setAttribute('height', initialHeight);
  iframe.setAttribute('frameborder', '0');
  iframe.setAttribute('scrolling', 'no');
  iframe.setAttribute('title', 'Feedback Board');
  iframe.setAttribute('loading', 'lazy');
  iframe.style.width = width;
  iframe.style.height = initialHeight;
  iframe.style.border = 'none';
  iframe.style.minHeight = '200px';
  iframe.style.display = 'block';
  iframe.style.overflow = 'hidden';

  // Insert the iframe right after the script tag.
  if (currentScript.parentNode) {
    currentScript.parentNode.insertBefore(iframe, currentScript.nextSibling);
  }

  // Auto-resize iframe height based on postMessage from the embed page.
  window.addEventListener('message', function (event) {
    // Accept messages from any origin (the embed board is read-only and safe).
    var data = event.data;
    if (!data || !data.runguEmbed || typeof data.height !== 'number') return;
    iframe.style.height = Math.ceil(data.height) + 'px';
    iframe.setAttribute('height', Math.ceil(data.height) + 'px');
  });
})();
