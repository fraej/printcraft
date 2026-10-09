//! PrintCraft for Android: the same shell as the desktop app, laid out for a phone
//! (`printcraft-ui-egui`'s compact layout), with Android's ways in and out of files.
//!
//! Build and install with `cargo xtask android [--release] [--install]` (cargo-apk, the NDK and
//! the SDK; see README §Android). Android loads `libprintcraft_android.so` through
//! NativeActivity and calls [`android_main`].
//!
//! Files: "Open with PrintCraft" and Share ▸ PrintCraft open the document they carry; inside
//! the app, PrintCraft's own file browser opens and saves (Android has no file dialog a native
//! app can wait on). With All files access (asked for the first time the browser opens) it sees
//! the whole shared storage and saves in place.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(target_os = "android")]
mod android;
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
mod storage;

#[cfg(target_os = "android")]
pub use android::android_main;
