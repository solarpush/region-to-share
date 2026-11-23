//! Core v3 `RegionApp` orchestrating:
//! - QuickHub launcher with saved presets & KDE native portal shortcut
//! - Pro Spotlight Region Selector (zero self-capture, 8 handles, 4x loupe, ratios)
//! - Clean pure-pixel Mirror Window + Detached Mini-HUD Pill
//! - Simultaneous Live Recording (`FfmpegPipeRecorder`) & PipeWire Virtual Camera via `FrameHub`
//! - Live D-Bus (`io.github.solarpush.RegionToShare`) & Unix Socket IPC control

use crate::hud::{hud_dimensions, render_embedded_hud, render_hud, HudAction, HudState};
use crate::icons::{paint_icon, Icon};
use crate::pause_screen::{
    build_pause_logo_frame, embedded_logo_color_image, render_pause_message_screen,
    render_pause_video_brand_badge, PauseVideoPlayer,
};
use crate::selector::{RegionSelectorView, SelectorOutcome};
use crate::settings_view::{render_settings_view, SettingsAction, SettingsTab};
use crate::theme::{apply_theme, install_fonts, Palette};
use crate::widgets::{action_card, pill_button, status_badge};
use eframe::egui;
use log::{debug, error, info, warn};
use parking_lot::{Mutex as FastMutex, RwLock};
use region_capture::{AutoBackend, CaptureBackend, Frame, FrameHub};
use region_config::ConfigStore;
use region_core::performance::FrameProfiler;
use region_core::{
    PauseScreenMode, PixelFormat, PresetAction, Rectangle, SessionStateKind, UiTheme,
};
use region_ipc::{IpcCommand, PresetDbusItem, SessionStatusSnapshot};
use region_platform::{
    bring_window_to_front, copy_frame_to_clipboard, detect_system_color_scheme,
    pick_video_file_blocking, save_frame_png, set_window_keep_below, set_window_minimized,
    AppPaths, DesktopEnvironment, DisplayServer, ResourceMonitor, SystemColorScheme,
};
use region_recorder::{
    expand_output_path, EncoderCapabilities, FfmpegPipeRecorder, RecorderBackend, RecordingJob,
    RecordingOutput,
};
use rust_i18n::t;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex as TokioMutex;

#[cfg(target_os = "linux")]
use region_portal::{
    PortalBackend, PortalBackendOptions, PortalSessionMode, RestoreToken, VirtualCameraConfig,
    VirtualCameraSink,
};

const HUB_WINDOW_SIZE: [f32; 2] = [500.0, 560.0];

/// Initial action passed from `region-cli` into the GUI.
#[derive(Debug, Clone, Default)]
pub enum LaunchIntent {
    #[default]
    QuickHub,
    Share {
        region: Option<Rectangle>,
        native_portal: bool,
        virtual_cam: bool,
    },
    Record {
        region: Option<Rectangle>,
        output: Option<PathBuf>,
        native_portal: bool,
        also_share: bool,
    },
    Snapshot {
        region: Option<Rectangle>,
        output: Option<PathBuf>,
        to_clipboard: bool,
    },
    OpenSettings,
}

/// Action to run after the user confirms a selection in the overlay.
#[derive(Debug, Clone)]
enum PendingSelectionAction {
    Share,
    Record { output: Option<PathBuf> },
    VirtualCam,
    Snapshot { output: Option<PathBuf>, to_clipboard: bool },
}

enum AppScreen {
    Hub,
    Settings,
    PreparingCapture,
    Selecting,
    Streaming,
}

enum UiEvent {
    ScreenshotReady {
        frame: Arc<Frame>,
        screen_size: (u32, u32),
        precropped: bool,
    },
    Telemetry {
        fps: f64,
        cpu: f64,
        mem_mb: f64,
        rec_active: bool,
        rec_paused: bool,
        stream_paused: bool,
        rec_elapsed: u64,
        rec_bytes: u64,
        encoder: String,
    },
    PortalCancelled,
    RestoreToken(String),
    PauseVideoPicked(PathBuf),
    Error(String),
    Notice(String),
    Stopped,
}

/// Deferred viewport resize state so Wayland compositors finish exiting fullscreen
/// before `InnerSize` is applied (otherwise KWin/Mutter resets the size to Hub size).
struct PendingResize {
    target_pts: (f32, f32),
    ready_at: std::time::Instant,
    send_to_background_after: bool,
}

enum SessionControl {
    Stop,
    /// Stop streaming but recycle the live Portal/PipeWire backend into `reusable_backend`
    /// so re-selecting a new region does NOT reopen the Wayland portal permission prompt!
    /// Signals `done` once the backend has been returned to `reusable_backend`.
    StopAndRecycle(tokio::sync::oneshot::Sender<()>),
    UpdateRegion(Rectangle),
    StartRecord(Option<PathBuf>),
    StopRecord,
    SetStreamPaused {
        paused: bool,
        mode: PauseScreenMode,
        video_path: String,
    },
    ToggleVirtualCam(bool),
    TakeSnapshot {
        to_clipboard: bool,
        output: Option<PathBuf>,
    },
}

/// Launch the Region to Share v3 GUI application.
pub fn run_gui(
    runtime: tokio::runtime::Runtime,
    store: ConfigStore,
    intent: LaunchIntent,
) -> Result<(), eframe::Error> {
    apply_locale(&store.config.ui.language);

    let system_dark = runtime.block_on(async {
        matches!(detect_system_color_scheme().await, SystemColorScheme::Dark)
    });
    let initial_dark = match store.config.ui.theme {
        UiTheme::Auto => system_dark,
        UiTheme::Dark => true,
        UiTheme::Light => false,
    };

    let shared_status = Arc::new(RwLock::new(build_initial_status(&store)));
    let (ipc_tx, ipc_rx) = mpsc::unbounded_channel::<IpcCommand>();

    let options = eframe::NativeOptions {
        // On Wayland, EGL vsync (`SwapInterval::Wait(1)`) blocks the main UI thread
        // indefinitely inside `eglSwapBuffers()` waiting for `wl_surface.frame` whenever
        // the root mirror window is minimized or moved to another virtual desktop — which
        // freezes the detached HUD pill AND stalls offscreen PipeWire window streaming.
        // Wayland compositors always composite tear-free (mailbox), so disabling EGL vsync
        // on Wayland prevents the deadlock while keeping the mirror live in the background.
        vsync: !DisplayServer::detect().is_wayland(),
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(HUB_WINDOW_SIZE)
            .with_min_inner_size(HUB_WINDOW_SIZE)
            .with_max_inner_size(HUB_WINDOW_SIZE)
            .with_title(&store.config.display.window_title)
            .with_resizable(false),
        ..Default::default()
    };

    let rt = Arc::new(runtime);

    eframe::run_native(
        "Region to Share",
        options,
        Box::new(move |cc| {
            install_fonts(&cc.egui_ctx);
            apply_theme(&cc.egui_ctx, Palette::for_dark_mode(initial_dark));

            // Wire D-Bus service + Unix socket IPC server to wake egui immediately
            let wake_ctx = cc.egui_ctx.clone();
            let wake_fn: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                wake_ctx.request_repaint();
            });

            let sock_wake_ctx = cc.egui_ctx.clone();
            region_ipc::socket::spawn_socket_server(
                ipc_tx.clone(),
                Arc::clone(&shared_status),
                Some(Box::new(move || sock_wake_ctx.request_repaint())),
            );

            #[cfg(target_os = "linux")]
            let dbus_slot: Arc<TokioMutex<Option<region_ipc::DbusServiceHandle>>> =
                Arc::new(TokioMutex::new(None));

            #[cfg(target_os = "linux")]
            {
                let st = Arc::clone(&shared_status);
                let tx = ipc_tx.clone();
                let wf = Arc::clone(&wake_fn);
                let slot = Arc::clone(&dbus_slot);
                rt.spawn(async move {
                    match region_ipc::DbusServiceHandle::start(st, tx, wf).await {
                        Ok(handle) => {
                            *slot.lock().await = Some(handle);
                        }
                        Err(e) => warn!("[D-Bus] Could not register session service: {}", e),
                    }
                });
            }

            Ok(Box::new(RegionApp::new(
                rt,
                store,
                shared_status,
                #[cfg(target_os = "linux")]
                dbus_slot,
                ipc_rx,
                system_dark,
                intent,
            )))
        }),
    )
}

fn apply_locale(lang_setting: &str) {
    let lang = if !lang_setting.is_empty() {
        lang_setting.to_string()
    } else {
        sys_locale::get_locale()
            .unwrap_or_else(|| "en".to_string())
            .split(|c: char| c == '-' || c == '_')
            .next()
            .unwrap_or("en")
            .to_owned()
    };
    rust_i18n::set_locale(&lang);
}

fn build_initial_status(store: &ConfigStore) -> SessionStatusSnapshot {
    SessionStatusSnapshot {
        presets: store
            .config
            .presets
            .iter()
            .map(|p| PresetDbusItem {
                id: p.id.clone(),
                name: p.name.clone(),
                x: p.region.x,
                y: p.region.y,
                width: p.region.width,
                height: p.region.height,
            })
            .collect(),
        ..Default::default()
    }
}

struct RegionApp {
    runtime: Arc<tokio::runtime::Runtime>,
    store: ConfigStore,
    shared_status: Arc<RwLock<SessionStatusSnapshot>>,
    #[cfg(target_os = "linux")]
    dbus_slot: Arc<TokioMutex<Option<region_ipc::DbusServiceHandle>>>,
    ipc_rx: UnboundedReceiver<IpcCommand>,
    ui_tx: UnboundedSender<UiEvent>,
    ui_rx: UnboundedReceiver<UiEvent>,

    screen: AppScreen,
    settings_tab: SettingsTab,
    system_dark: bool,
    palette: Palette,
    enc_caps: EncoderCapabilities,

    // Selection state
    selector: RegionSelectorView,
    pending_action: PendingSelectionAction,
    screenshot_tex: Option<egui::TextureHandle>,
    backend_screen_size: Option<(u32, u32)>,
    reusable_backend: Arc<TokioMutex<Option<Box<dyn CaptureBackend>>>>,

    // Active streaming/recording state
    active_region: Rectangle,
    mirror_visible: bool,
    mirror_tex: Option<egui::TextureHandle>,
    logo_tex: Option<egui::TextureHandle>,
    pause_video_tex: Option<egui::TextureHandle>,
    /// Single-slot latest-frame mailbox (O(1), prevents unbounded RAM accumulation!).
    latest_live_frame: Arc<FastMutex<Option<Arc<Frame>>>>,
    /// Separate single-slot mailbox for looping pause video frames (never mixes with desktop pixels!).
    latest_pause_video_frame: Arc<FastMutex<Option<Arc<Frame>>>>,
    /// Actual physical pixel dimensions of the live stream (for exact 1:1 sizing).
    live_frame_phys_size: Option<(u32, u32)>,
    /// Tracks whether we already snapped the mirror window 1:1 to the current frame size.
    snapped_window_for_phys_size: Option<(u32, u32)>,
    session_ctrl: Option<UnboundedSender<SessionControl>>,
    hud_state: HudState,
    pending_window_resize: Option<PendingResize>,
    pending_hub_lock_at: Option<std::time::Instant>,
    hub_size_retries: u8,
    last_root_hud_size: Option<egui::Vec2>,
    pending_auto_background_at: Option<std::time::Instant>,
    pending_launch_intent: Option<LaunchIntent>,
    toast_notice: Option<(String, std::time::Instant)>,
    integration_feedback: Option<String>,
}

impl RegionApp {
    fn new(
        runtime: Arc<tokio::runtime::Runtime>,
        store: ConfigStore,
        shared_status: Arc<RwLock<SessionStatusSnapshot>>,
        #[cfg(target_os = "linux")] dbus_slot: Arc<
            TokioMutex<Option<region_ipc::DbusServiceHandle>>,
        >,
        ipc_rx: UnboundedReceiver<IpcCommand>,
        system_dark: bool,
        intent: LaunchIntent,
    ) -> Self {
        let dark = match store.config.ui.theme {
            UiTheme::Auto => system_dark,
            UiTheme::Dark => true,
            UiTheme::Light => false,
        };
        let (ui_tx, ui_rx) = mpsc::unbounded_channel();
        let selector = RegionSelectorView::new(
            store.config.ui.default_aspect_ratio,
            store.config.ui.show_selection_grid,
            store.config.ui.show_magnifier,
        );
        let active_region = store
            .last_region()
            .unwrap_or_else(|| Rectangle::new(200, 140, 1280, 720));

        Self {
            runtime,
            store,
            shared_status,
            #[cfg(target_os = "linux")]
            dbus_slot,
            ipc_rx,
            ui_tx,
            ui_rx,
            screen: AppScreen::Hub,
            settings_tab: SettingsTab::Capture,
            system_dark,
            palette: Palette::for_dark_mode(dark),
            enc_caps: EncoderCapabilities::probe(),
            selector,
            pending_action: PendingSelectionAction::Share,
            screenshot_tex: None,
            backend_screen_size: None,
            reusable_backend: Arc::new(TokioMutex::new(None)),
            active_region,
            mirror_visible: true,
            mirror_tex: None,
            logo_tex: None,
            pause_video_tex: None,
            latest_live_frame: Arc::new(FastMutex::new(None)),
            latest_pause_video_frame: Arc::new(FastMutex::new(None)),
            live_frame_phys_size: None,
            snapped_window_for_phys_size: None,
            session_ctrl: None,
            hud_state: HudState {
                is_recording: false,
                is_paused: false,
                mirror_visible: true,
                mirror_keep_below: false,
                virtual_cam_active: false,
                show_tools: false,
                show_stats: false,
                fps: 0.0,
                cpu_percent: 0.0,
                memory_mb: 0.0,
                rec_elapsed_secs: 0,
                rec_bytes: 0,
                encoder_label: String::new(),
            },
            pending_window_resize: None,
            pending_hub_lock_at: None,
            hub_size_retries: 0,
            last_root_hud_size: None,
            pending_auto_background_at: None,
            pending_launch_intent: Some(intent),
            toast_notice: None,
            integration_feedback: None,
        }
    }

    fn set_notice(&mut self, msg: impl Into<String>) {
        self.toast_notice = Some((msg.into(), std::time::Instant::now()));
    }

    fn sync_presets_to_ipc(&self) {
        let mut st = self.shared_status.write();
        st.presets = self
            .store
            .config
            .presets
            .iter()
            .map(|p| PresetDbusItem {
                id: p.id.clone(),
                name: p.name.clone(),
                x: p.region.x,
                y: p.region.y,
                width: p.region.width,
                height: p.region.height,
            })
            .collect();
    }

    fn begin_selection(
        &mut self,
        ctx: &egui::Context,
        action: PendingSelectionAction,
        native_portal_direct: bool,
    ) {
        self.begin_selection_with_recycle(ctx, action, native_portal_direct, None, false);
    }

    fn begin_selection_with_recycle(
        &mut self,
        ctx: &egui::Context,
        action: PendingSelectionAction,
        native_portal_direct: bool,
        wait_for_recycle: Option<tokio::sync::oneshot::Receiver<()>>,
        bring_to_front_first: bool,
    ) {
        self.pending_action = action;
        self.screen = AppScreen::PreparingCapture;
        self.screenshot_tex = None;
        self.selector.editor.clear();
        self.shared_status.write().state = SessionStateKind::Selecting;

        // Hide the launcher BEFORE capturing the desktop so it NEVER appears inside the screenshot!
        // When re-selecting from an active mirror (`bring_to_front_first`), we clear keepBelow /
        // hidden desktop FIRST while the window is still mapped in KWin, and then hide it.
        if !native_portal_direct && !bring_to_front_first {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        let rt = Arc::clone(&self.runtime);
        let ui_tx = self.ui_tx.clone();
        let ctx_cl = ctx.clone();
        let backend_slot = Arc::clone(&self.reusable_backend);
        let restore_token = self.store.restore_token().map(ToOwned::to_owned);
        let include_cursor = self.store.config.capture.show_cursor;
        let prefer_dmabuf = self.store.config.portal.prefer_dmabuf;
        let window_title = self.store.config.display.window_title.clone();

        rt.spawn(async move {
            // Bring window back to the active desktop, un-minimize & clear `keepBelow` FIRST
            // before hiding for the desktop snapshot, so the fullscreen selector opens on top!
            if bring_to_front_first && !native_portal_direct {
                if !DisplayServer::detect().is_wayland() {
                    ctx_cl.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                }
                let _ = bring_window_to_front(&window_title).await;
                ctx_cl.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                ctx_cl.request_repaint();
            }

            // If recycling from an active stream (`Reselect`), wait deterministically until the
            // old capture pipeline returns the live Portal backend into `backend_slot`.
            if let Some(rx) = wait_for_recycle {
                let _ = tokio::time::timeout(std::time::Duration::from_millis(800), rx).await;
            }

            // Brief settle delay so the compositor unmaps the launcher window before snapshotting
            if !native_portal_direct {
                let settle_ms = if bring_to_front_first { 180 } else { 140 };
                tokio::time::sleep(tokio::time::Duration::from_millis(settle_ms)).await;
            }

            let is_wayland = DisplayServer::detect().is_wayland();
            // Reuse an already-authorized Portal/PipeWire session if present (ZERO prompt!)
            let recycled = if is_wayland && !native_portal_direct {
                backend_slot.lock().await.take()
            } else {
                None
            };

            let mut backend: Box<dyn CaptureBackend> = if let Some(b) = recycled {
                debug!("[UI] Reusing live Portal session for instant re-selection (zero prompt)");
                b
            } else if is_wayland {
                #[cfg(target_os = "linux")]
                {
                    let mode = if native_portal_direct {
                        PortalSessionMode::NativeDirect
                    } else {
                        PortalSessionMode::MonitorForOverlay
                    };
                    Box::new(PortalBackend::with_options(PortalBackendOptions {
                        restore_token: restore_token.map(RestoreToken),
                        include_cursor,
                        session_mode: mode,
                        prefer_dmabuf,
                    }))
                }
                #[cfg(not(target_os = "linux"))]
                {
                    let _ = ui_tx.send(UiEvent::Error("Wayland not supported on this OS".into()));
                    ctx_cl.request_repaint();
                    return;
                }
            } else {
                match AutoBackend::new() {
                    Ok(b) => Box::new(b),
                    Err(e) => {
                        let _ = ui_tx.send(UiEvent::Error(format!("Capture init failed: {e}")));
                        ctx_cl.request_repaint();
                        return;
                    }
                }
            };

            if let Err(e) = backend.init(Rectangle::new(0, 0, 1920, 1080)).await {
                let msg = e.to_string();
                if msg.contains("cancelled") || msg.contains("Cancelled") {
                    let _ = ui_tx.send(UiEvent::PortalCancelled);
                } else {
                    let _ = ui_tx.send(UiEvent::Error(msg));
                }
                ctx_cl.request_repaint();
                return;
            }

            let screen_size = backend.get_screen_size().await.unwrap_or((1920, 1080));
            let precropped = backend.is_precropped_stream();
            if let Some(tok) = backend.restore_token_str() {
                let _ = ui_tx.send(UiEvent::RestoreToken(tok));
            }

            if precropped {
                // KDE Plasma 6 "Rectangular Region" or Window stream -> ZERO double selection!
                *backend_slot.lock().await = Some(backend);
                let dummy = Arc::new(Frame::new(
                    screen_size.0,
                    screen_size.1,
                    PixelFormat::BGRA8888,
                    region_capture::FrameData::from_buffer(Vec::new()),
                    Rectangle::new(0, 0, screen_size.0, screen_size.1),
                    0,
                ));
                let _ = ui_tx.send(UiEvent::ScreenshotReady {
                    frame: dummy,
                    screen_size,
                    precropped: true,
                });
                ctx_cl.request_repaint();
                return;
            }

            match backend.capture_screenshot().await {
                Ok(frame) => {
                    if is_wayland {
                        *backend_slot.lock().await = Some(backend);
                    }
                    let _ = ui_tx.send(UiEvent::ScreenshotReady {
                        frame: Arc::new(frame),
                        screen_size,
                        precropped: false,
                    });
                }
                Err(e) => {
                    let _ = ui_tx.send(UiEvent::Error(format!("Screenshot failed: {e}")));
                }
            }
            ctx_cl.request_repaint();
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn start_live_session(
        &mut self,
        ctx: &egui::Context,
        region: Rectangle,
        start_recording: bool,
        record_output: Option<PathBuf>,
        start_vcam: bool,
        show_mirror: bool,
    ) {
        self.stop_live_session(ctx, false);

        self.active_region = region;
        self.mirror_visible = show_mirror;
        if self.store.config.capture.remember_region {
            self.store.set_last_region(region);
            let _ = self.store.save();
        }

        self.screen = AppScreen::Streaming;
        self.hud_state.is_recording = start_recording;
        self.hud_state.is_paused = false;
        self.hud_state.mirror_visible = show_mirror;
        self.hud_state.mirror_keep_below = false;
        self.hud_state.virtual_cam_active = start_vcam;
        self.hud_state.show_tools = false;
        self.hud_state.show_stats = self.store.config.performance.show_performance_overlay;
        self.live_frame_phys_size = None;
        self.snapped_window_for_phys_size = None;
        self.last_root_hud_size = None;
        self.pause_video_tex = None;
        *self.latest_live_frame.lock() = None;
        *self.latest_pause_video_frame.lock() = None;

        if show_mirror {
            self.apply_root_as_mirror_geometry(ctx, true);
        } else {
            self.apply_root_as_hud_geometry(ctx, true);
        }

        let (ctrl_tx, ctrl_rx) = mpsc::unbounded_channel::<SessionControl>();
        self.session_ctrl = Some(ctrl_tx);

        let rt = Arc::clone(&self.runtime);
        let ui_tx = self.ui_tx.clone();
        let latest_slot = Arc::clone(&self.latest_live_frame);
        let pause_video_slot = Arc::clone(&self.latest_pause_video_frame);
        let ctx_cl = ctx.clone();
        let backend_slot = Arc::clone(&self.reusable_backend);
        let shared_status = Arc::clone(&self.shared_status);
        #[cfg(target_os = "linux")]
        let dbus_slot = Arc::clone(&self.dbus_slot);
        let cfg = self.store.config.clone();
        let enc_caps = self.enc_caps.clone();

        rt.spawn(async move {
            run_capture_pipeline(
                region,
                cfg,
                enc_caps,
                start_recording,
                record_output,
                start_vcam,
                backend_slot,
                ctrl_rx,
                ui_tx,
                latest_slot,
                pause_video_slot,
                shared_status,
                #[cfg(target_os = "linux")]
                dbus_slot,
                ctx_cl,
            )
            .await;
        });
    }

    /// Preserve exact aspect ratio while enforcing sensible min/max logical bounds.
    fn clamp_window_pts_preserving_aspect(w: f32, h: f32) -> (f32, f32) {
        let mut w = w.max(1.0);
        let mut h = h.max(1.0);
        let max_w = 7680.0_f32;
        let max_h = 4320.0_f32;
        if w > max_w || h > max_h {
            let scale = (max_w / w).min(max_h / h);
            w *= scale;
            h *= scale;
        }
        let min_w = 160.0_f32;
        let min_h = 120.0_f32;
        if w < min_w && h < min_h {
            let scale = (min_w / w).max(min_h / h);
            w *= scale;
            h *= scale;
        }
        (w.round(), h.round())
    }

    /// Configure the root viewport as the resizable 1:1 pure-pixel Mirror Window.
    fn apply_root_as_mirror_geometry(&mut self, ctx: &egui::Context, settle_from_fullscreen: bool) {
        self.pending_hub_lock_at = None;
        self.last_root_hud_size = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            egui::WindowLevel::Normal,
        ));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(
            self.store.config.display.show_decorations,
        ));
        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(120.0, 90.0)));
        ctx.send_viewport_cmd(egui::ViewportCommand::MaxInnerSize(egui::vec2(7680.0, 4320.0)));

        let ppp = ctx.pixels_per_point().max(1.0);
        let target_pts = if let Some((fw, fh)) = self.live_frame_phys_size {
            self.snapped_window_for_phys_size = Some((fw, fh));
            Self::clamp_window_pts_preserving_aspect(fw as f32 / ppp, fh as f32 / ppp)
        } else {
            let (raw_w, raw_h) = if DisplayServer::detect().is_wayland() {
                (self.active_region.width as f32, self.active_region.height as f32)
            } else {
                (
                    self.active_region.width as f32 / ppp,
                    self.active_region.height as f32 / ppp,
                )
            };
            Self::clamp_window_pts_preserving_aspect(raw_w, raw_h)
        };

        // ALWAYS defer InnerSize on Wayland (even when transitioning from fixed-size HUD pill!)
        // so KWin commits the unlocked Min/Max/Resizable constraints BEFORE applying exact 1:1 size!
        let settle_ms = if settle_from_fullscreen {
            if DisplayServer::detect().is_wayland() { 260 } else { 60 }
        } else if DisplayServer::detect().is_wayland() {
            140
        } else {
            0
        };
        if settle_ms > 0 {
            self.snapped_window_for_phys_size = None;
            self.pending_window_resize = Some(PendingResize {
                target_pts,
                ready_at: std::time::Instant::now() + std::time::Duration::from_millis(settle_ms),
                send_to_background_after: settle_from_fullscreen
                    && self.store.config.display.auto_background,
            });
        } else {
            self.pending_window_resize = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                target_pts.0,
                target_pts.1,
            )));
        }
    }

    /// Configure the root viewport as the compact, borderless always-on-top HUD pill
    /// when running in mirror-hidden mode (zero mirror window on screen!).
    fn apply_root_as_hud_geometry(&mut self, ctx: &egui::Context, settle_from_fullscreen: bool) {
        self.pending_hub_lock_at = None;
        let size = hud_dimensions(&self.hud_state);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            egui::WindowLevel::AlwaysOnTop,
        ));
        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(size));
        ctx.send_viewport_cmd(egui::ViewportCommand::MaxInnerSize(size));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        self.last_root_hud_size = Some(size);

        if settle_from_fullscreen {
            let settle_ms = if DisplayServer::detect().is_wayland() { 260 } else { 60 };
            self.pending_window_resize = Some(PendingResize {
                target_pts: (size.x, size.y),
                ready_at: std::time::Instant::now() + std::time::Duration::from_millis(settle_ms),
                send_to_background_after: false,
            });
        } else {
            self.pending_window_resize = None;
        }
    }

    /// Lock the Hub / Settings window strictly to fixed `HUB_WINDOW_SIZE` so it NEVER
    /// inherits the mirror window's or fullscreen selector's dimensions!
    fn enforce_hub_window_geometry(&mut self, ctx: &egui::Context) {
        let fixed = egui::vec2(HUB_WINDOW_SIZE[0], HUB_WINDOW_SIZE[1]);
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            egui::WindowLevel::Normal,
        ));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(fixed));
        ctx.send_viewport_cmd(egui::ViewportCommand::MaxInnerSize(fixed));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(fixed));

        let settle_ms = if DisplayServer::detect().is_wayland() { 240 } else { 60 };
        self.pending_hub_lock_at =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(settle_ms));
        self.hub_size_retries = 3;
    }

    /// Snap mirror window size to exact 1:1 physical pixel match for the active stream.
    fn snap_window_to_native_1_to_1(&mut self, ctx: &egui::Context) {
        if !self.mirror_visible {
            return;
        }
        if let Some((fw, fh)) = self.live_frame_phys_size {
            let ppp = ctx.pixels_per_point().max(1.0);
            let pts = Self::clamp_window_pts_preserving_aspect(fw as f32 / ppp, fh as f32 / ppp);
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(120.0, 90.0)));
            ctx.send_viewport_cmd(egui::ViewportCommand::MaxInnerSize(egui::vec2(7680.0, 4320.0)));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(pts.0, pts.1)));
            self.snapped_window_for_phys_size = Some((fw, fh));
        }
    }

    fn set_stream_paused(&mut self, paused: bool) {
        self.hud_state.is_paused = paused;
        if !paused {
            self.pause_video_tex = None;
            *self.latest_pause_video_frame.lock() = None;
        }
        if let Some(ref tx) = self.session_ctrl {
            let _ = tx.send(SessionControl::SetStreamPaused {
                paused,
                mode: self.store.config.display.pause_mode,
                video_path: self.store.config.display.pause_video_path.clone(),
            });
        }
    }

    fn stop_live_session(&mut self, ctx: &egui::Context, return_to_hub: bool) {
        if let Some(tx) = self.session_ctrl.take() {
            let _ = tx.send(SessionControl::Stop);
        }
        self.mirror_tex = None;
        self.pause_video_tex = None;
        self.mirror_visible = true;
        *self.latest_live_frame.lock() = None;
        *self.latest_pause_video_frame.lock() = None;
        self.live_frame_phys_size = None;
        self.snapped_window_for_phys_size = None;
        self.pending_window_resize = None;
        self.last_root_hud_size = None;
        self.pending_auto_background_at = None;
        self.hud_state.is_recording = false;
        self.hud_state.is_paused = false;
        self.hud_state.mirror_visible = true;
        let was_backgrounded = self.hud_state.mirror_keep_below;
        self.hud_state.mirror_keep_below = false;
        self.hud_state.virtual_cam_active = false;
        self.hud_state.show_tools = false;
        self.shared_status.write().state = SessionStateKind::Idle;

        if return_to_hub {
            self.screen = AppScreen::Hub;
            if !DisplayServer::detect().is_wayland() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            }
            self.enforce_hub_window_geometry(ctx);
            if was_backgrounded {
                let title = self.store.config.display.window_title.clone();
                self.runtime.spawn(async move {
                    let _ = bring_window_to_front(&title).await;
                });
            }
        }
    }

    fn handle_ipc_commands(&mut self, ctx: &egui::Context) {
        while let Ok(cmd) = self.ipc_rx.try_recv() {
            debug!("[UI] Received IPC command: {:?}", cmd);
            match cmd {
                IpcCommand::Raise => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    if !DisplayServer::detect().is_wayland() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    let title = self.store.config.display.window_title.clone();
                    self.runtime.spawn(async move {
                        let _ = bring_window_to_front(&title).await;
                    });
                }
                IpcCommand::OpenSettings => {
                    self.stop_live_session(ctx, true);
                    self.screen = AppScreen::Settings;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                IpcCommand::StartShare {
                    native_portal,
                    preset_or_region,
                    virtual_cam,
                } => {
                    if let Some(target) = preset_or_region {
                        if let Some(r) = self.resolve_region_or_preset(&target) {
                            self.start_live_session(ctx, r, false, None, virtual_cam, !virtual_cam);
                            continue;
                        }
                    }
                    let act = if virtual_cam {
                        PendingSelectionAction::VirtualCam
                    } else {
                        PendingSelectionAction::Share
                    };
                    self.begin_selection(ctx, act, native_portal);
                }
                IpcCommand::StartRecord {
                    native_portal,
                    preset_or_region,
                    output_path,
                    also_share,
                } => {
                    let out = output_path.map(PathBuf::from);
                    if let Some(target) = preset_or_region {
                        if let Some(r) = self.resolve_region_or_preset(&target) {
                            self.start_live_session(ctx, r, true, out, false, also_share);
                            continue;
                        }
                    }
                    self.begin_selection(
                        ctx,
                        PendingSelectionAction::Record { output: out },
                        native_portal,
                    );
                }
                IpcCommand::TakeSnapshot {
                    to_clipboard,
                    output_path,
                } => {
                    if let Some(ref tx) = self.session_ctrl {
                        let _ = tx.send(SessionControl::TakeSnapshot {
                            to_clipboard,
                            output: output_path.map(PathBuf::from),
                        });
                    } else {
                        self.begin_selection(
                            ctx,
                            PendingSelectionAction::Snapshot {
                                output: output_path.map(PathBuf::from),
                                to_clipboard,
                            },
                            false,
                        );
                    }
                }
                IpcCommand::ToggleRecord => {
                    if let Some(ref tx) = self.session_ctrl {
                        if self.hud_state.is_recording {
                            let _ = tx.send(SessionControl::StopRecord);
                        } else {
                            let _ = tx.send(SessionControl::StartRecord(None));
                        }
                    }
                }
                IpcCommand::Pause => {
                    self.set_stream_paused(true);
                }
                IpcCommand::Resume => {
                    self.set_stream_paused(false);
                }
                IpcCommand::Stop => {
                    self.stop_live_session(ctx, true);
                }
                IpcCommand::SetRegion { region } => {
                    self.active_region = region;
                    if let Some(ref tx) = self.session_ctrl {
                        let _ = tx.send(SessionControl::UpdateRegion(region));
                    }
                }
                IpcCommand::GetStatus => {}
            }
        }
    }

    fn resolve_region_or_preset(&self, target: &str) -> Option<Rectangle> {
        if let Some(preset) = self.store.config.find_preset(target) {
            return Some(preset.region);
        }
        target.parse::<Rectangle>().ok()
    }

    fn process_ui_events(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.ui_rx.try_recv() {
            match ev {
                UiEvent::ScreenshotReady {
                    frame,
                    screen_size,
                    precropped,
                } => {
                    self.backend_screen_size = Some(screen_size);
                    if precropped {
                        // Direct KDE Portal stream -> skip secondary selector!
                        let r = Rectangle::new(0, 0, screen_size.0, screen_size.1);
                        let (rec, out, vcam, show_mirror) = match self.pending_action.clone() {
                            PendingSelectionAction::Record { output, .. } => {
                                (true, output, false, false)
                            }
                            PendingSelectionAction::VirtualCam => (false, None, true, false),
                            _ => (false, None, false, true),
                        };
                        self.start_live_session(ctx, r, rec, out, vcam, show_mirror);
                    } else if let Some(img) = Self::frame_to_color_image(&frame) {
                        self.screenshot_tex = Some(ctx.load_texture(
                            "v3_screenshot",
                            img,
                            egui::TextureOptions::LINEAR,
                        ));
                        self.screen = AppScreen::Selecting;
                        self.pending_hub_lock_at = None;
                        // Unlock fixed Hub constraints BEFORE going fullscreen!
                        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                            egui::WindowLevel::Normal,
                        ));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
                            120.0, 90.0,
                        )));
                        ctx.send_viewport_cmd(egui::ViewportCommand::MaxInnerSize(egui::vec2(
                            16384.0, 16384.0,
                        )));
                        if !DisplayServer::detect().is_wayland() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);

                        // Also raise & activate via KWin/X11 once the fullscreen surface maps so
                        // compositor focus-stealing prevention or stale keepBelow NEVER puts the
                        // region selector underneath other windows.
                        let title = self.store.config.display.window_title.clone();
                        self.runtime.spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(90)).await;
                            let _ = bring_window_to_front(&title).await;
                        });
                    }
                }
                UiEvent::Telemetry {
                    fps,
                    cpu,
                    mem_mb,
                    rec_active,
                    rec_paused,
                    stream_paused,
                    rec_elapsed,
                    rec_bytes,
                    encoder,
                } => {
                    self.hud_state.fps = fps;
                    self.hud_state.cpu_percent = cpu;
                    self.hud_state.memory_mb = mem_mb;
                    self.hud_state.is_recording = rec_active;
                    self.hud_state.is_paused = stream_paused || rec_paused;
                    self.hud_state.rec_elapsed_secs = rec_elapsed;
                    self.hud_state.rec_bytes = rec_bytes;
                    self.hud_state.encoder_label = encoder;
                }
                UiEvent::PortalCancelled => {
                    self.stop_live_session(ctx, true);
                    self.set_notice("Portal selection cancelled");
                }
                UiEvent::RestoreToken(tok) => {
                    self.store.set_restore_token(Some(tok));
                }
                UiEvent::PauseVideoPicked(path) => {
                    self.store.config.display.pause_video_path = path.display().to_string();
                    let _ = self.store.save();
                }
                UiEvent::Error(e) => {
                    error!("[UI] {}", e);
                    self.stop_live_session(ctx, true);
                    self.set_notice(e);
                }
                UiEvent::Notice(n) => {
                    self.set_notice(n);
                }
                UiEvent::Stopped => {
                    self.stop_live_session(ctx, true);
                }
            }
        }

        // While stream is paused, pull ONLY looping pause-video frames (never render desktop pixels!)
        if self.hud_state.is_paused {
            let _ = self.latest_live_frame.lock().take();
            if self.mirror_visible {
                if let Some(pframe) = self.latest_pause_video_frame.lock().take() {
                    if let Some(img) = Self::frame_to_color_image(&pframe) {
                        if let Some(tex) = self.pause_video_tex.as_mut() {
                            tex.set(img, egui::TextureOptions::LINEAR);
                        } else {
                            self.pause_video_tex = Some(ctx.load_texture(
                                "v3_pause_video",
                                img,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                }
            }
            return;
        }

        // Pull AT MOST ONE latest frame from the single-slot mailbox (prevents RAM buildup!)
        let latest_live_frame = self.latest_live_frame.lock().take();
        if let Some(frame) = latest_live_frame {
            let phys_size = (frame.width, frame.height);
            self.live_frame_phys_size = Some(phys_size);

            if self.mirror_visible {
                // Once fullscreen settle has finished, ensure mirror window matches exact 1:1 physical pixel size!
                if matches!(self.screen, AppScreen::Streaming)
                    && self.pending_window_resize.is_none()
                    && self.snapped_window_for_phys_size != Some(phys_size)
                {
                    self.snap_window_to_native_1_to_1(ctx);
                }

                if let Some(img) = Self::frame_to_color_image(&frame) {
                    // Choose NEAREST when displayed 1:1 for razor-sharp text; LINEAR if resized
                    let ppp = ctx.pixels_per_point().max(1.0);
                    let vp_rect = ctx.screen_rect();
                    let is_one_to_one = ((vp_rect.width() * ppp) - frame.width as f32).abs() <= 3.0
                        && ((vp_rect.height() * ppp) - frame.height as f32).abs() <= 3.0;
                    let tex_opts = if is_one_to_one {
                        egui::TextureOptions::NEAREST
                    } else {
                        egui::TextureOptions::LINEAR
                    };

                    if let Some(tex) = self.mirror_tex.as_mut() {
                        tex.set(img, tex_opts);
                    } else {
                        self.mirror_tex = Some(ctx.load_texture("v3_mirror", img, tex_opts));
                    }
                }
            }
        }
    }

    /// Single-allocation BGRA/RGBA conversion (zero persistent scratch clone!).
    fn frame_to_color_image(frame: &Frame) -> Option<egui::ColorImage> {
        let bytes = frame.data.as_buffer()?;
        let count = (frame.width as usize).saturating_mul(frame.height as usize);
        if count == 0 || bytes.len() < count * 4 {
            return None;
        }

        let mut pixels = Vec::with_capacity(count);
        let is_rgba = matches!(frame.format, PixelFormat::RGBA8888);
        if is_rgba {
            for px in bytes.chunks_exact(4).take(count) {
                pixels.push(egui::Color32::from_rgba_unmultiplied(px[0], px[1], px[2], 255));
            }
        } else {
            for px in bytes.chunks_exact(4).take(count) {
                pixels.push(egui::Color32::from_rgba_unmultiplied(px[2], px[1], px[0], 255));
            }
        }

        Some(egui::ColorImage {
            size: [frame.width as usize, frame.height as usize],
            pixels,
        })
    }

    fn execute_initial_intent(&mut self, ctx: &egui::Context) {
        let Some(intent) = self.pending_launch_intent.take() else {
            return;
        };
        match intent {
            LaunchIntent::QuickHub => {
                if self.store.config.capture.auto_use_last_region {
                    if let Some(last) = self.store.last_region() {
                        self.start_live_session(
                            ctx,
                            last,
                            false,
                            None,
                            self.store.config.portal.enable_virtual_camera,
                            true,
                        );
                    }
                }
            }
            LaunchIntent::OpenSettings => {
                self.screen = AppScreen::Settings;
            }
            LaunchIntent::Share {
                region,
                native_portal,
                virtual_cam,
            } => {
                if let Some(r) = region {
                    self.start_live_session(ctx, r, false, None, virtual_cam, !virtual_cam);
                } else {
                    self.begin_selection(
                        ctx,
                        if virtual_cam {
                            PendingSelectionAction::VirtualCam
                        } else {
                            PendingSelectionAction::Share
                        },
                        native_portal,
                    );
                }
            }
            LaunchIntent::Record {
                region,
                output,
                native_portal,
                also_share,
            } => {
                if let Some(r) = region {
                    self.start_live_session(ctx, r, true, output, false, also_share);
                } else {
                    self.begin_selection(
                        ctx,
                        PendingSelectionAction::Record { output },
                        native_portal,
                    );
                }
            }
            LaunchIntent::Snapshot {
                region: _,
                output,
                to_clipboard,
            } => {
                self.begin_selection(
                    ctx,
                    PendingSelectionAction::Snapshot {
                        output,
                        to_clipboard,
                    },
                    false,
                );
            }
        }
    }
}

impl eframe::App for RegionApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.execute_initial_intent(ctx);
        self.handle_ipc_commands(ctx);
        self.process_ui_events(ctx);

        // Apply deferred viewport resize AFTER Wayland finishes exiting fullscreen
        if let Some(pending) = &self.pending_window_resize {
            if std::time::Instant::now() >= pending.ready_at {
                let pending = self.pending_window_resize.take().unwrap();
                if self.mirror_visible {
                    // Prefer exact physical frame dimensions if a live frame already arrived
                    let (w, h) = if let Some((fw, fh)) = self.live_frame_phys_size {
                        let ppp = ctx.pixels_per_point().max(1.0);
                        self.snapped_window_for_phys_size = Some((fw, fh));
                        Self::clamp_window_pts_preserving_aspect(fw as f32 / ppp, fh as f32 / ppp)
                    } else {
                        pending.target_pts
                    };
                    ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(
                        self.store.config.display.show_decorations,
                    ));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
                        120.0, 90.0,
                    )));
                    ctx.send_viewport_cmd(egui::ViewportCommand::MaxInnerSize(egui::vec2(
                        7680.0, 4320.0,
                    )));
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
                    if pending.send_to_background_after {
                        let delay = if DisplayServer::detect().is_wayland() { 220 } else { 120 };
                        self.pending_auto_background_at = Some(
                            std::time::Instant::now() + std::time::Duration::from_millis(delay),
                        );
                    }
                } else {
                    let size = hud_dimensions(&self.hud_state);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                        egui::WindowLevel::AlwaysOnTop,
                    ));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(size));
                    ctx.send_viewport_cmd(egui::ViewportCommand::MaxInnerSize(size));
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
                    self.last_root_hud_size = Some(size);
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(30));
            }
        }

        // Enforce fixed HUB_WINDOW_SIZE for Hub & Settings AFTER Wayland exits fullscreen/mirror!
        if matches!(self.screen, AppScreen::Hub | AppScreen::Settings) {
            let fixed = egui::vec2(HUB_WINDOW_SIZE[0], HUB_WINDOW_SIZE[1]);
            if let Some(lock_at) = self.pending_hub_lock_at {
                if std::time::Instant::now() >= lock_at {
                    self.pending_hub_lock_at = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(fixed));
                    ctx.send_viewport_cmd(egui::ViewportCommand::MaxInnerSize(fixed));
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(fixed));
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_millis(30));
                }
            } else if self.hub_size_retries > 0 {
                let cur = ctx.screen_rect().size();
                if (cur.x - fixed.x).abs() > 6.0 || (cur.y - fixed.y).abs() > 6.0 {
                    self.hub_size_retries -= 1;
                    self.pending_hub_lock_at = Some(
                        std::time::Instant::now() + std::time::Duration::from_millis(120),
                    );
                    ctx.request_repaint_after(std::time::Duration::from_millis(120));
                } else {
                    self.hub_size_retries = 0;
                }
            }
        }

        if let Some(bg_at) = self.pending_auto_background_at {
            if std::time::Instant::now() >= bg_at {
                self.pending_auto_background_at = None;
                self.hud_state.mirror_keep_below = true;
                let title = self.store.config.display.window_title.clone();
                if DisplayServer::detect().is_wayland() {
                    // Wayland keeps sharing minimized windows cleanly via PipeWire
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    self.runtime.spawn(async move {
                        let _ = set_window_minimized(&title, true).await;
                    });
                } else {
                    // X11/XComposite requires keeping the window mapped & lowered behind normal apps
                    self.runtime.spawn(async move {
                        let _ = set_window_keep_below(&title, true).await;
                    });
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(30));
            }
        }

        match self.screen {
            AppScreen::PreparingCapture => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(160.0);
                            // 30 FPS paced spinner (avoids unthrottled spin when vsync=false on Wayland)
                            let (spin_rect, _) = ui.allocate_exact_size(
                                egui::vec2(28.0, 28.0),
                                egui::Sense::hover(),
                            );
                            let t = ctx.input(|i| i.time) as f32;
                            let center = spin_rect.center();
                            let radius = 11.0_f32;
                            let start_angle = t * 5.0_f32;
                            let segments = 18_usize;
                            let arc_len = std::f32::consts::TAU * 0.7_f32;
                            let pts: Vec<egui::Pos2> = (0..=segments)
                                .map(|k| {
                                    let a = start_angle + arc_len * (k as f32 / segments as f32);
                                    egui::pos2(center.x + radius * a.cos(), center.y + radius * a.sin())
                                })
                                .collect();
                            ui.painter().add(egui::Shape::line(
                                pts,
                                egui::Stroke::new(2.5_f32, self.palette.accent_share),
                            ));
                            ctx.request_repaint_after(std::time::Duration::from_millis(33));
                            ui.add_space(12.0);
                            ui.label(
                                egui::RichText::new(t!("selector.loading"))
                                    .size(15.0)
                                    .color(self.palette.text_secondary),
                            );
                        });
                    });
                });
            }

            AppScreen::Selecting => {
                let mut outcome = None;
                egui::CentralPanel::default()
                    .frame(egui::Frame::none().fill(egui::Color32::BLACK))
                    .show(ctx, |ui| {
                        if let Some(tex) = &self.screenshot_tex {
                            outcome = self.selector.show(
                                ctx,
                                ui,
                                self.palette,
                                tex,
                                self.backend_screen_size,
                            );
                        }
                    });

                if let Some(out) = outcome {
                    self.screenshot_tex = None;
                    match out {
                        SelectorOutcome::Share(r) => {
                            self.start_live_session(
                                ctx,
                                r,
                                false,
                                None,
                                self.store.config.portal.enable_virtual_camera,
                                true,
                            );
                        }
                        SelectorOutcome::Record(r) => {
                            let out_path = match &self.pending_action {
                                PendingSelectionAction::Record { output, .. } => output.clone(),
                                _ => None,
                            };
                            // Live recording defaults to mirror-hidden (sleek floating REC pill only!)
                            self.start_live_session(ctx, r, true, out_path, false, false);
                        }
                        SelectorOutcome::ShareAndRecord(r) => {
                            self.start_live_session(ctx, r, true, None, false, true);
                        }
                        SelectorOutcome::VirtualCam(r) => {
                            // Virtual camera runs cleanly in background without cluttering screen with mirror!
                            self.start_live_session(ctx, r, false, None, true, false);
                        }
                        SelectorOutcome::SnapshotClipboard(r) => {
                            let (to_clip, out_path) = match &self.pending_action {
                                PendingSelectionAction::Snapshot { output, to_clipboard } => {
                                    (*to_clipboard, output.clone())
                                }
                                _ => (true, None),
                            };
                            self.capture_single_snapshot(ctx, r, to_clip, out_path);
                        }
                        SelectorOutcome::SavePreset { name, rect } => {
                            self.store.add_preset(
                                name.clone(),
                                rect,
                                self.selector.editor.aspect_ratio,
                                PresetAction::Share,
                            );
                            let _ = self.store.save();
                            self.sync_presets_to_ipc();
                            self.stop_live_session(ctx, true);
                            self.set_notice(format!("Saved preset \"{name}\""));
                        }
                        SelectorOutcome::Cancel => {
                            self.stop_live_session(ctx, true);
                        }
                    }
                }
            }

            AppScreen::Streaming => {
                if self.mirror_visible {
                    if self.logo_tex.is_none() {
                        if let Some(img) = embedded_logo_color_image() {
                            self.logo_tex = Some(ctx.load_texture(
                                "v3_brand_logo",
                                img,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }

                    let mut mirror_hovered = false;
                    egui::CentralPanel::default()
                        .frame(egui::Frame::none().inner_margin(egui::Margin::ZERO))
                        .show(ctx, |ui| {
                            ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 0.0);
                            let rect = ui.max_rect();
                            ui.painter().rect_filled(rect, 0.0, self.palette.bg_canvas);

                            if self.hud_state.is_paused {
                                if self.store.config.display.pause_mode == PauseScreenMode::Video {
                                    if let Some(vtex) = &self.pause_video_tex {
                                        let tex_size = vtex.size_vec2();
                                        let fitted = RegionSelectorView::fit_aspect_rect(
                                            rect,
                                            tex_size.x,
                                            tex_size.y,
                                        );
                                        ui.painter().rect_filled(
                                            rect,
                                            0.0,
                                            egui::Color32::from_rgb(13, 16, 23),
                                        );
                                        ui.put(fitted, egui::Image::new((vtex.id(), fitted.size())));
                                        render_pause_video_brand_badge(
                                            ui,
                                            rect,
                                            self.palette,
                                            self.logo_tex.as_ref(),
                                        );
                                    } else {
                                        render_pause_message_screen(
                                            ui,
                                            rect,
                                            self.palette,
                                            self.logo_tex.as_ref(),
                                            &self.store.config.display.pause_message,
                                        );
                                    }
                                } else {
                                    render_pause_message_screen(
                                        ui,
                                        rect,
                                        self.palette,
                                        self.logo_tex.as_ref(),
                                        &self.store.config.display.pause_message,
                                    );
                                }
                            } else if let Some(tex) = &self.mirror_tex {
                                // ALWAYS preserve exact aspect ratio (never squish horizontally/vertically!)
                                let tex_size = tex.size_vec2();
                                let fitted = RegionSelectorView::fit_aspect_rect(
                                    rect,
                                    tex_size.x,
                                    tex_size.y,
                                );
                                ui.put(fitted, egui::Image::new((tex.id(), fitted.size())));
                            }

                            // Borderless drag to move the mirror window freely
                            let drag_resp = ui.allocate_rect(rect, egui::Sense::drag());
                            mirror_hovered = drag_resp.hovered();
                            if drag_resp.drag_started() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                            }
                        });

                    // Render controls in Detached Floating Pill (zero artifacts in shared mirror!)
                    if let Some(hud_act) = render_hud(
                        ctx,
                        self.palette,
                        self.store.config.display.hud_mode,
                        &self.hud_state,
                        mirror_hovered,
                    ) {
                        self.handle_hud_action(ctx, hud_act);
                    }
                } else {
                    // Mirror-hidden mode: ZERO mirror window on screen!
                    // Root viewport IS the sleek floating HUD pill itself.
                    let desired = hud_dimensions(&self.hud_state);
                    if self.pending_window_resize.is_none()
                        && self.last_root_hud_size != Some(desired)
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(desired));
                        ctx.send_viewport_cmd(egui::ViewportCommand::MaxInnerSize(desired));
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(desired));
                        self.last_root_hud_size = Some(desired);
                    }

                    if let Some(hud_act) = render_embedded_hud(ctx, self.palette, &self.hud_state) {
                        self.handle_hud_action(ctx, hud_act);
                    }
                }

                // Gentle 250ms tick keeps REC timer & pulse smooth when static.
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }

            AppScreen::Settings => {
                let mut act = None;
                egui::CentralPanel::default().show(ctx, |ui| {
                    act = render_settings_view(
                        ui,
                        self.palette,
                        &mut self.store,
                        &mut self.settings_tab,
                        &self.enc_caps,
                        self.integration_feedback.as_deref(),
                    );
                });

                if let Some(a) = act {
                    match a {
                        SettingsAction::Close => {
                            self.screen = AppScreen::Hub;
                        }
                        SettingsAction::ClearRestoreToken => {
                            self.store.set_restore_token(None);
                            self.set_notice("Portal restore token cleared");
                        }
                        SettingsAction::InstallKdePlasmoid => {
                            self.integration_feedback = Some(
                                "Run `region-to-share integrate install-kde` to install the Plasma 6 Plasmoid."
                                    .to_string(),
                            );
                        }
                        SettingsAction::InstallGnomeExtension => {
                            self.integration_feedback = Some(
                                "Run `region-to-share integrate install-gnome` to install the GNOME Shell Extension."
                                    .to_string(),
                            );
                        }
                        SettingsAction::ThemeChanged => {
                            let dark = match self.store.config.ui.theme {
                                UiTheme::Auto => self.system_dark,
                                UiTheme::Dark => true,
                                UiTheme::Light => false,
                            };
                            self.palette = Palette::for_dark_mode(dark);
                            apply_theme(ctx, self.palette);
                            let _ = self.store.save();
                        }
                        SettingsAction::LanguageChanged => {
                            apply_locale(&self.store.config.ui.language);
                            let _ = self.store.save();
                        }
                        SettingsAction::BrowsePauseVideo => {
                            let start_dir = PathBuf::from(&self.store.config.display.pause_video_path)
                                .parent()
                                .map(PathBuf::from);
                            let ui_tx = self.ui_tx.clone();
                            let ctx_cl = ctx.clone();
                            self.runtime.spawn(async move {
                                let picked = tokio::task::spawn_blocking(move || {
                                    pick_video_file_blocking(start_dir.as_deref())
                                })
                                .await
                                .ok()
                                .flatten();
                                if let Some(path) = picked {
                                    let _ = ui_tx.send(UiEvent::PauseVideoPicked(path));
                                    ctx_cl.request_repaint();
                                }
                            });
                        }
                    }
                }
            }

            AppScreen::Hub => {
                self.render_hub_screen(ctx);
            }
        }
    }
}

impl RegionApp {
    fn render_hub_screen(&mut self, ctx: &egui::Context) {
        // Keyboard shortcuts on QuickHub
        if ctx.input(|i| i.key_pressed(egui::Key::S)) {
            self.begin_selection(ctx, PendingSelectionAction::Share, false);
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::R)) {
            self.begin_selection(
                ctx,
                PendingSelectionAction::Record { output: None },
                false,
            );
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::C)) {
            self.begin_selection(
                ctx,
                PendingSelectionAction::Snapshot {
                    output: None,
                    to_clipboard: true,
                },
                false,
            );
            return;
        }

        let ds = DisplayServer::detect();
        let de = DesktopEnvironment::detect();

        egui::CentralPanel::default().show(ctx, |ui| {
            // ── Top Brand Header ───────────────────────────────────────────────
            ui.horizontal(|ui| {
                let logo_rect = ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover()).0;
                ui.painter().rect_filled(
                    logo_rect,
                    egui::Rounding::same(10.0),
                    Palette::tint(self.palette.accent_share, 38),
                );
                paint_icon(ui.painter(), logo_rect, Icon::Share, self.palette.accent_share);

                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(t!("app.title"))
                            .size(17.0)
                            .strong()
                            .color(self.palette.text_primary),
                    );
                    ui.label(
                        egui::RichText::new(format!("v{} · {} · {}", env!("CARGO_PKG_VERSION"), ds.label(), de.label()))
                            .size(11.0)
                            .color(self.palette.text_muted),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if pill_button(ui, self.palette, Icon::Settings, "", false, self.palette.text_secondary)
                        .on_hover_text(t!("settings.title"))
                        .clicked()
                    {
                        self.screen = AppScreen::Settings;
                    }
                    let next_dark = !self.palette.dark;
                    if pill_button(
                        ui,
                        self.palette,
                        if self.palette.dark { Icon::Sun } else { Icon::Moon },
                        "",
                        false,
                        self.palette.accent_warn,
                    )
                    .clicked()
                    {
                        self.palette = Palette::for_dark_mode(next_dark);
                        self.store.config.ui.theme = if next_dark { UiTheme::Dark } else { UiTheme::Light };
                        apply_theme(ctx, self.palette);
                        let _ = self.store.save();
                    }
                });
            });

            ui.add_space(14.0);

            // ── Primary 3 Action Cards ─────────────────────────────────────────
            let avail_w = ui.available_width();
            let card_w = ((avail_w - 16.0) / 3.0).max(120.0);
            ui.horizontal(|ui| {
                if action_card(
                    ui,
                    self.palette,
                    Icon::Share,
                    &t!("hub.share_title"),
                    "Mirror window",
                    "S",
                    self.palette.accent_share,
                    card_w,
                )
                .clicked()
                {
                    self.begin_selection(ctx, PendingSelectionAction::Share, false);
                }

                if action_card(
                    ui,
                    self.palette,
                    Icon::Record,
                    &t!("hub.record_title"),
                    "Live MKV / MP4",
                    "R",
                    self.palette.accent_record,
                    card_w,
                )
                .clicked()
                {
                    self.begin_selection(
                        ctx,
                        PendingSelectionAction::Record { output: None },
                        false,
                    );
                }

                if action_card(
                    ui,
                    self.palette,
                    Icon::Camera,
                    &t!("hub.snapshot_title"),
                    "PNG / Clipboard",
                    "C",
                    self.palette.accent_live,
                    card_w,
                )
                .clicked()
                {
                    self.begin_selection(
                        ctx,
                        PendingSelectionAction::Snapshot {
                            output: None,
                            to_clipboard: true,
                        },
                        false,
                    );
                }
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            // ── Last Region Quick Card ─────────────────────────────────────────
            if let Some(last) = self.store.last_region() {
                egui::Frame::none()
                    .fill(self.palette.bg_surface)
                    .stroke(egui::Stroke::new(1.0_f32, self.palette.border_subtle))
                    .rounding(egui::Rounding::same(11.0))
                    .inner_margin(egui::Margin::symmetric(12.0, 9.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(t!("hub.last_region"))
                                        .size(12.0)
                                        .strong()
                                        .color(self.palette.text_primary),
                                );
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} × {} ({})  @ {}, {}",
                                        last.width,
                                        last.height,
                                        last.aspect_label(),
                                        last.x,
                                        last.y
                                    ))
                                    .monospace()
                                    .size(11.0)
                                    .color(self.palette.text_muted),
                                );
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if pill_button(
                                    ui,
                                    self.palette,
                                    Icon::Record,
                                    &t!("hub.reuse_record"),
                                    false,
                                    self.palette.accent_record,
                                )
                                .clicked()
                                {
                                    self.start_live_session(ctx, last, true, None, false, false);
                                }
                                if pill_button(
                                    ui,
                                    self.palette,
                                    Icon::Play,
                                    &t!("hub.reuse_share"),
                                    true,
                                    self.palette.accent_share,
                                )
                                .clicked()
                                {
                                    self.start_live_session(
                                        ctx,
                                        last,
                                        false,
                                        None,
                                        self.store.config.portal.enable_virtual_camera,
                                        true,
                                    );
                                }
                            });
                        });
                    });
                ui.add_space(10.0);
            }

            // ── Saved Presets List ─────────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(t!("hub.presets_title"))
                        .size(13.0)
                        .strong()
                        .color(self.palette.text_primary),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    status_badge(
                        ui,
                        self.palette,
                        if self.enc_caps.ffmpeg_available {
                            "FFmpeg Ready"
                        } else {
                            "No FFmpeg"
                        },
                        if self.enc_caps.ffmpeg_available {
                            self.palette.accent_live
                        } else {
                            self.palette.accent_warn
                        },
                        false,
                    );
                });
            });
            ui.add_space(4.0);

            let mut launch_preset: Option<(Rectangle, bool)> = None;
            let mut remove_preset_id: Option<String> = None;

            egui::ScrollArea::vertical()
                .max_height(155.0)
                .show(ui, |ui| {
                    if self.store.config.presets.is_empty() {
                        ui.label(
                            egui::RichText::new(t!("hub.presets_empty"))
                                .size(11.5)
                                .color(self.palette.text_muted),
                        );
                    } else {
                        for preset in &self.store.config.presets {
                            egui::Frame::none()
                                .fill(self.palette.bg_surface)
                                .stroke(egui::Stroke::new(1.0_f32, self.palette.border_subtle))
                                .rounding(egui::Rounding::same(10.0))
                                .inner_margin(egui::Margin::symmetric(10.0, 7.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(
                                                egui::RichText::new(&preset.name)
                                                    .strong()
                                                    .size(12.5)
                                                    .color(self.palette.text_primary),
                                            );
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{}×{} @ ({}, {})",
                                                    preset.region.width,
                                                    preset.region.height,
                                                    preset.region.x,
                                                    preset.region.y
                                                ))
                                                .monospace()
                                                .size(10.5)
                                                .color(self.palette.text_muted),
                                            );
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if pill_button(
                                                    ui,
                                                    self.palette,
                                                    Icon::Trash,
                                                    "",
                                                    false,
                                                    self.palette.accent_record,
                                                )
                                                .clicked()
                                                {
                                                    remove_preset_id = Some(preset.id.clone());
                                                }
                                                if pill_button(
                                                    ui,
                                                    self.palette,
                                                    Icon::Record,
                                                    "",
                                                    false,
                                                    self.palette.accent_record,
                                                )
                                                .clicked()
                                                {
                                                    launch_preset = Some((preset.region, true));
                                                }
                                                if pill_button(
                                                    ui,
                                                    self.palette,
                                                    Icon::Play,
                                                    "",
                                                    true,
                                                    self.palette.accent_share,
                                                )
                                                .clicked()
                                                {
                                                    launch_preset = Some((preset.region, false));
                                                }
                                            },
                                        );
                                    });
                                });
                            ui.add_space(4.0);
                        }
                    }
                });

            if let Some(id) = remove_preset_id {
                self.store.config.remove_preset(&id);
                let _ = self.store.save();
                self.sync_presets_to_ipc();
            }
            if let Some((region, rec)) = launch_preset {
                self.start_live_session(ctx, region, rec, None, false, !rec);
            }

            // ── Toast Notice & Footer Hint ─────────────────────────────────────
            if let Some((msg, at)) = &self.toast_notice {
                if at.elapsed() < std::time::Duration::from_secs(5) {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(msg)
                            .size(11.5)
                            .color(self.palette.accent_live),
                    );
                }
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(t!("hub.hint"))
                        .size(11.0)
                        .color(self.palette.text_muted),
                );
            });
        });
    }

    fn handle_hud_action(&mut self, ctx: &egui::Context, action: HudAction) {
        match action {
            HudAction::ToggleRecord => {
                if let Some(ref tx) = self.session_ctrl {
                    if self.hud_state.is_recording {
                        let _ = tx.send(SessionControl::StopRecord);
                    } else {
                        let _ = tx.send(SessionControl::StartRecord(None));
                    }
                }
            }
            HudAction::TogglePause => {
                let next = !self.hud_state.is_paused;
                self.set_stream_paused(next);
            }
            HudAction::Snapshot => {
                if let Some(ref tx) = self.session_ctrl {
                    let _ = tx.send(SessionControl::TakeSnapshot {
                        to_clipboard: true,
                        output: None,
                    });
                }
            }
            HudAction::ToggleKeepBelow => {
                let next = !self.hud_state.mirror_keep_below;
                self.hud_state.mirror_keep_below = next;
                let title = self.store.config.display.window_title.clone();
                if DisplayServer::detect().is_wayland() {
                    // Wayland compositors continue streaming minimized windows via PipeWire!
                    if next {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    } else {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    self.runtime.spawn(async move {
                        let _ = set_window_minimized(&title, next).await;
                    });
                } else {
                    // X11/XComposite stops updating minimized windows -> keep mapped behind normal apps.
                    self.runtime.spawn(async move {
                        let _ = set_window_keep_below(&title, next).await;
                    });
                }
            }
            HudAction::ToggleMirrorVisible => {
                let was_backgrounded = self.hud_state.mirror_keep_below;
                let next = !self.mirror_visible;
                self.mirror_visible = next;
                self.hud_state.mirror_visible = next;
                self.hud_state.mirror_keep_below = false;
                if was_backgrounded {
                    if !DisplayServer::detect().is_wayland() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    }
                    let title = self.store.config.display.window_title.clone();
                    self.runtime.spawn(async move {
                        let _ = bring_window_to_front(&title).await;
                    });
                }
                if next {
                    self.apply_root_as_mirror_geometry(ctx, false);
                } else {
                    self.mirror_tex = None;
                    self.apply_root_as_hud_geometry(ctx, false);
                }
            }
            HudAction::ToggleTools => {
                self.hud_state.show_tools = !self.hud_state.show_tools;
            }
            HudAction::Reselect => {
                // Recycle live Portal session deterministically so re-cropping NEVER re-asks Wayland permission!
                let was_recording = self.hud_state.is_recording;
                let was_vcam = self.hud_state.virtual_cam_active;
                let recycle_rx = if let Some(tx) = self.session_ctrl.take() {
                    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
                    let _ = tx.send(SessionControl::StopAndRecycle(done_tx));
                    Some(done_rx)
                } else {
                    None
                };
                self.mirror_tex = None;
                self.pause_video_tex = None;
                *self.latest_live_frame.lock() = None;
                *self.latest_pause_video_frame.lock() = None;
                self.pending_window_resize = None;
                self.pending_auto_background_at = None;
                self.hud_state.is_paused = false;
                self.hud_state.mirror_keep_below = false;
                self.hud_state.show_tools = false;
                if !DisplayServer::detect().is_wayland() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                }
                let next_action = if was_recording {
                    PendingSelectionAction::Record { output: None }
                } else if was_vcam && !self.mirror_visible {
                    PendingSelectionAction::VirtualCam
                } else {
                    PendingSelectionAction::Share
                };
                // Bring the window back to the front (un-minimizing / clearing keepBelow)
                // FIRST so the fullscreen selection overlay never opens underneath other windows!
                self.begin_selection_with_recycle(ctx, next_action, false, recycle_rx, true);
            }
            HudAction::ToggleVirtualCam => {
                let next = !self.hud_state.virtual_cam_active;
                self.hud_state.virtual_cam_active = next;
                if let Some(ref tx) = self.session_ctrl {
                    let _ = tx.send(SessionControl::ToggleVirtualCam(next));
                }
            }
            HudAction::ToggleStats => {
                self.hud_state.show_stats = !self.hud_state.show_stats;
                self.store.config.performance.show_performance_overlay = self.hud_state.show_stats;
                let _ = self.store.save();
            }
            HudAction::Stop => {
                self.stop_live_session(ctx, true);
            }
        }
    }

    fn capture_single_snapshot(
        &mut self,
        ctx: &egui::Context,
        region: Rectangle,
        to_clipboard: bool,
        output: Option<PathBuf>,
    ) {
        self.stop_live_session(ctx, true);
        let rt = Arc::clone(&self.runtime);
        let ui_tx = self.ui_tx.clone();
        let ctx_cl = ctx.clone();
        let backend_slot = Arc::clone(&self.reusable_backend);
        let token = self.store.restore_token().map(ToOwned::to_owned);

        rt.spawn(async move {
            let existing = backend_slot.lock().await.take();
            let mut backend: Box<dyn CaptureBackend> = if let Some(b) = existing {
                b
            } else if DisplayServer::detect().is_wayland() {
                #[cfg(target_os = "linux")]
                {
                    Box::new(PortalBackend::with_options(PortalBackendOptions {
                        restore_token: token.map(RestoreToken),
                        ..Default::default()
                    }))
                }
                #[cfg(not(target_os = "linux"))]
                {
                    return;
                }
            } else {
                match AutoBackend::new() {
                    Ok(b) => Box::new(b),
                    Err(e) => {
                        let _ = ui_tx.send(UiEvent::Error(e.to_string()));
                        return;
                    }
                }
            };

            if let Err(e) = backend.init(region).await {
                let _ = ui_tx.send(UiEvent::Error(e.to_string()));
                ctx_cl.request_repaint();
                return;
            }
            match backend.capture_frame().await {
                Ok(frame) => {
                    if let Some(bytes) = frame.data.as_buffer() {
                        if to_clipboard {
                            let _ = copy_frame_to_clipboard(
                                frame.width,
                                frame.height,
                                frame.format,
                                bytes,
                            );
                        }
                        let path = output.unwrap_or_else(|| {
                            let ts = region_core::format_local_timestamp("%Y%m%d-%H%M%S");
                            AppPaths::pictures_dir().join(format!("region-{ts}.png"))
                        });
                        match save_frame_png(&path, frame.width, frame.height, frame.format, bytes) {
                            Ok(()) => {
                                let _ = ui_tx.send(UiEvent::Notice(format!(
                                    "Snapshot saved to {} (copied to clipboard)",
                                    path.display()
                                )));
                            }
                            Err(e) => {
                                let _ = ui_tx.send(UiEvent::Error(e));
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = ui_tx.send(UiEvent::Error(e.to_string()));
                }
            }
            let _ = backend.stop().await;
            ctx_cl.request_repaint();
        });
    }
}

#[cfg(target_os = "linux")]
async fn emit_dbus_state(
    dbus_slot: &Arc<TokioMutex<Option<region_ipc::DbusServiceHandle>>>,
    state: SessionStateKind,
) {
    if let Some(ref handle) = *dbus_slot.lock().await {
        handle.emit_state_changed(state).await;
    }
}

#[cfg(target_os = "linux")]
async fn emit_dbus_recording_finished(
    dbus_slot: &Arc<TokioMutex<Option<region_ipc::DbusServiceHandle>>>,
    summary: &region_recorder::RecordingSummary,
) {
    if let Some(ref handle) = *dbus_slot.lock().await {
        handle
            .emit_recording_finished(
                &summary.output_label,
                summary.duration_secs.round() as u64,
                summary.final_bytes,
            )
            .await;
    }
}

/// Core async fan-out capture pipeline driving Preview + Live Recorder + PipeWire Virtual Camera.
#[allow(clippy::too_many_arguments)]
async fn run_capture_pipeline(
    initial_region: Rectangle,
    cfg: region_core::AppConfig,
    enc_caps: EncoderCapabilities,
    start_recording: bool,
    record_output: Option<PathBuf>,
    start_vcam: bool,
    backend_slot: Arc<TokioMutex<Option<Box<dyn CaptureBackend>>>>,
    mut ctrl_rx: UnboundedReceiver<SessionControl>,
    ui_tx: UnboundedSender<UiEvent>,
    latest_live_frame: Arc<FastMutex<Option<Arc<Frame>>>>,
    latest_pause_video_frame: Arc<FastMutex<Option<Arc<Frame>>>>,
    shared_status: Arc<RwLock<SessionStatusSnapshot>>,
    #[cfg(target_os = "linux")] dbus_slot: Arc<
        TokioMutex<Option<region_ipc::DbusServiceHandle>>,
    >,
    egui_ctx: egui::Context,
) {
    let region = Rectangle::new(
        initial_region.x.max(0),
        initial_region.y.max(0),
        initial_region.width.clamp(2, 16384),
        initial_region.height.clamp(2, 16384),
    );

    let existing = backend_slot.lock().await.take();
    let mut backend: Box<dyn CaptureBackend> = if let Some(b) = existing {
        b
    } else if DisplayServer::detect().is_wayland() {
        #[cfg(target_os = "linux")]
        {
            Box::new(PortalBackend::with_options(PortalBackendOptions {
                restore_token: cfg.portal.restore_token.clone().map(RestoreToken),
                include_cursor: cfg.capture.show_cursor,
                session_mode: PortalSessionMode::MonitorForOverlay,
                prefer_dmabuf: cfg.portal.prefer_dmabuf,
            }))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = ui_tx.send(UiEvent::Error("No Wayland backend on this OS".into()));
            return;
        }
    } else {
        match AutoBackend::new() {
            Ok(b) => Box::new(b),
            Err(e) => {
                let _ = ui_tx.send(UiEvent::Error(e.to_string()));
                return;
            }
        }
    };

    if let Err(e) = backend.init(region).await {
        let _ = ui_tx.send(UiEvent::Error(format!("Capture init failed: {e}")));
        egui_ctx.request_repaint();
        return;
    }
    if let Some(tok) = backend.restore_token_str() {
        let _ = ui_tx.send(UiEvent::RestoreToken(tok));
    }

    let backend_name = backend.name().await;
    let hub = FrameHub::new();
    let preview_sub = hub.subscribe("preview", 2);
    let _ = preview_sub; // UI receives via latest_live_frame mailbox for egui repaint sync

    let mut recorder: Option<FfmpegPipeRecorder> = None;
    let mut pending_start_record = if start_recording {
        Some(record_output)
    } else {
        None
    };

    #[cfg(target_os = "linux")]
    let mut vcam: Option<VirtualCameraSink> = None;
    let mut want_vcam = start_vcam;

    let target_fps = cfg.capture.frame_rate.clamp(1, 240);
    let frame_duration = std::time::Duration::from_secs_f64(1.0 / target_fps as f64);
    let mut profiler = FrameProfiler::new(30);
    let mut monitor = ResourceMonitor::new();
    let mut last_seq: u64 = 0;
    let mut last_frame: Option<Arc<Frame>> = None;
    let session_start = std::time::Instant::now();
    let mut last_telemetry_at = std::time::Instant::now();
    let mut last_dbus_state = if start_recording {
        SessionStateKind::SharingAndRecording
    } else {
        SessionStateKind::Sharing
    };

    {
        let mut st = shared_status.write();
        st.state = last_dbus_state;
        st.active_region = Some(region);
        st.backend_name = backend_name.clone();
    }
    #[cfg(target_os = "linux")]
    emit_dbus_state(&dbus_slot, last_dbus_state).await;

    let stop_flag = Arc::new(AtomicBool::new(false));
    let mut recycle_done_tx: Option<tokio::sync::oneshot::Sender<()>> = None;
    let mut stream_paused = false;
    let mut pause_video: Option<PauseVideoPlayer> = None;

    while !stop_flag.load(Ordering::Relaxed) {
        let loop_start = std::time::Instant::now();

        // Drain control commands immediately
        while let Ok(cmd) = ctrl_rx.try_recv() {
            match cmd {
                SessionControl::Stop => {
                    stop_flag.store(true, Ordering::Relaxed);
                }
                SessionControl::StopAndRecycle(done_tx) => {
                    recycle_done_tx = Some(done_tx);
                    stop_flag.store(true, Ordering::Relaxed);
                }
                SessionControl::UpdateRegion(r) => {
                    let _ = backend.update_region(r).await;
                    shared_status.write().active_region = Some(r);
                }
                SessionControl::StartRecord(out) => {
                    if recorder.is_none() {
                        pending_start_record = Some(out);
                    }
                }
                SessionControl::StopRecord => {
                    if let Some(mut rec) = recorder.take() {
                        if let Ok(sum) = rec.stop().await {
                            let _ = ui_tx.send(UiEvent::Notice(format!(
                                "Saved recording: {} ({:.1}s)",
                                sum.output_label, sum.duration_secs
                            )));
                            #[cfg(target_os = "linux")]
                            emit_dbus_recording_finished(&dbus_slot, &sum).await;
                        }
                        shared_status.write().output_file = None;
                    }
                }
                SessionControl::SetStreamPaused {
                    paused,
                    mode,
                    video_path,
                } => {
                    stream_paused = paused;
                    if let Some(mut pv) = pause_video.take() {
                        pv.stop();
                    }
                    *latest_pause_video_frame.lock() = None;

                    if paused {
                        if let Some(ref mut rec) = recorder {
                            let _ = rec.pause().await;
                        }
                        *latest_live_frame.lock() = None;

                        let (pw, ph) = last_frame
                            .as_ref()
                            .map(|f| (f.width, f.height))
                            .unwrap_or((region.width, region.height));

                        if mode == PauseScreenMode::Video && !video_path.trim().is_empty() {
                            match PauseVideoPlayer::start(
                                std::path::Path::new(video_path.trim()),
                                pw,
                                ph,
                                Arc::clone(&latest_pause_video_frame),
                                egui_ctx.clone(),
                            ) {
                                Ok(player) => {
                                    pause_video = Some(player);
                                }
                                Err(e) => {
                                    warn!("[Pipeline] {e} — falling back to branded pause message");
                                    let _ = ui_tx.send(UiEvent::Notice(e));
                                }
                            }
                        }

                        // If Virtual Camera is active and no video player started, push a clean
                        // branded logo card so webcam viewers immediately stop seeing desktop pixels.
                        #[cfg(target_os = "linux")]
                        if pause_video.is_none() {
                            if let Some(ref vc) = vcam {
                                vc.push_frame(Arc::new(build_pause_logo_frame(pw, ph)));
                            }
                        }
                    } else {
                        if let Some(ref mut rec) = recorder {
                            let _ = rec.resume().await;
                        }
                        // Instantly restore the last captured desktop frame on unpause (0ms on static screen!)
                        if let Some(ref seed) = last_frame {
                            *latest_live_frame.lock() = Some(Arc::clone(seed));
                            #[cfg(target_os = "linux")]
                            if let Some(ref vc) = vcam {
                                vc.push_frame(Arc::clone(seed));
                            }
                        }
                    }
                    egui_ctx.request_repaint();
                }
                SessionControl::ToggleVirtualCam(enable) => {
                    want_vcam = enable;
                    #[cfg(target_os = "linux")]
                    if !enable {
                        if let Some(mut vc) = vcam.take() {
                            vc.stop();
                        }
                        shared_status.write().virtual_cam_active = false;
                    }
                }
                SessionControl::TakeSnapshot {
                    to_clipboard,
                    output,
                } => {
                    // Instant 0ms snapshot from last captured frame (no stall on static Wayland screen!)
                    let snap_opt = match last_frame.clone() {
                        Some(f) => Some(f),
                        None => backend.capture_frame().await.ok().map(Arc::new),
                    };
                    if let Some(snap_frame) = snap_opt {
                        if let Some(bytes) = snap_frame.data.as_buffer() {
                            if to_clipboard {
                                let _ = copy_frame_to_clipboard(
                                    snap_frame.width,
                                    snap_frame.height,
                                    snap_frame.format,
                                    bytes,
                                );
                            }
                            let p = output.unwrap_or_else(|| {
                                let ts = region_core::format_local_timestamp("%Y%m%d-%H%M%S");
                                AppPaths::pictures_dir().join(format!("region-{ts}.png"))
                            });
                            if save_frame_png(
                                &p,
                                snap_frame.width,
                                snap_frame.height,
                                snap_frame.format,
                                bytes,
                            )
                            .is_ok()
                            {
                                let _ = ui_tx.send(UiEvent::Notice(format!(
                                    "Snapshot saved: {}",
                                    p.display()
                                )));
                                egui_ctx.request_repaint();
                            }
                        }
                    }
                }
            }
        }

        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        // While stream is paused: keep Portal / PipeWire backend 100% open & connected,
        // stop publishing desktop pixels, and forward looping pause video frames to Virtual Cam if active.
        if stream_paused {
            #[cfg(target_os = "linux")]
            if let (Some(ref vc), Some(ref pv)) = (&vcam, &pause_video) {
                if let Some(vf) = pv.take_vcam_frame() {
                    vc.push_frame(vf);
                }
            }
        } else {
            // Start live recorder / virtual cam immediately using `last_frame` even if screen is static!
            if let Some(ref seed) = last_frame {
                if let Some(out_opt) = pending_start_record.take() {
                    let mut rec = FfmpegPipeRecorder::with_capabilities(enc_caps.clone());
                    let path =
                        out_opt.unwrap_or_else(|| expand_output_path(&cfg.recording, None, None));
                    let job = RecordingJob::from_config(
                        &cfg.recording,
                        RecordingOutput::File(path.clone()),
                        seed.width,
                        seed.height,
                        target_fps,
                        seed.format,
                    );
                    match rec.start(job).await {
                        Ok(()) => {
                            info!("[Pipeline] Live recording started -> {}", path.display());
                            let _ = rec.push_frame(Arc::clone(seed));
                            shared_status.write().output_file = Some(path.display().to_string());
                            recorder = Some(rec);
                        }
                        Err(e) => {
                            let _ = ui_tx
                                .send(UiEvent::Notice(format!("Recording unavailable: {e}")));
                        }
                    }
                }

                #[cfg(target_os = "linux")]
                if want_vcam && vcam.is_none() {
                    match VirtualCameraSink::start(VirtualCameraConfig {
                        width: seed.width,
                        height: seed.height,
                        fps: target_fps,
                        ..Default::default()
                    }) {
                        Ok(sink) => {
                            let msg = if let Some(dev) = sink.v4l2_device() {
                                format!(
                                    "Virtual Camera active ({} + PipeWire) — ready in Chrome/Meet/OBS",
                                    dev.display()
                                )
                            } else {
                                "Virtual Camera active (PipeWire) — OBS/Firefox ready; Chrome needs v4l2loopback or pipewire-camera flag".into()
                            };
                            sink.push_frame(Arc::clone(seed));
                            vcam = Some(sink);
                            shared_status.write().virtual_cam_active = true;
                            let _ = ui_tx.send(UiEvent::Notice(msg));
                        }
                        Err(e) => {
                            want_vcam = false;
                            let _ = ui_tx.send(UiEvent::Notice(format!("Virtual camera: {e}")));
                        }
                    }
                }
            }

            profiler.start_frame();
            if let Ok(raw_frame) = backend.capture_frame().await {
                let frame = Arc::new(raw_frame);
                if frame.sequence != last_seq {
                    last_seq = frame.sequence;
                    last_frame = Some(Arc::clone(&frame));

                    // Start live recorder on first real frame if not started yet
                    if let Some(out_opt) = pending_start_record.take() {
                        let mut rec = FfmpegPipeRecorder::with_capabilities(enc_caps.clone());
                        let path =
                            out_opt.unwrap_or_else(|| expand_output_path(&cfg.recording, None, None));
                        let job = RecordingJob::from_config(
                            &cfg.recording,
                            RecordingOutput::File(path.clone()),
                            frame.width,
                            frame.height,
                            target_fps,
                            frame.format,
                        );
                        match rec.start(job).await {
                            Ok(()) => {
                                info!("[Pipeline] Live recording started -> {}", path.display());
                                shared_status.write().output_file =
                                    Some(path.display().to_string());
                                recorder = Some(rec);
                            }
                            Err(e) => {
                                let _ = ui_tx
                                    .send(UiEvent::Notice(format!("Recording unavailable: {e}")));
                            }
                        }
                    }

                    // Start PipeWire virtual camera lazily once real dimensions are known
                    #[cfg(target_os = "linux")]
                    if want_vcam && vcam.is_none() {
                        match VirtualCameraSink::start(VirtualCameraConfig {
                            width: frame.width,
                            height: frame.height,
                            fps: target_fps,
                            ..Default::default()
                        }) {
                            Ok(sink) => {
                                let msg = if let Some(dev) = sink.v4l2_device() {
                                    format!(
                                        "Virtual Camera active ({} + PipeWire) — ready in Chrome/Meet/OBS",
                                        dev.display()
                                    )
                                } else {
                                    "Virtual Camera active (PipeWire) — OBS/Firefox ready; Chrome needs v4l2loopback or pipewire-camera flag".into()
                                };
                                vcam = Some(sink);
                                shared_status.write().virtual_cam_active = true;
                                let _ = ui_tx.send(UiEvent::Notice(msg));
                            }
                            Err(e) => {
                                want_vcam = false;
                                let _ =
                                    ui_tx.send(UiEvent::Notice(format!("Virtual camera: {e}")));
                            }
                        }
                    }

                    // Fan-out zero-copy Arc<Frame> to all active outputs!
                    hub.publish(Arc::clone(&frame));

                    if let Some(ref rec) = recorder {
                        let _ = rec.push_frame(Arc::clone(&frame));
                    }
                    #[cfg(target_os = "linux")]
                    if let Some(ref vc) = vcam {
                        vc.push_frame(Arc::clone(&frame));
                    }

                    // Overwrite single latest-frame slot (O(1), zero unbounded RAM buildup!)
                    *latest_live_frame.lock() = Some(frame);
                    egui_ctx.request_repaint();
                }
            }
        }

        // Wall-clock telemetry tick every 250ms (keeps HUD REC timer, bytes & D-Bus live even on static screen!)
        if last_telemetry_at.elapsed() >= std::time::Duration::from_millis(250) {
            last_telemetry_at = std::time::Instant::now();
            let stats = profiler.stats();
            let (cpu, mem) = monitor.sample();
            let rec_stats = recorder.as_ref().map(|r| r.stats()).unwrap_or_default();

            let next_state = if stream_paused || (rec_stats.active && rec_stats.paused) {
                SessionStateKind::Paused
            } else if rec_stats.active {
                SessionStateKind::SharingAndRecording
            } else {
                SessionStateKind::Sharing
            };

            {
                let mut st = shared_status.write();
                st.current_fps = if stream_paused { 0.0 } else { stats.avg_fps };
                st.cpu_percent = cpu;
                st.memory_mb = mem;
                st.elapsed_seconds = if rec_stats.active {
                    rec_stats.elapsed_secs as u64
                } else {
                    session_start.elapsed().as_secs()
                };
                st.bytes_written = rec_stats.bytes_on_disk;
                st.state = next_state;
            }

            #[cfg(target_os = "linux")]
            if next_state != last_dbus_state {
                last_dbus_state = next_state;
                emit_dbus_state(&dbus_slot, next_state).await;
            }

            let _ = ui_tx.send(UiEvent::Telemetry {
                fps: if stream_paused { 0.0 } else { stats.avg_fps },
                cpu,
                mem_mb: mem,
                rec_active: rec_stats.active,
                rec_paused: rec_stats.paused,
                stream_paused,
                rec_elapsed: rec_stats.elapsed_secs as u64,
                rec_bytes: rec_stats.bytes_on_disk,
                encoder: rec_stats.encoder_name,
            });
            egui_ctx.request_repaint();
        }

        let elapsed = loop_start.elapsed();
        if elapsed < frame_duration {
            tokio::time::sleep(frame_duration - elapsed).await;
        }
    }

    // Graceful shutdown of all outputs
    if let Some(mut pv) = pause_video.take() {
        pv.stop();
    }
    *latest_pause_video_frame.lock() = None;
    if let Some(mut rec) = recorder.take() {
        if let Ok(sum) = rec.stop().await {
            let _ = ui_tx.send(UiEvent::Notice(format!(
                "Saved recording: {} ({:.1}s)",
                sum.output_label, sum.duration_secs
            )));
            #[cfg(target_os = "linux")]
            emit_dbus_recording_finished(&dbus_slot, &sum).await;
        }
    }
    #[cfg(target_os = "linux")]
    if let Some(mut vc) = vcam.take() {
        vc.stop();
    }
    let recycled = recycle_done_tx.is_some();
    if let Some(done_tx) = recycle_done_tx {
        *backend_slot.lock().await = Some(backend);
        let _ = done_tx.send(());
    } else {
        let _ = backend.stop().await;
    }

    {
        let mut st = shared_status.write();
        st.state = SessionStateKind::Idle;
        st.virtual_cam_active = false;
        st.output_file = None;
    }
    #[cfg(target_os = "linux")]
    emit_dbus_state(&dbus_slot, SessionStateKind::Idle).await;

    if !recycled {
        let _ = ui_tx.send(UiEvent::Stopped);
        egui_ctx.request_repaint();
    }
}
