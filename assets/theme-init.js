// MAPPS-356: FOUC-prevention theme applier. Runs synchronously in <head>
// before the WASM app boots so a dark-mode user does not see a light-mode
// flash on first paint. Same logic as the previous inline <script> block
// in index.html; extracted to an external same-origin file so it satisfies
// the SPA's CSP directive `script-src 'self' 'wasm-unsafe-eval'` (inline
// execution is refused without a nonce or hash, which the server-side CSP
// header does not currently supply).
//
// The WASM app owns the theme after boot via `use_apply_theme` /
// `use_theme_sync`; this block only decides the initial state so the very
// first frame paints in the user's preferred mode.
(function () {
  try {
    var stored = localStorage.theme;
    var prefersDark =
      !('theme' in localStorage) &&
      window.matchMedia('(prefers-color-scheme: dark)').matches;
    if (stored === 'dark' || prefersDark) {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
  } catch (e) {
    // Some browsers refuse localStorage access on first paint (private
    // mode, third-party-cookie block, ITP). Silently fall back to the
    // OS-provided prefers-color-scheme match; a bad readback here would
    // otherwise throw uncaught and break the head parse.
    try {
      if (window.matchMedia('(prefers-color-scheme: dark)').matches) {
        document.documentElement.classList.add('dark');
      }
    } catch (_) {
      /* give up quietly; WASM app will apply the theme once it boots */
    }
  }
})();
