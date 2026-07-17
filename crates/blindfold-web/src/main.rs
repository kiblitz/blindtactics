//! Mounts the app. Everything else is in the library, where it can be tested.

fn main() {
    // Without this a wasm panic surfaces as `RuntimeError: unreachable`, with no
    // message and no line — indistinguishable from a browser bug.
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(blindfold_web::app::App);
}
