//! Unified `region-to-share` v3 binary — GUI, live headless recorder, D-Bus daemon,
//! preset manager, KDE/GNOME integration installer, and system diagnostics (`doctor`).

mod integrate;

use clap::{Parser, Subcommand, ValueEnum};
use log::{error, info, warn};
use parking_lot::RwLock;
use region_capture::{AutoBackend, CaptureBackend};
use region_config::ConfigStore;
use region_core::{
    AspectRatio, AudioMode, ContainerFormat, HardwareEncoder, PresetAction, Rectangle,
    SessionStateKind, VideoCodec,
};
use region_ipc::{IpcCommand, IpcResponse, SessionStatusSnapshot};
use region_overlay::OverlayCapabilities;
use region_platform::{
    copy_frame_to_clipboard, save_frame_png, AppPaths, DesktopEnvironment, DisplayServer,
    PackageFormat,
};
use region_recorder::{
    expand_output_path, EncoderCapabilities, FfmpegPipeRecorder, RecorderBackend, RecordingJob,
    RecordingOutput,
};
use region_ui::{run_gui, LaunchIntent};
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use region_portal::{
    PortalBackend, PortalBackendOptions, PortalCapabilities, PortalSessionMode, RestoreToken,
};

/// Region to Share v3 — Screen region sharing, live recording, and virtual camera.
#[derive(Parser, Debug)]
#[command(name = "region-to-share", author, version, about)]
struct Cli {
    /// Enable debug logging for internal crates
    #[arg(short, long, global = true)]
    debug: bool,

    /// Enable full trace logging (including D-Bus / PipeWire / GUI dependencies)
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Internal flag used by `region-to-share daemon` to spawn the GUI without single-instance raise recursion
    #[arg(long, hide = true)]
    direct_gui: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Share a screen region in a clean mirror window and/or PipeWire Virtual Camera
    Share {
        /// Explicit region (`X,Y,WxH` or `WxH+X+Y`)
        #[arg(long)]
        region: Option<String>,
        /// Launch a saved preset by name or id
        #[arg(long)]
        preset: Option<String>,
        /// Use KDE Plasma 6's native portal "Rectangular Region" directly (zero double-select)
        #[arg(long)]
        native_portal: bool,
        /// Target frame rate (15..240)
        #[arg(long)]
        fps: Option<u32>,
        /// Publish the region as a native PipeWire Virtual Camera (`Video/Source`)
        #[arg(long)]
        virtual_cam: bool,
    },

    /// Record a screen region LIVE to disk (crash-safe MKV/fMP4/WebM/GIF) or stream (`-o -` / `rtmp://`)
    Record {
        /// Output file path, `-` for stdout live pipe, or URL (`rtmp://...`, `srt://...`)
        #[arg(short, long)]
        output: Option<String>,
        /// Explicit region (`X,Y,WxH` or `WxH+X+Y`). If omitted without --native-portal, opens GUI selector
        #[arg(long)]
        region: Option<String>,
        /// Use a saved preset by name or id
        #[arg(long)]
        preset: Option<String>,
        /// Use KDE Plasma 6 native portal region picker directly
        #[arg(long)]
        native_portal: bool,
        /// Container format (auto-inferred from extension if omitted)
        #[arg(long, value_enum)]
        format: Option<CliContainer>,
        /// Video codec preference
        #[arg(long, value_enum)]
        codec: Option<CliCodec>,
        /// Hardware acceleration preference
        #[arg(long, value_enum)]
        hw: Option<CliHw>,
        /// Quality CRF (lower = higher quality, default 20)
        #[arg(long)]
        crf: Option<u8>,
        /// Target FPS
        #[arg(long)]
        fps: Option<u32>,
        /// Audio capture mode
        #[arg(long, value_enum)]
        audio: Option<CliAudio>,
        /// Stop automatically after N seconds
        #[arg(long)]
        duration: Option<u64>,
        /// Countdown delay before recording starts (seconds)
        #[arg(long)]
        delay: Option<u64>,
        /// Also show the clean mirror window while recording live
        #[arg(long)]
        also_share: bool,
        /// Force pure headless CLI mode (no GUI window)
        #[arg(long)]
        headless: bool,
    },

    /// Capture a single crisp PNG snapshot (file and/or clipboard)
    Snapshot {
        /// Explicit region (`X,Y,WxH`)
        #[arg(long)]
        region: Option<String>,
        /// Saved preset name or id
        #[arg(long)]
        preset: Option<String>,
        /// Output PNG path
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Copy captured image straight to OS clipboard
        #[arg(long, default_value_t = true)]
        clipboard: bool,
    },

    /// Run background D-Bus (`io.github.solarpush.RegionToShare`) & IPC daemon for KDE/GNOME panels
    Daemon,

    /// Query live status of a running session (for panels, scripts, Waybar/Polybar)
    Status {
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Toggle live recording on/off in the active session
    ToggleRecord,

    /// Pause live recording in the active session
    Pause,

    /// Resume paused live recording
    Resume,

    /// Stop the active share/record session
    Stop,

    /// Live re-crop the active session without restarting (`X,Y,WxH`)
    SetRegion {
        /// New region coordinates (`X,Y,WxH`)
        region: String,
    },

    /// Manage saved 1-click region presets
    Presets {
        #[command(subcommand)]
        action: PresetCommands,
    },

    /// Install/manage native KDE Plasma 6 Plasmoid, GNOME Shell Extension, and D-Bus service
    Integrate {
        #[command(subcommand)]
        action: IntegrateCommands,
    },

    /// Print full diagnostic report (Wayland portal v6, KDE stream_region, Layer-Shell, PipeWire, FFmpeg/VA-API)
    Doctor {
        /// Output JSON report
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
enum PresetCommands {
    /// List all saved presets
    List {
        #[arg(long)]
        json: bool,
    },
    /// Add or update a preset (`region-to-share presets add "Presentation" 0,0,1920x1080`)
    Add {
        name: String,
        region: String,
        #[arg(long, value_enum, default_value_t = CliPresetAction::Share)]
        action: CliPresetAction,
    },
    /// Remove a preset by name or id
    Remove { name: String },
}

#[derive(Subcommand, Debug)]
enum IntegrateCommands {
    /// Install or upgrade the KDE Plasma 6 Plasmoid (`org.kde.plasma.regiontoshare`)
    InstallKde,
    /// Uninstall the KDE Plasma 6 Plasmoid
    UninstallKde,
    /// Install and enable the GNOME Shell Extension (`region-to-share@solarpush.github.io`)
    InstallGnome,
    /// Uninstall the GNOME Shell Extension
    UninstallGnome,
    /// Install D-Bus activation & systemd user service files for background KDE/GNOME panel control
    InstallDbus,
    /// Show installation status of KDE, GNOME, and D-Bus components
    Status,
}

#[derive(ValueEnum, Debug, Clone, Copy)]
enum CliContainer {
    Mkv,
    Mp4,
    Webm,
    Gif,
    Mpegts,
}
impl From<CliContainer> for ContainerFormat {
    fn from(v: CliContainer) -> Self {
        match v {
            CliContainer::Mkv => Self::Mkv,
            CliContainer::Mp4 => Self::Mp4,
            CliContainer::Webm => Self::Webm,
            CliContainer::Gif => Self::Gif,
            CliContainer::Mpegts => Self::MpegTs,
        }
    }
}

#[derive(ValueEnum, Debug, Clone, Copy)]
enum CliCodec {
    Auto,
    H264,
    Hevc,
    Av1,
    Vp9,
}
impl From<CliCodec> for VideoCodec {
    fn from(v: CliCodec) -> Self {
        match v {
            CliCodec::Auto => Self::Auto,
            CliCodec::H264 => Self::H264,
            CliCodec::Hevc => Self::Hevc,
            CliCodec::Av1 => Self::Av1,
            CliCodec::Vp9 => Self::Vp9,
        }
    }
}

#[derive(ValueEnum, Debug, Clone, Copy)]
enum CliHw {
    Auto,
    Vaapi,
    Nvenc,
    Cpu,
}
impl From<CliHw> for HardwareEncoder {
    fn from(v: CliHw) -> Self {
        match v {
            CliHw::Auto => Self::Auto,
            CliHw::Vaapi => Self::Vaapi,
            CliHw::Nvenc => Self::Nvenc,
            CliHw::Cpu => Self::Cpu,
        }
    }
}

#[derive(ValueEnum, Debug, Clone, Copy)]
enum CliAudio {
    None,
    Desktop,
    Mic,
    Both,
}
impl From<CliAudio> for AudioMode {
    fn from(v: CliAudio) -> Self {
        match v {
            CliAudio::None => Self::None,
            CliAudio::Desktop => Self::Desktop,
            CliAudio::Mic => Self::Mic,
            CliAudio::Both => Self::Both,
        }
    }
}

#[derive(ValueEnum, Debug, Clone, Copy)]
enum CliPresetAction {
    Share,
    Record,
    Vcam,
}
impl From<CliPresetAction> for PresetAction {
    fn from(v: CliPresetAction) -> Self {
        match v {
            CliPresetAction::Share => Self::Share,
            CliPresetAction::Record => Self::Record,
            CliPresetAction::Vcam => Self::VirtualCamera,
        }
    }
}

fn init_logging(debug: bool, verbose: bool) {
    let mut builder = env_logger::Builder::new();
    if verbose {
        builder.filter_level(log::LevelFilter::Trace);
    } else if debug {
        builder.filter_level(log::LevelFilter::Warn);
        for name in [
            "region_to_share",
            "region_cli",
            "region_ui",
            "region_portal",
            "region_capture",
            "region_core",
            "region_config",
            "region_recorder",
            "region_overlay",
            "region_ipc",
            "region_platform",
        ] {
            builder.filter_module(name, log::LevelFilter::Debug);
        }
    } else {
        builder.filter_level(log::LevelFilter::Warn);
    }
    builder.format_timestamp_millis().init();
}

fn resolve_target_region(
    store: &ConfigStore,
    region: Option<&str>,
    preset: Option<&str>,
) -> Result<Option<Rectangle>, String> {
    if let Some(p) = preset {
        let found = store
            .config
            .find_preset(p)
            .ok_or_else(|| format!("Preset '{p}' not found. Run `region-to-share presets list`."))?;
        return Ok(Some(found.region));
    }
    if let Some(r) = region {
        let parsed = r.parse::<Rectangle>()?;
        return Ok(Some(parsed));
    }
    Ok(None)
}

fn main() {
    let cli = Cli::parse();
    init_logging(cli.debug, cli.verbose);

    let mut store = ConfigStore::load_default();
    let runtime = tokio::runtime::Runtime::new().expect("Failed to initialize Tokio runtime");

    let exit_code = match cli.command {
        None => {
            // Single-instance check: raise an already-open GUI instance (but NEVER recurse into `daemon`!)
            let existing_gui = !cli.direct_gui
                && matches!(
                    region_ipc::socket::send_socket_command(&IpcCommand::GetStatus),
                    Ok(IpcResponse::Status(ref s)) if s.backend_name != "daemon"
                );
            if existing_gui && region_ipc::socket::send_socket_command(&IpcCommand::Raise).is_ok() {
                info!("Raised existing Region to Share instance.");
                0
            } else if let Err(e) = run_gui(runtime, store, LaunchIntent::QuickHub) {
                error!("GUI error: {e}");
                1
            } else {
                0
            }
        }

        Some(Commands::Share {
            region,
            preset,
            native_portal,
            fps,
            virtual_cam,
        }) => {
            if let Some(f) = fps {
                store.config.capture.frame_rate = f.clamp(1, 240);
            }
            match resolve_target_region(&store, region.as_deref(), preset.as_deref()) {
                Ok(resolved) => {
                    let intent = LaunchIntent::Share {
                        region: resolved,
                        native_portal,
                        virtual_cam,
                    };
                    if let Err(e) = run_gui(runtime, store, intent) {
                        eprintln!("Error: {e}");
                        1
                    } else {
                        0
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    2
                }
            }
        }

        Some(Commands::Record {
            output,
            region,
            preset,
            native_portal,
            format,
            codec,
            hw,
            crf,
            fps,
            audio,
            duration,
            delay,
            also_share,
            headless,
        }) => {
            if let Some(f) = fps {
                store.config.capture.frame_rate = f.clamp(1, 240);
            }
            if let Some(fmt) = format {
                store.config.recording.container = fmt.into();
            }
            if let Some(c) = codec {
                store.config.recording.codec = c.into();
            }
            if let Some(h) = hw {
                store.config.recording.hw_accel = h.into();
            }
            if let Some(q) = crf {
                store.config.recording.quality_crf = q.min(51);
            }
            if let Some(a) = audio {
                store.config.recording.audio_mode = a.into();
            }

            let resolved = match resolve_target_region(&store, region.as_deref(), preset.as_deref())
            {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(2);
                }
            };

            let run_headless = headless
                || (!also_share && (resolved.is_some() || native_portal))
                || matches!(output.as_deref(), Some("-"));

            if run_headless {
                let res = runtime.block_on(run_headless_record(
                    &mut store,
                    resolved,
                    native_portal,
                    output,
                    duration,
                    delay,
                ));
                match res {
                    Ok(()) => 0,
                    Err(e) => {
                        eprintln!("Recording error: {e}");
                        1
                    }
                }
            } else {
                let out_path = output.map(PathBuf::from);
                let intent = LaunchIntent::Record {
                    region: resolved,
                    output: out_path,
                    native_portal,
                    also_share,
                };
                if let Err(e) = run_gui(runtime, store, intent) {
                    eprintln!("Error: {e}");
                    1
                } else {
                    0
                }
            }
        }

        Some(Commands::Snapshot {
            region,
            preset,
            output,
            clipboard,
        }) => {
            let resolved = match resolve_target_region(&store, region.as_deref(), preset.as_deref())
            {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(2);
                }
            };
            if let Some(r) = resolved {
                match runtime.block_on(run_headless_snapshot(&mut store, r, output, clipboard)) {
                    Ok(path) => {
                        println!("Snapshot saved to {}", path.display());
                        0
                    }
                    Err(e) => {
                        eprintln!("Snapshot error: {e}");
                        1
                    }
                }
            } else if let Err(e) = run_gui(
                runtime,
                store,
                LaunchIntent::Snapshot {
                    region: None,
                    output,
                    to_clipboard: clipboard,
                },
            ) {
                eprintln!("Error: {e}");
                1
            } else {
                0
            }
        }

        Some(Commands::Daemon) => match runtime.block_on(run_daemon(store)) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("Daemon error: {e}");
                1
            }
        },

        Some(Commands::Status { json }) => {
            let snap = runtime.block_on(query_live_status());
            match snap {
                Ok(s) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&s).unwrap_or_default());
                    } else {
                        println!(
                            "State: {} | FPS: {:.1} | Elapsed: {}s | Written: {} B | Backend: {}",
                            s.state.as_dbus_str(),
                            s.current_fps,
                            s.elapsed_seconds,
                            s.bytes_written,
                            s.backend_name
                        );
                    }
                    0
                }
                Err(_) => {
                    let idle = SessionStatusSnapshot::default();
                    if json {
                        println!("{}", serde_json::to_string_pretty(&idle).unwrap_or_default());
                    } else {
                        println!("State: idle (no active instance)");
                    }
                    0
                }
            }
        }

        Some(Commands::ToggleRecord) => {
            dispatch_remote(&runtime, &IpcCommand::ToggleRecord)
        }
        Some(Commands::Pause) => dispatch_remote(&runtime, &IpcCommand::Pause),
        Some(Commands::Resume) => dispatch_remote(&runtime, &IpcCommand::Resume),
        Some(Commands::Stop) => dispatch_remote(&runtime, &IpcCommand::Stop),
        Some(Commands::SetRegion { region }) => match region.parse::<Rectangle>() {
            Ok(r) => dispatch_remote(&runtime, &IpcCommand::SetRegion { region: r }),
            Err(e) => {
                eprintln!("Invalid region: {e}");
                2
            }
        },

        Some(Commands::Presets { action }) => match action {
            PresetCommands::List { json } => {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&store.config.presets).unwrap_or_default()
                    );
                } else if store.config.presets.is_empty() {
                    println!("No saved presets.");
                } else {
                    for p in &store.config.presets {
                        println!(
                            "• {:<20}  {}  ({} · {:?} · {}fps)",
                            p.name,
                            p.region,
                            p.region.aspect_label(),
                            p.default_action,
                            p.fps
                        );
                    }
                }
                0
            }
            PresetCommands::Add {
                name,
                region,
                action,
            } => match region.parse::<Rectangle>() {
                Ok(rect) => {
                    store.add_preset(name.clone(), rect, AspectRatio::Free, action.into());
                    if let Err(e) = store.save() {
                        eprintln!("Failed to save config: {e}");
                        1
                    } else {
                        println!("Saved preset '{name}' ({rect}).");
                        0
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    2
                }
            },
            PresetCommands::Remove { name } => {
                if store.config.remove_preset(&name) {
                    let _ = store.save();
                    println!("Removed preset '{name}'.");
                    0
                } else {
                    eprintln!("Preset '{name}' not found.");
                    1
                }
            }
        },

        Some(Commands::Integrate { action }) => match action {
            IntegrateCommands::InstallKde => integrate::install_kde(),
            IntegrateCommands::UninstallKde => integrate::uninstall_kde(),
            IntegrateCommands::InstallGnome => integrate::install_gnome(),
            IntegrateCommands::UninstallGnome => integrate::uninstall_gnome(),
            IntegrateCommands::InstallDbus => integrate::install_dbus_and_systemd(),
            IntegrateCommands::Status => integrate::print_status(),
        },

        Some(Commands::Doctor { json }) => {
            runtime.block_on(run_doctor(&store, json));
            0
        }
    };

    std::process::exit(exit_code);
}

async fn query_live_status() -> Result<SessionStatusSnapshot, String> {
    #[cfg(target_os = "linux")]
    if let Ok(client) = region_ipc::DbusClient::connect().await {
        if let Ok(status) = client.get_status().await {
            return Ok(status);
        }
    }
    match region_ipc::socket::send_socket_command(&IpcCommand::GetStatus)? {
        IpcResponse::Status(s) => Ok(s),
        _ => Err("Unexpected IPC response".into()),
    }
}

fn dispatch_remote(runtime: &tokio::runtime::Runtime, cmd: &IpcCommand) -> i32 {
    #[cfg(target_os = "linux")]
    {
        let res = runtime.block_on(async {
            let client = region_ipc::DbusClient::connect().await?;
            client.send_command(cmd).await
        });
        if let Ok(msg) = res {
            println!("{msg}");
            return 0;
        }
    }

    match region_ipc::socket::send_socket_command(cmd) {
        Ok(IpcResponse::Ok { message }) => {
            println!("{message}");
            0
        }
        Ok(IpcResponse::Status(s)) => {
            println!("{}", serde_json::to_string_pretty(&s).unwrap_or_default());
            0
        }
        Ok(IpcResponse::Error { message }) => {
            eprintln!("Error: {message}");
            1
        }
        Err(e) => {
            eprintln!("No active Region to Share session found ({e}).");
            1
        }
    }
}

/// Pure headless CLI live recording (writes continuously to disk / stdout / RTMP with Ctrl+C flush).
async fn run_headless_record(
    store: &mut ConfigStore,
    region: Option<Rectangle>,
    native_portal: bool,
    output_arg: Option<String>,
    duration: Option<u64>,
    delay: Option<u64>,
) -> Result<(), String> {
    if let Some(secs) = delay {
        for rem in (1..=secs).rev() {
            eprint!("\rStarting live recording in {rem}s... ");
            let _ = std::io::stderr().flush();
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        eprintln!("\rStarting live recording now!       ");
    }

    let is_wayland = DisplayServer::detect().is_wayland();
    let mut backend: Box<dyn CaptureBackend> = if is_wayland {
        #[cfg(target_os = "linux")]
        {
            let mode = if native_portal || region.is_none() {
                PortalSessionMode::NativeDirect
            } else {
                PortalSessionMode::MonitorForOverlay
            };
            Box::new(PortalBackend::with_options(PortalBackendOptions {
                restore_token: store.restore_token().map(|t| RestoreToken(t.to_string())),
                include_cursor: store.config.capture.show_cursor,
                session_mode: mode,
                prefer_dmabuf: store.config.portal.prefer_dmabuf,
            }))
        }
        #[cfg(not(target_os = "linux"))]
        {
            return Err("Wayland backend unavailable on this OS".into());
        }
    } else {
        Box::new(AutoBackend::new().map_err(|e| e.to_string())?)
    };

    let init_rect = region.unwrap_or_else(|| Rectangle::new(0, 0, 1920, 1080));
    backend.init(init_rect).await.map_err(|e| e.to_string())?;

    if let Some(tok) = backend.restore_token_str() {
        store.set_restore_token(Some(tok));
    }

    let first_frame = backend.capture_frame().await.map_err(|e| e.to_string())?;
    let output = match output_arg.as_deref() {
        Some("-") => RecordingOutput::Stdout,
        Some(u) if u.starts_with("rtmp://") || u.starts_with("srt://") || u.starts_with("udp://") => {
            RecordingOutput::Url(u.to_string())
        }
        Some(path) => RecordingOutput::File(PathBuf::from(path)),
        None => RecordingOutput::File(expand_output_path(&store.config.recording, None, None)),
    };

    let target_fps = store.config.capture.frame_rate.clamp(1, 240);
    let mut job = RecordingJob::from_config(
        &store.config.recording,
        output.clone(),
        first_frame.width,
        first_frame.height,
        target_fps,
        first_frame.format,
    );
    job.max_duration = duration.map(Duration::from_secs);

    let mut recorder = FfmpegPipeRecorder::new();
    recorder.start(job).await.map_err(|e| e.to_string())?;
    let _ = recorder.push_frame(Arc::new(first_frame));

    eprintln!(
        "● Recording LIVE -> {} ({}fps, press Ctrl+C to finish cleanly)",
        output.display_label(),
        target_fps
    );

    let frame_dur = Duration::from_secs_f64(1.0 / target_fps as f64);
    let started = Instant::now();
    let max_dur = duration.map(Duration::from_secs);
    let mut last_print = Instant::now();

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        if let Some(limit) = max_dur {
            if started.elapsed() >= limit {
                break;
            }
        }

        tokio::select! {
            _ = &mut ctrl_c => {
                eprintln!("\nCtrl+C received — flushing live stream safely...");
                break;
            }
            frame_res = backend.capture_frame() => {
                let got_frame = if let Ok(frame) = frame_res {
                    if let Err(e) = recorder.push_frame(Arc::new(frame)) {
                        warn!("Recorder write ended: {e}");
                        break;
                    }
                    true
                } else {
                    false
                };
                if last_print.elapsed() >= Duration::from_millis(500) {
                    let st = recorder.stats();
                    eprint!(
                        "\r● REC {:02}:{:02} | {} frames ({} dropped) | {:.1} MB | {}   ",
                        (st.elapsed_secs as u64) / 60,
                        (st.elapsed_secs as u64) % 60,
                        st.frames_written,
                        st.frames_dropped,
                        st.bytes_on_disk as f64 / (1024.0 * 1024.0),
                        st.encoder_name
                    );
                    let _ = std::io::stderr().flush();
                    last_print = Instant::now();
                }
                if got_frame {
                    tokio::time::sleep(frame_dur / 2).await;
                }
            }
        }
    }

    let summary = recorder.stop().await.map_err(|e| e.to_string())?;
    let _ = backend.stop().await;

    eprintln!(
        "\n✓ Saved {} ({:.1}s, {} frames, {:.2} MB, encoder={})",
        summary.output_label,
        summary.duration_secs,
        summary.frames_written,
        summary.final_bytes as f64 / (1024.0 * 1024.0),
        summary.encoder_name
    );

    Ok(())
}

async fn run_headless_snapshot(
    store: &mut ConfigStore,
    region: Rectangle,
    output: Option<PathBuf>,
    clipboard: bool,
) -> Result<PathBuf, String> {
    let mut backend: Box<dyn CaptureBackend> = if DisplayServer::detect().is_wayland() {
        #[cfg(target_os = "linux")]
        {
            Box::new(PortalBackend::with_options(PortalBackendOptions {
                restore_token: store.restore_token().map(|t| RestoreToken(t.to_string())),
                ..Default::default()
            }))
        }
        #[cfg(not(target_os = "linux"))]
        {
            return Err("Wayland backend unavailable".into());
        }
    } else {
        Box::new(AutoBackend::new().map_err(|e| e.to_string())?)
    };

    backend.init(region).await.map_err(|e| e.to_string())?;
    if let Some(tok) = backend.restore_token_str() {
        store.set_restore_token(Some(tok));
    }
    let frame = backend.capture_frame().await.map_err(|e| e.to_string())?;
    let _ = backend.stop().await;

    let bytes = frame
        .data
        .as_buffer()
        .ok_or_else(|| "Snapshot buffer unavailable".to_string())?;
    if clipboard {
        let _ = copy_frame_to_clipboard(frame.width, frame.height, frame.format, bytes);
    }
    let path = output.unwrap_or_else(|| {
        let ts = region_core::format_local_timestamp("%Y%m%d-%H%M%S");
        AppPaths::pictures_dir().join(format!("region-{ts}.png"))
    });
    save_frame_png(&path, frame.width, frame.height, frame.format, bytes)?;
    Ok(path)
}

/// Lightweight background D-Bus daemon spawned by systemd or D-Bus activation.
/// Dispatches GUI/recording actions triggered from the KDE Plasma 6 Plasmoid or GNOME Shell Extension.
async fn run_daemon(store: ConfigStore) -> Result<(), String> {
    let shared_status = Arc::new(RwLock::new(SessionStatusSnapshot {
        state: SessionStateKind::Idle,
        backend_name: "daemon".into(),
        presets: store
            .config
            .presets
            .iter()
            .map(|p| region_ipc::PresetDbusItem {
                id: p.id.clone(),
                name: p.name.clone(),
                x: p.region.x,
                y: p.region.y,
                width: p.region.width,
                height: p.region.height,
            })
            .collect(),
        ..Default::default()
    }));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<IpcCommand>();
    region_ipc::socket::spawn_socket_server(tx.clone(), Arc::clone(&shared_status), None);

    #[cfg(target_os = "linux")]
    let _dbus = region_ipc::DbusServiceHandle::start(
        Arc::clone(&shared_status),
        tx.clone(),
        Arc::new(|| {}),
    )
    .await
    .map_err(|e| format!("Failed to register D-Bus daemon: {e}"))?;

    info!("Region to Share D-Bus daemon active (io.github.solarpush.RegionToShare).");

    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("region-to-share"));
    while let Some(cmd) = rx.recv().await {
        match cmd {
            IpcCommand::Raise => {
                let _ = std::process::Command::new(&exe).arg("--direct-gui").spawn();
            }
            IpcCommand::StartShare {
                native_portal,
                preset_or_region,
                virtual_cam,
            } => {
                let mut c = std::process::Command::new(&exe);
                c.arg("share");
                if native_portal {
                    c.arg("--native-portal");
                }
                if virtual_cam {
                    c.arg("--virtual-cam");
                }
                if let Some(target) = preset_or_region {
                    if target.parse::<Rectangle>().is_ok() {
                        c.args(["--region", &target]);
                    } else {
                        c.args(["--preset", &target]);
                    }
                }
                let _ = c.spawn();
            }
            IpcCommand::StartRecord {
                native_portal,
                preset_or_region,
                output_path,
                also_share,
            } => {
                let mut c = std::process::Command::new(&exe);
                c.arg("record");
                if native_portal {
                    c.arg("--native-portal");
                }
                if also_share {
                    c.arg("--also-share");
                }
                if let Some(out) = output_path {
                    c.args(["-o", &out]);
                }
                if let Some(target) = preset_or_region {
                    if target.parse::<Rectangle>().is_ok() {
                        c.args(["--region", &target]);
                    } else {
                        c.args(["--preset", &target]);
                    }
                }
                let _ = c.spawn();
            }
            IpcCommand::TakeSnapshot {
                to_clipboard,
                output_path,
            } => {
                let mut c = std::process::Command::new(&exe);
                c.arg("snapshot");
                if !to_clipboard {
                    c.arg("--clipboard=false");
                }
                if let Some(out) = output_path {
                    c.args(["-o", &out]);
                }
                let _ = c.spawn();
            }
            _ => {}
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct DoctorReport {
    version: &'static str,
    display_server: DisplayServer,
    desktop_environment: DesktopEnvironment,
    package_format: PackageFormat,
    pipewire_feature_compiled: bool,
    #[cfg(target_os = "linux")]
    portal: PortalCapabilities,
    overlay: OverlayCapabilities,
    encoders: EncoderCapabilities,
    config_path: String,
    has_restore_token: bool,
    presets_count: usize,
    kde_plasmoid_installed: bool,
    gnome_extension_installed: bool,
}

async fn run_doctor(store: &ConfigStore, json: bool) {
    #[cfg(target_os = "linux")]
    let portal = PortalCapabilities::probe().await;
    let overlay = OverlayCapabilities::probe();
    let encoders = EncoderCapabilities::probe();
    let (kde_ok, gnome_ok) = integrate::check_installed();

    let report = DoctorReport {
        version: env!("CARGO_PKG_VERSION"),
        display_server: DisplayServer::detect(),
        desktop_environment: DesktopEnvironment::detect(),
        package_format: PackageFormat::detect(),
        pipewire_feature_compiled: cfg!(feature = "wayland-pipewire"),
        #[cfg(target_os = "linux")]
        portal,
        overlay,
        encoders,
        config_path: store.config_path().display().to_string(),
        has_restore_token: store.restore_token().is_some(),
        presets_count: store.config.presets.len(),
        kde_plasmoid_installed: kde_ok,
        gnome_extension_installed: gnome_ok,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
        return;
    }

    println!("Region to Share v{} — System Doctor", report.version);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  Display Server        : {}", report.display_server.label());
    println!("  Desktop Environment   : {}", report.desktop_environment.label());
    println!("  Package Format        : {:?}", report.package_format);
    println!("  PipeWire Compiled     : {}", if report.pipewire_feature_compiled { "yes (native)" } else { "no (rebuild with pipewire-devel)" });
    #[cfg(target_os = "linux")]
    {
        println!(
            "  XDG ScreenCast Portal : {} (v{}, Monitor={}, Window={}, Virtual={})",
            if report.portal.available { "available" } else { "unavailable" },
            report.portal.version,
            report.portal.supports_monitor,
            report.portal.supports_window,
            report.portal.supports_virtual
        );
        println!(
            "  KDE Native Region     : {}",
            if report.portal.supports_native_region_picker {
                "supported (KWin zkde_screencast_unstable_v1::stream_region)"
            } else {
                "not active (using Pro Spotlight Overlay)"
            }
        );
    }
    println!(
        "  Wayland Layer-Shell   : {}",
        if report.overlay.wlr_layer_shell_available {
            "supported (zwlr_layer_shell_v1)"
        } else {
            "not advertised"
        }
    );
    println!(
        "  FFmpeg Live Recorder  : {}",
        report
            .encoders
            .ffmpeg_version
            .as_deref()
            .unwrap_or("NOT FOUND — install ffmpeg")
    );
    println!(
        "  VA-API Render Device  : {}",
        report
            .encoders
            .vaapi_render_node
            .as_deref()
            .unwrap_or("none")
    );
    println!(
        "  Available Encoders    : {}",
        if report.encoders.encoders.is_empty() {
            "none".into()
        } else {
            report.encoders.encoders.join(", ")
        }
    );
    println!("  Config Path           : {}", report.config_path);
    println!(
        "  Portal Restore Token  : {}",
        if report.has_restore_token { "saved" } else { "none" }
    );
    println!("  Saved Presets         : {}", report.presets_count);
    println!(
        "  Virtual Cam (v4l2)    : {}",
        region_platform::detect_v4l2loopback_device()
            .map(|p| format!("{} (Chrome/Meet/Discord zero-flag ready!)", p.display()))
            .unwrap_or_else(|| {
                "not loaded (PipeWire Video/Source only; install v4l2loopback for zero-flag Chrome/Meet)".into()
            })
    );
    println!(
        "  KDE Plasma 6 Plasmoid : {}",
        if report.kde_plasmoid_installed { "installed" } else { "not installed (run `region-to-share integrate install-kde`)" }
    );
    println!(
        "  GNOME Shell Extension : {}",
        if report.gnome_extension_installed { "installed" } else { "not installed (run `region-to-share integrate install-gnome`)" }
    );
}
