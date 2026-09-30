// MAPPS-970: auto-hiding scrollbars, ported line for line from bunyip's app.js (BUNYIP-848).
// A same-origin file because the CSP refuses inline script; it runs before and apart from WASM.
(function () {
  // Thumbs rest hidden under html[data-scrollbars="auto"] and show while their container
  // is [data-scrollbar-active]: on scroll, while a mouse or pen is in the 14px zone, and for a drag.
  var SCROLLBAR_IDLE_MS = 6000;
  var SCROLLBAR_ZONE_PX = 14;
  var sbTimers = new WeakMap();
  var sbHover = null;
  var sbDrag = null;
  var sbLast = null;
  var sbFrame = 0;
  var sbRoot = document.documentElement;
  sbRoot.setAttribute('data-scrollbars', 'auto');

  // Show now, hide SCROLLBAR_IDLE_MS after the last activity unless the pointer still holds it.
  function sbWake(el) {
    if (!el.hasAttribute('data-scrollbar-active')) el.setAttribute('data-scrollbar-active', '');
    clearTimeout(sbTimers.get(el));
    sbTimers.set(
      el,
      setTimeout(function () {
        sbTimers.delete(el);
        if (el !== sbHover && el !== sbDrag) el.removeAttribute('data-scrollbar-active');
      }, SCROLLBAR_IDLE_MS)
    );
  }

  // The innermost scroll container whose right or bottom scrollbar strip holds (x, y), else null.
  function sbZoneOwner(target, x, y) {
    for (var el = target; el && el.nodeType === 1; el = el.parentElement) {
      var root = el === sbRoot;
      var r = root
        ? { left: 0, top: 0, right: window.innerWidth, bottom: window.innerHeight }
        : el.getBoundingClientRect();
      var vbar = Math.max(root ? r.right - el.clientWidth : el.offsetWidth - el.clientWidth, SCROLLBAR_ZONE_PX);
      var hbar = Math.max(root ? r.bottom - el.clientHeight : el.offsetHeight - el.clientHeight, SCROLLBAR_ZONE_PX);
      var inV = x >= r.right - vbar && x < r.right && y >= r.top && y < r.bottom;
      var inH = y >= r.bottom - hbar && y < r.bottom && x >= r.left && x < r.right;
      if (!inV && !inH) continue;
      var cs = getComputedStyle(el);
      if (inV && el.scrollHeight > el.clientHeight && sbScrolls(cs.overflowY, root)) return el;
      if (inH && el.scrollWidth > el.clientWidth && sbScrolls(cs.overflowX, root)) return el;
    }
    return null;
  }

  // The viewport scrolls unless <html> clips; any other element only when its overflow says so.
  function sbScrolls(overflow, root) {
    return root ? overflow !== 'hidden' && overflow !== 'clip' : /auto|scroll|overlay/.test(overflow);
  }

  function sbSetHover(owner) {
    if (owner === sbHover) return;
    var left = sbHover;
    sbHover = owner;
    // Leaving the zone starts the idle countdown; entering it shows the thumb.
    if (left) sbWake(left);
    if (owner) sbWake(owner);
  }

  function sbRelease() {
    var el = sbDrag;
    sbDrag = null;
    if (el) sbWake(el);
  }

  document.addEventListener(
    'scroll',
    function (e) {
      var el = e.target === document ? sbRoot : e.target;
      if (el && el.nodeType === 1) sbWake(el);
    },
    { capture: true, passive: true }
  );

  // One zone test per animation frame, on the latest position; touch has no bar to reach for.
  document.addEventListener(
    'pointermove',
    function (e) {
      if (e.pointerType === 'touch') return;
      sbLast = e;
      if (sbFrame) return;
      sbFrame = requestAnimationFrame(function () {
        sbFrame = 0;
        var ev = sbLast;
        sbLast = null;
        if (!ev) return;
        // A release outside the window never reaches pointerup.
        if (sbDrag && ev.buttons === 0) sbRelease();
        sbSetHover(sbZoneOwner(ev.target, ev.clientX, ev.clientY));
      });
    },
    { passive: true }
  );

  // The pointer left the window (e.g. onto a neighboring one), so nothing holds the thumb any more.
  document.addEventListener(
    'pointerout',
    function (e) {
      if (e.relatedTarget) return;
      sbLast = null;
      sbSetHover(null);
    },
    { passive: true }
  );

  document.addEventListener(
    'pointerdown',
    function (e) {
      if (e.pointerType === 'touch') return;
      var owner = sbZoneOwner(e.target, e.clientX, e.clientY);
      if (!owner) return;
      sbDrag = owner;
      sbWake(owner);
    },
    { capture: true, passive: true }
  );
  document.addEventListener('pointerup', sbRelease, { capture: true, passive: true });
  document.addEventListener('pointercancel', sbRelease, { capture: true, passive: true });
})();
