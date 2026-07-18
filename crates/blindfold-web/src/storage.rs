//! The one `localStorage` seam, shared by the modules that persist a preference.
//!
//! Both [`crate::rating`] and [`crate::settings`] keep a value across reloads, and
//! the fallible steps to reach it — `window().local_storage()`, then a `get_item`
//! or `set_item` that can itself fail — are the same for each. So the whole read and
//! the whole write live here once, as [`read`]/[`write`], rather than being
//! open-coded in both. Callers deal only in `Option<String>` and a key.

/// The browser's `localStorage`, or `None` when it is unavailable (private mode,
/// storage disabled).
fn local() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// The stored string for `key`, or `None` when it is absent or `localStorage` is
/// unavailable. Callers treat `None` as "no saved value", a graceful degradation
/// rather than a failure worth surfacing.
pub fn read(key: &str) -> Option<String> {
    local()?.get_item(key).ok().flatten()
}

/// Persist `value` under `key`. Silent if `localStorage` is unavailable: the value
/// then simply does not survive a reload, a graceful degradation rather than a
/// failure worth interrupting the user over.
pub fn write(key: &str, value: &str) {
    if let Some(s) = local() {
        let _ = s.set_item(key, value);
    }
}
