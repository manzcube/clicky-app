//! The background loop that makes clicky feel alive.
//!
//! It does two jobs at ~90Hz:
//!
//!   1. Drags the bead window along behind your cursor.
//!   2. Watches for the *gesture* of selecting text, and when it sees one,
//!      grabs whatever got selected and offers to do something with it.
//!
//! On (2): there is no cross-platform "the user selected something" event. The
//! honest options are polling accessibility APIs (slow, and missing in a lot of
//! apps) or recognising the gesture. We recognise the gesture — a drag of more
//! than a few pixels, or a double/triple click — and only then pay for a
//! clipboard round-trip. That means clicky does *not* read your screen
//! continuously; it reads exactly once, right after you highlight something.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use device_query::{DeviceQuery, DeviceState};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

use crate::{selection, settings::Store};

/// Minimum drag in pixels before we believe it was a selection and not a click.
const DRAG_THRESHOLD: f64 = 9.0; // logical points, scaled below
/// Two clicks closer together than this, in the same spot, = word selection.
const DOUBLE_CLICK_MS: u128 = 420;
/// Let the app finish painting its selection before we ask for it.
const SETTLE_MS: u64 = 90;

pub struct Watcher {
    /// Set while a capture is in flight, or while the panel is up.
    pub paused: Arc<AtomicBool>,
    /// Set while the bead is showing an offer. A window that moves between
    /// your mousedown and mouseup swallows the click, so once the bead has
    /// something to say, it stands still until you deal with it.
    pub frozen: Arc<AtomicBool>,
    pub last_offer: Arc<Mutex<String>>,
}

impl Watcher {
    pub fn start(app: AppHandle) -> Self {
        let paused = Arc::new(AtomicBool::new(false));
        let frozen = Arc::new(AtomicBool::new(false));
        let last_offer = Arc::new(Mutex::new(String::new()));

        let w = Self {
            paused: paused.clone(),
            frozen: frozen.clone(),
            last_offer: last_offer.clone(),
        };
        thread::spawn(move || run(app, paused, frozen, last_offer));
        w
    }
}

fn run(
    app: AppHandle,
    paused: Arc<AtomicBool>,
    frozen: Arc<AtomicBool>,
    last_offer: Arc<Mutex<String>>,
) {
    let device = DeviceState::new();
    let frame = Duration::from_millis(11); // ~90Hz

    // Smoothed bead position, so it trails instead of snapping.
    let (mut bx, mut by) = (0.0_f64, 0.0_f64);
    let mut placed = false;

    // Gesture state.
    let mut was_down = false;
    let mut down_at = (0.0_f64, 0.0_f64);
    let mut down_time = Instant::now();
    let mut last_click_at = (0.0_f64, 0.0_f64);
    let mut last_click_time = Instant::now() - Duration::from_secs(10);

    loop {
        thread::sleep(frame);

        let Some(store) = app.try_state::<Store>() else {
            continue;
        };
        let cfg = store.get();

        // Mouse *buttons* come from device_query. Mouse *position* comes from
        // Tauri, because that is what set_position() is measured in — mixing
        // the two is how the bead drifted away on a Retina display.
        let down = *device.get_mouse().button_pressed.get(1).unwrap_or(&false);
        let Ok(cursor) = app.cursor_position() else {
            continue;
        };
        let (mx, my) = (cursor.x, cursor.y);

        let bead = app.get_webview_window("bead");
        let scale = bead
            .as_ref()
            .and_then(|b| b.scale_factor().ok())
            .unwrap_or(1.0);

        // ---- 1. follow the cursor ----------------------------------------
        if cfg.follow_cursor && !frozen.load(Ordering::Relaxed) {
            // Offsets are written in the settings file as points, so they mean
            // the same thing on a Retina screen as on an external monitor.
            let target = (
                mx + cfg.offset_x as f64 * scale,
                my + cfg.offset_y as f64 * scale,
            );
            if !placed {
                bx = target.0;
                by = target.1;
                placed = true;
            } else {
                let e = cfg.follow_easing.clamp(0.01, 1.0);
                bx += (target.0 - bx) * e;
                by += (target.1 - by) * e;
            }

            if let Some(b) = &bead {
                // Skip the move when we're within half a pixel — saves a lot
                // of pointless window-server traffic while the mouse is still.
                if (bx - target.0).abs() > 0.5 || (by - target.1).abs() > 0.5 {
                    let _ =
                        b.set_position(PhysicalPosition::new(bx.round() as i32, by.round() as i32));
                }
            }
        } else if !cfg.follow_cursor {
            placed = false;
        }

        // ---- 2. spot a selection gesture ---------------------------------
        if !cfg.watch_selection || paused.load(Ordering::Relaxed) || frozen.load(Ordering::Relaxed)
        {
            was_down = down;
            continue;
        }

        if down && !was_down {
            down_at = (mx, my);
            down_time = Instant::now();
        }

        if !down && was_down {
            let dist = ((mx - down_at.0).powi(2) + (my - down_at.1).powi(2)).sqrt();
            let held = down_time.elapsed().as_millis();

            let near_last = ((mx - last_click_at.0).powi(2) + (my - last_click_at.1).powi(2))
                .sqrt()
                < 6.0 * scale;
            let is_double = near_last && last_click_time.elapsed().as_millis() < DOUBLE_CLICK_MS;

            last_click_at = (mx, my);
            last_click_time = Instant::now();

            // A drag that took a plausible amount of time, or a double click.
            let looks_like_selection =
                (dist > DRAG_THRESHOLD * scale && held > 60 && held < 12_000) || is_double;

            if looks_like_selection {
                offer(&app, &paused, &last_offer);
            }
        }

        was_down = down;
    }
}

fn offer(app: &AppHandle, paused: &Arc<AtomicBool>, last_offer: &Arc<Mutex<String>>) {
    // Don't react to selections made inside clicky's own panel. An auto-opened
    // panel sitting there unfocused is fair game, though — highlighting
    // something else should just swap out what it's working on.
    if let Some(p) = app.get_webview_window("panel") {
        if p.is_visible().unwrap_or(false) && p.is_focused().unwrap_or(false) {
            return;
        }
    }

    paused.store(true, Ordering::Relaxed);

    let app = app.clone();
    let paused = paused.clone();
    let last_offer = last_offer.clone();

    thread::spawn(move || {
        thread::sleep(Duration::from_millis(SETTLE_MS));

        // The copy keystroke has to be sent from the main thread on macOS.
        let result: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let slot = result.clone();
        let _ = app.run_on_main_thread(move || {
            *slot.lock().unwrap() = Some(selection::capture());
        });

        // run_on_main_thread returns immediately; wait for the slot to fill.
        let mut text = String::new();
        for _ in 0..60 {
            thread::sleep(Duration::from_millis(12));
            if let Some(t) = result.lock().unwrap().take() {
                text = t;
                break;
            }
        }

        paused.store(false, Ordering::Relaxed);

        let text = text.trim().to_string();
        if text.is_empty() || text.chars().count() < 2 {
            let _ = app.emit("offer-cleared", ());
            return;
        }

        // Don't re-offer the same thing over and over.
        {
            let mut last = last_offer.lock().unwrap();
            if *last == text {
                return;
            }
            *last = text.clone();
        }

        if let Some(bead) = app.get_webview_window("bead") {
            // When the bead isn't trailing the cursor it lives offscreen, so
            // bring it to wherever the selection just happened.
            let following = app
                .try_state::<Store>()
                .map(|s| s.get().follow_cursor)
                .unwrap_or(true);
            if !following {
                if let Ok(c) = app.cursor_position() {
                    let scale = bead.scale_factor().unwrap_or(1.0);
                    let _ = bead.set_position(PhysicalPosition::new(
                        (c.x + 18.0 * scale) as i32,
                        (c.y + 18.0 * scale) as i32,
                    ));
                }
            }
            let _ = bead.show();
        }

        let _ = app.emit("offer", text.clone());

        if let Some(store) = app.try_state::<Store>() {
            let cfg = store.get();
            if cfg.auto_open {
                let app2 = app.clone();
                let focus = cfg.auto_open_focus;
                // Windows have to be shown from the main thread.
                let _ = app.run_on_main_thread(move || {
                    crate::show_panel(&app2, text, focus, true);
                });
            }
        }
    });
}
