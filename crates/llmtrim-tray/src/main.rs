// Prevents a Windows console window from opening alongside the tray icon.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use anyhow::Context as _;
use llmtrim_ledger::breakdown_db::BreakdownDb;
use llmtrim_ledger::dashboard::{Dashboard, build_dashboard, parse_period, sanitise_error};
use llmtrim_ledger::tracking::{Period, db_path};
use tauri::image::Image;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};
use tauri_plugin_positioner::{Position, WindowExt};

/// Menu-bar glyph bytes. macOS gets the black template image (the system tints
/// it for light/dark bars); every other platform gets the green glyph so it
/// stays visible on a dark taskbar. See `tools/gen_icons.py`.
#[cfg(target_os = "macos")]
const TRAY_ICON_PNG: &[u8] = include_bytes!("../icons/tray-mono.png");
#[cfg(not(target_os = "macos"))]
const TRAY_ICON_PNG: &[u8] = include_bytes!("../icons/tray-color.png");

/// Window is re-shown only if the last blur-dismiss is older than this. A tray
/// click that dismisses a focused popover delivers `Focused(false)` (hide) just
/// before the click event, so without this guard the click would re-open it.
const DISMISS_DEBOUNCE: Duration = Duration::from_millis(250);

/// Granularity of the poll sleep, so a quit is observed within ~1s rather than
/// after a full (possibly 30s) interval.
const POLL_TICK: Duration = Duration::from_millis(500);

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

/// Shared mutable state managed by Tauri.
struct TrayState {
    /// Current poll interval in seconds. Updated by `set_poll_interval`.
    poll_interval_secs: u64,
    /// Instant of the most recent blur-driven hide, used to debounce the
    /// tray-click that caused the blur.
    last_dismiss: Option<Instant>,
}

impl Default for TrayState {
    fn default() -> Self {
        Self {
            poll_interval_secs: 30,
            last_dismiss: None,
        }
    }
}

/// Lock the state recovering from a poisoned mutex instead of panicking — a
/// panic in one path must not take down every later IPC call (no `.unwrap()`
/// in production, per project rules). Recovering the inner value is safe here:
/// the only state is `poll_interval_secs` and `last_dismiss`, so the worst case
/// after a poisoning panic is a stale interval or dismiss timestamp, never a
/// corrupt invariant.
fn lock_state(state: &Mutex<TrayState>) -> MutexGuard<'_, TrayState> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ---------------------------------------------------------------------------
// Tauri commands — thin wrappers over llmtrim-ledger pure functions.
//
// Real-IO entry-points: legitimately uncovered by unit tests (the logic lives
// in llmtrim-ledger and is fully tested there).
// ---------------------------------------------------------------------------

/// Return the full dashboard snapshot. The ledger path is resolved the same way the
/// proxy/CLI does — via `llmtrim_ledger::tracking::db_path()` which respects the
/// `LLMTRIM_DB_PATH` env var and `XDG_DATA_HOME`.
///
/// SECURITY: filesystem paths in errors are stripped before reaching JS. The full
/// error detail is written to stderr so it appears in `llmtrim serve` logs.
#[tauri::command]
fn get_dashboard(state: State<'_, Arc<Mutex<TrayState>>>) -> Result<Dashboard, String> {
    let poll_secs = lock_state(&state).poll_interval_secs;
    let dashboard = load_dashboard(poll_secs).map_err(|e| {
        // Strip filesystem paths: log full detail, surface only the class of failure.
        eprintln!("llmtrim-tray: get_dashboard failed: {e:#}");
        sanitise_error(&e)
    })?;
    Ok(dashboard)
}

/// Return raw saved_pct trend buckets for one agent over a given period.
/// `period` accepts "day", "week", or "month" (case-insensitive).
#[tauri::command]
fn get_agent_trend(agent: String, period: String) -> Result<Vec<f64>, String> {
    let p = parse_period(&period).map_err(|e| format!("unknown period {period:?}: {e}"))?;
    let path = db_path().map_err(|e| {
        eprintln!("llmtrim-tray: db_path failed: {e:#}");
        "could not resolve ledger path".to_string()
    })?;
    let db = BreakdownDb::open_readonly(&path).map_err(|e| {
        eprintln!("llmtrim-tray: open_readonly failed: {e:#}");
        sanitise_error(&e)
    })?;
    let trend = db.agent_trend(&agent, p, 30).map_err(|e| {
        eprintln!("llmtrim-tray: agent_trend failed: {e:#}");
        "failed to query trend data".to_string()
    })?;
    Ok(trend.into_iter().map(|b| b.saved_pct).collect())
}

/// Update the poll interval for the background refresh loop.
#[tauri::command]
fn set_poll_interval(secs: u64, state: State<'_, Arc<Mutex<TrayState>>>) {
    lock_state(&state).poll_interval_secs = secs;
}

/// Quit the application cleanly.
#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

// ---------------------------------------------------------------------------
// Application setup
// ---------------------------------------------------------------------------

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_positioner::init())
        .manage(Arc::new(Mutex::new(TrayState::default())))
        .manage(Arc::new(AtomicBool::new(false)))
        .setup(|app| {
            // ----------------------------------------------------------------
            // macOS: run as a menubar app — hide the Dock icon.
            // `ActivationPolicy::Accessory` is the AppKit equivalent of
            // `setActivationPolicy(.accessory)` in Swift; it removes the Dock
            // entry and the default menu bar without touching our tray icon.
            // ----------------------------------------------------------------
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            // ----------------------------------------------------------------
            // Popover window — hidden at start; toggled by tray click.
            // ----------------------------------------------------------------
            let popover = app
                .get_webview_window("popover")
                .expect("popover window not found — check tauri.conf.json");

            // Auto-hide on blur (works on both macOS and Windows). Record the
            // dismiss time so the tray click that caused this blur doesn't
            // immediately re-open the window (see `toggle_popover`).
            let popover_blur = popover.clone();
            let blur_handle = app.handle().clone();
            popover.on_window_event(move |event| {
                if let tauri::WindowEvent::Focused(false) = event {
                    let _ = popover_blur.hide();
                    let state = blur_handle.state::<Arc<Mutex<TrayState>>>();
                    lock_state(&state).last_dismiss = Some(Instant::now());
                }
            });

            // ----------------------------------------------------------------
            // Tray icon with macOS title and tooltip.
            // ----------------------------------------------------------------
            let tray_app = app.handle().clone();
            // tauri::Error is a std::error::Error, so `?` converts into the
            // setup closure's Box<dyn Error> directly (anyhow's Context would not).
            let tray_icon = Image::from_bytes(TRAY_ICON_PNG)?;
            TrayIconBuilder::new()
                .id("main")
                .icon(tray_icon)
                // macOS template: the black glyph is auto-tinted per menu-bar theme.
                .icon_as_template(cfg!(target_os = "macos"))
                .tooltip("llmtrim — compression savings")
                .on_tray_icon_event(move |tray, event| {
                    // Forward to positioner so TrayCenter positioning works.
                    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);

                    // Match only the left-button release. `Click` fires on both
                    // press and release; the wildcard would toggle twice per click.
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_popover(&tray_app);
                    }
                })
                .build(app)?;

            // ----------------------------------------------------------------
            // Update macOS menubar title with aggregate % saved on first load.
            // ----------------------------------------------------------------
            #[cfg(target_os = "macos")]
            {
                if let Ok(dash) = load_dashboard(30) {
                    let pct = dash.totals.saved_pct;
                    if let Some(tray) = app.tray_by_id("main") {
                        let _ = tray.set_title(Some(&format!("{pct:.0}% saved")));
                    }
                }
            }

            // ----------------------------------------------------------------
            // Background poll loop: emits a `dashboard` event every N seconds.
            // The stop flag lets the loop exit promptly on app shutdown.
            // ----------------------------------------------------------------
            let poll_app = app.handle().clone();
            let stop = app.state::<Arc<AtomicBool>>().inner().clone();
            std::thread::spawn(move || {
                poll_loop(poll_app, stop);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard,
            get_agent_trend,
            set_poll_interval,
            quit,
        ])
        .build(tauri::generate_context!())
        .expect("error building llmtrim tray")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                // Signal the poll thread to stop before the process tears down.
                app.state::<Arc<AtomicBool>>()
                    .store(true, Ordering::Relaxed);
            }
        });
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load a fresh dashboard snapshot from the ledger.
fn load_dashboard(poll_secs: u64) -> anyhow::Result<Dashboard> {
    let path = db_path().context("could not resolve ledger path")?;
    let db = BreakdownDb::open_readonly(&path).context("could not open ledger")?;
    let aggregates = db
        .agent_aggregates()
        .context("agent_aggregates query failed")?;
    let mut trends: HashMap<String, Vec<f64>> = HashMap::new();
    for agg in &aggregates {
        let trend = db
            .agent_trend(&agg.agent, Period::Day, 30)
            .context("agent_trend query failed")?;
        trends.insert(
            agg.agent.clone(),
            trend.into_iter().map(|b| b.saved_pct).collect(),
        );
    }
    let now = chrono::Utc::now().to_rfc3339();
    Ok(build_dashboard(aggregates, trends, now, poll_secs))
}

/// Toggle the popover window: show (positioned at TrayCenter) or hide.
/// Includes a short debounce so a tray click while the window is closing
/// doesn't re-open it immediately.
fn toggle_popover(app: &AppHandle) {
    let Some(popover) = app.get_webview_window("popover") else {
        return;
    };
    let visible = popover.is_visible().unwrap_or(false);
    if visible {
        let _ = popover.hide();
    } else {
        // If the window was just dismissed by blur (this very click moved focus
        // away first), don't re-open it — that would make a dismissing click a
        // no-op flicker.
        let state = app.state::<Arc<Mutex<TrayState>>>();
        if let Some(t) = lock_state(&state).last_dismiss {
            if t.elapsed() < DISMISS_DEBOUNCE {
                return;
            }
        }
        // Position next to the tray icon before showing.
        let _ = popover.move_window(Position::TrayCenter);
        let _ = popover.show();
        let _ = popover.set_focus();
    }
}

/// Background poll loop: sleeps `poll_interval_secs`, then emits a `dashboard`
/// event on the app so the frontend can refresh without polling from JS.
fn poll_loop(app: AppHandle, stop: Arc<AtomicBool>) {
    loop {
        // Read current interval from state.
        let secs = lock_state(&app.state::<Arc<Mutex<TrayState>>>()).poll_interval_secs;

        // Sleep in short ticks so a quit is observed within ~POLL_TICK rather
        // than after a full (possibly 30s) interval. A mid-sleep change to the
        // interval also takes effect on the next cycle.
        let mut elapsed = Duration::ZERO;
        let target = Duration::from_secs(secs);
        while elapsed < target {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(POLL_TICK);
            elapsed += POLL_TICK;
        }
        if stop.load(Ordering::Relaxed) {
            return;
        }

        match load_dashboard(secs) {
            Ok(dash) => {
                // Update macOS menubar title.
                #[cfg(target_os = "macos")]
                {
                    let pct = dash.totals.saved_pct;
                    if let Some(tray) = app.tray_by_id("main") {
                        let _ = tray.set_title(Some(&format!("{pct:.0}% saved")));
                    }
                }
                // Emit to all webview windows (front-end listens for "dashboard").
                let _ = app.emit("dashboard", &dash);
            }
            Err(e) => {
                eprintln!("llmtrim-tray: poll failed: {e:#}");
            }
        }
    }
}
