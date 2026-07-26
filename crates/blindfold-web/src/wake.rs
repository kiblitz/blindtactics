//! Screen Wake Lock — keep a phone from dimming and sleeping while the app is open.
//!
//! A blindfold trainer in voice mode is used hands-free and eyes-free: the user never
//! touches the screen for whole minutes at a time, so the phone's idle timer would dim
//! and lock it mid-puzzle. The [Screen Wake Lock API][mdn] holds it awake. The app takes
//! the lock once at startup and holds it for the whole session (the user's call — "always
//! while open" over a per-mode or opt-in trigger).
//!
//! Two browser facts shape the wrapper, both handled in the inline JS so the caller only
//! ever [`request`]s once:
//!
//! - **A wake lock is released automatically whenever the page is hidden** (tab switch,
//!   app backgrounded, screen locked by the hardware button). It does *not* come back on
//!   its own, so the module re-acquires on `visibilitychange` for the life of the page.
//! - **It requires a secure context and a visible document.** We serve over HTTPS, and the
//!   re-acquire is gated on `visibilityState === "visible"`, so the request is only ever
//!   made when it can succeed.
//!
//! Everything degrades to nothing: a browser without `navigator.wakeLock` (older Safari,
//! any desktop that lacks it) simply never holds a lock, and the app is unaffected — the
//! screen behaves exactly as it did before, which on a desktop is no worse. Like
//! [`crate::speech`] and [`crate::recognition`], this is a thin browser seam with no logic
//! worth a native test.
//!
//! [mdn]: https://developer.mozilla.org/en-US/docs/Web/API/Screen_Wake_Lock_API

use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
// The active WakeLockSentinel, or null when we hold no lock — never acquired yet, released
// by the browser on hide, or a request that failed. The visibilitychange listener is
// installed once, lazily, on the first request.
let _sentinel = null;
let _installed = false;

// Acquire the lock if we do not already hold it and the page is visible — the conditions
// the API needs. Any failure (no support, a refused request) leaves _sentinel null and
// simply means the screen keeps its normal idle behaviour.
async function bft_wake_acquire() {
  if (_sentinel) return;
  if (typeof navigator === "undefined" || !("wakeLock" in navigator)) return;
  if (typeof document !== "undefined" && document.visibilityState !== "visible") return;
  try {
    _sentinel = await navigator.wakeLock.request("screen");
    // The browser drops the lock when the page hides; clear our handle so a later
    // visibilitychange re-acquires rather than thinking it still holds one.
    _sentinel.addEventListener("release", () => { _sentinel = null; });
  } catch (_) {
    _sentinel = null;
  }
}

export function bft_wake_request() {
  if (!_installed && typeof document !== "undefined") {
    _installed = true;
    // Re-acquire when the page becomes visible again: the browser releases the lock on
    // hide and never restores it, so without this the screen would sleep after the first
    // tab switch for the rest of the session.
    document.addEventListener("visibilitychange", () => {
      if (document.visibilityState === "visible") bft_wake_acquire();
    });
  }
  bft_wake_acquire();
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = bft_wake_request)]
    fn request_js();
}

/// Ask the browser to keep the screen awake, and keep it awake for the rest of the
/// session — re-acquiring on its own each time the page returns to the foreground (the
/// browser drops the lock while hidden). Idempotent; a harmless no-op where unsupported.
pub fn request() {
    request_js();
}
