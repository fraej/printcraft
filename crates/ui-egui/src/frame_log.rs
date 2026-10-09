//! Slow-frame log: frames whose logic and layout take over 20 ms, with where the time went.
//! Off unless an app turns it on (`PrintCraftApp::frame_log`; the Android app does).

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};

/// A frame's start, when logging (browsers have no `Instant`).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn now(on: bool) -> Option<Instant> {
    on.then(Instant::now)
}
#[cfg(target_arch = "wasm32")]
pub(crate) fn now(_on: bool) -> Option<()> {
    None
}

#[derive(Default)]
pub(crate) struct FrameClock {
    #[cfg(not(target_arch = "wasm32"))]
    logic: Option<(Duration, Duration, usize)>,
}

#[cfg(not(target_arch = "wasm32"))]
impl FrameClock {
    /// `logic` ran from `start`; turning renders into textures (`uploaded` bytes) from `receive`.
    pub(crate) fn logic(&mut self, start: Option<Instant>, receive: Option<Instant>, uploaded: usize) {
        if let (Some(s), Some(r)) = (start, receive) {
            self.logic = Some((s.elapsed(), r.elapsed(), uploaded));
        }
    }

    /// `ui` ran from `start`: log the frame if it was slow.
    pub(crate) fn ui(&mut self, start: Option<Instant>) {
        let Some(start) = start else { return };
        let ui = start.elapsed();
        let (logic, receive, bytes) = self.logic.take().unwrap_or_default();
        let total = logic + ui;
        if total > Duration::from_millis(20) {
            log::info!(
                "slow frame {:.1} ms: logic {:.1} ms (textures {:.1} ms, {} KB), layout {:.1} ms",
                total.as_secs_f64() * 1e3,
                logic.as_secs_f64() * 1e3,
                receive.as_secs_f64() * 1e3,
                bytes / 1024,
                ui.as_secs_f64() * 1e3
            );
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl FrameClock {
    pub(crate) fn logic(&mut self, _: Option<()>, _: Option<()>, _: usize) {}
    pub(crate) fn ui(&mut self, _: Option<()>) {}
}
