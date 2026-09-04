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
  // MAPPS-659: must equal THEME_KEY in src/hooks/theme.rs, which is where the
  // preference is actually written. This read used `localStorage.theme`, a key
  // nothing writes, so the guard always fell through to the OS match and an
  // explicit choice that disagreed with the OS painted the wrong first frame.
  // `scripts/check-theme-storage-key.sh` fails if the two names drift again.
  var THEME_KEY = 'mokosh_theme';

  function prefersDark() {
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
  }

  // Mirrors Theme::parse + resolved_is_dark in src/hooks/theme.rs: only the
  // two explicit values decide by themselves, and everything else ("system",
  // absent, empty, unparseable) follows the OS.
  function resolveDark(stored) {
    if (stored === 'dark') {
      return true;
    }
    if (stored === 'light') {
      return false;
    }
    return prefersDark();
  }

  try {
    var stored = localStorage.getItem(THEME_KEY);
    document.documentElement.classList.toggle('dark', resolveDark(stored));
  } catch (e) {
    // Some browsers refuse localStorage access on first paint (private
    // mode, third-party-cookie block, ITP). Silently fall back to the
    // OS-provided prefers-color-scheme match; a bad readback here would
    // otherwise throw uncaught and break the head parse.
    try {
      document.documentElement.classList.toggle('dark', prefersDark());
      // After the correction, never before it: a console that throws must not
      // cost the fallback paint. Silent here would leave a user whose stored
      // choice is unreachable with no way to tell why the mode looks wrong.
      console.warn('theme-init: localStorage unreadable, using the OS match', e);
    } catch (_) {
      /* give up quietly; WASM app will apply the theme once it boots */
    }
  }
})();
