use eframe::egui;
use region_core::{Rectangle, PixelFormat};
use region_core::performance::FrameProfiler;
use region_capture::{CaptureBackend, AutoBackend};
use region_portal::PortalBackend;
use region_config::Config;
use std::sync::Arc;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex as TokioMutex;
use std::sync::Mutex as StdMutex;
use std::fs;
use rayon::prelude::*;
use clap::Parser;
use log::{debug, info, error, trace};
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

const DEFAULT_WINDOW_SIZE: [f32; 2] = [400.0, 350.0];

/// Region to Share - Screen region capture and streaming tool
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
    
    /// Enable verbose/trace logging (more detailed than debug)
    #[arg(short, long)]
    verbose: bool,
}

/// Check if running under Wayland
fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok() 
        || std::env::var("XDG_SESSION_TYPE").map(|t| t == "wayland").unwrap_or(false)
}

fn main() -> Result<(), eframe::Error> {
    let args = Args::parse();
    
    // Initialize logger based on flags
    let log_level = if args.verbose {
        log::LevelFilter::Trace
    } else if args.debug {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Warn
    };
    
    env_logger::Builder::new()
        .filter_level(log_level)
        .format_timestamp_millis()
        .init();

    // Load config first so we can honour the saved language preference
    let config = Config::new();

    // Locale priority: saved setting > env var > "en"
    let lang = if !config.settings.language.is_empty() {
        config.settings.language.clone()
    } else {
        let locale = sys_locale::get_locale().unwrap_or_else(|| String::from("en"));
        locale
            .split(|c: char| c == '-' || c == '_')
            .next()
            .unwrap_or("en")
            .to_owned()
    };
    rust_i18n::set_locale(&lang);

    info!("Region to Share v{}", env!("CARGO_PKG_VERSION"));
    debug!("Debug logging enabled");
    debug!("Session type: {}", if is_wayland() { "Wayland" } else { "X11" });
    
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(DEFAULT_WINDOW_SIZE)
            .with_min_inner_size(DEFAULT_WINDOW_SIZE)
            .with_title("Region to Share")
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "Region to Share",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(RegionApp::new(runtime, config)))
        }),
    )
}

struct RegionApp {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    capturing: bool,
    frames_captured: u32,
    current_fps: f64,
    current_capture_ms: f64,
    current_frame_ms: f64,
    texture: Option<egui::TextureHandle>,
    stats_rx: Arc<StdMutex<UnboundedReceiver<StatsUpdate>>>,
    stop_tx: Arc<StdMutex<Option<UnboundedSender<()>>>>,
    runtime: Arc<tokio::runtime::Runtime>,
    selecting_region: bool,
    selection_start: Option<egui::Pos2>,
    selection_end: Option<egui::Pos2>,
    screenshot_texture: Option<egui::TextureHandle>,
    selection_ready: bool,
    config: Config,
    streaming_only: bool,  // Mode streaming pur (sans UI)
    screenshot_display_rect: Option<egui::Rect>,  // Rectangle où le screenshot est affiché
    pending_resize: Option<(u32, u32)>,  // Redimensionnement en attente (width, height)
    resize_frame_count: u32,  // Compteur pour attendre quelques frames avant resize
    show_streaming_options: bool,  // Afficher la modal d'options pendant le streaming
    pending_send_to_background: bool,  // Envoi en arrière-plan après resize
    background_delay_frames: u32,  // Délai en frames avant minimisation
    // Backend Portal réutilisable (garde la session ouverte)
    portal_backend: Arc<TokioMutex<Option<Box<dyn CaptureBackend>>>>,
    // Stats système
    cpu_usage: f64,
    memory_mb: f64,
    data_rate_mbps: f64,
}

#[derive(Clone)]
struct FrameData {
    width: u32,
    height: u32,
    data: Arc<Vec<u8>>,  // Arc pour éviter les copies
    format: PixelFormat,
}

enum StatsUpdate {
    Progress { 
        frames: u32, 
        fps: f64, 
        capture_ms: f64, 
        frame_ms: f64,
        cpu_percent: f64,
        memory_mb: f64,
        data_rate_mbps: f64,
    },
    Frame(FrameData),
    Screenshot(FrameData),
    Stopped,
}

impl RegionApp {
    fn new(runtime: tokio::runtime::Runtime, config: Config) -> Self {
        let (_tx, rx) = mpsc::unbounded_channel();
        
        // Charger la dernière région si disponible
        let (x, y, width, height) = if let Some(last_region) = config.get_last_region() {
            (last_region.x, last_region.y, last_region.width, last_region.height)
        } else {
            (560, 240, 800, 600)
        };
        
        // Vérifier si on doit auto-démarrer avec la dernière région
        let auto_start = config.settings.auto_use_specific_region 
            && config.settings.remember_last_region 
            && config.get_last_region().is_some();
        
        // Capturer avant de move config
        let auto_send_bg = config.settings.auto_send_to_background;
        
        Self {
            x,
            y,
            width,
            height,
            capturing: false,
            frames_captured: 0,
            current_fps: 0.0,
            current_capture_ms: 0.0,
            current_frame_ms: 0.0,
            texture: None,
            stats_rx: Arc::new(StdMutex::new(rx)),
            stop_tx: Arc::new(StdMutex::new(None)),
            runtime: Arc::new(runtime),
            selecting_region: false,
            selection_start: None,
            selection_end: None,
            screenshot_texture: None,
            selection_ready: false,
            config,
            streaming_only: auto_start,  // Auto-démarrer si configuré
            screenshot_display_rect: None,
            pending_resize: if auto_start { Some((width, height)) } else { None },
            resize_frame_count: 0,
            show_streaming_options: false,
            pending_send_to_background: auto_start && auto_send_bg,
            background_delay_frames: 0,
            portal_backend: Arc::new(TokioMutex::new(None)),
            cpu_usage: 0.0,
            memory_mb: 0.0,
            data_rate_mbps: 0.0,
        }
    }
    
    /// Detect if running on Wayland.
    fn is_wayland() -> bool {
        std::env::var("WAYLAND_DISPLAY").is_ok() 
            || std::env::var("XDG_SESSION_TYPE").map(|t| t == "wayland").unwrap_or(false)
    }
    
    /// Lower window on X11 using x11rb directly (no external tools needed for snap).
    fn lower_window_x11() {
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::{ConnectionExt, ConfigureWindowAux, StackMode};
        
        // Connecter à X11
        let (conn, screen_num) = match x11rb::connect(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        
        let screen = &conn.setup().roots[screen_num];
        
        // Obtenir la fenêtre active via _NET_ACTIVE_WINDOW
        let active_atom = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW").ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| reply.atom);
        
        if let Some(atom) = active_atom {
            if let Ok(reply) = conn.get_property(false, screen.root, atom, x11rb::NONE, 0, 1) {
                if let Ok(prop) = reply.reply() {
                    if prop.length > 0 {
                        if let Some(window_id) = prop.value32().and_then(|mut iter| iter.next()) {
                            // Envoyer la fenêtre en bas de la pile (lower)
                            let aux = ConfigureWindowAux::new().stack_mode(StackMode::BELOW);
                            if conn.configure_window(window_id, &aux).is_ok() {
                                let _ = conn.flush();
                                return;
                            }
                        }
                    }
                }
            }
        }
    }
    
    /// Send window to background (X11) or minimize (Wayland).
    fn send_to_background(&self, ctx: &egui::Context) {
        if Self::is_wayland() {
            // On Wayland, minimize the window (background capture not possible)
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        } else {
            // On X11, lower the window
            Self::lower_window_x11();
        }
    }
    
    fn start_capture(&mut self) {
        self.capturing = true;
        self.frames_captured = 0;
        self.texture = None;
        
        let region = Rectangle {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        };
        
        let frame_rate = self.config.settings.frame_rate;
        
        let (new_tx, new_rx) = mpsc::unbounded_channel();
        *self.stats_rx.lock().unwrap() = new_rx;
        
        let (stop_tx, stop_rx) = mpsc::unbounded_channel();
        *self.stop_tx.lock().unwrap() = Some(stop_tx);
        
        let runtime = self.runtime.clone();
        let portal_backend_arc = self.portal_backend.clone();
        
        runtime.spawn(async move {
            // Récupérer le backend Portal existant s'il y en a un
            let existing_backend = portal_backend_arc.lock().await.take();
            
            if let Err(e) = capture_task_continuous(region, frame_rate, new_tx, stop_rx, existing_backend).await {
                error!("[Capture] Error: {}", e);
            }
        });
    }
    
    fn stop_capture(&mut self) {
        if let Some(tx) = self.stop_tx.lock().unwrap().take() {
            let _ = tx.send(());
        }
        self.capturing = false;
    }
    
    fn start_region_selection(&mut self, ctx: &egui::Context) {
        self.selecting_region = true;
        self.selection_start = None;
        self.selection_end = None;
        self.screenshot_texture = None;
        self.selection_ready = false;
        
        // NE PAS passer en plein écran maintenant
        // On le fera APRÈS avoir reçu le screenshot (quand l'utilisateur a accepté le partage)
        // La fenêtre reste normale pour l'instant
        
        // Créer un nouveau channel pour la capture du screenshot
        let (new_tx, new_rx) = mpsc::unbounded_channel();
        *self.stats_rx.lock().unwrap() = new_rx;
        
        let runtime = self.runtime.clone();
        let ctx_clone = ctx.clone();
        let use_wayland = is_wayland();
        let portal_backend_arc = self.portal_backend.clone();
        
        runtime.spawn(async move {
            // Create the appropriate backend for the display server
            let mut backend: Box<dyn CaptureBackend> = if use_wayland {
                Box::new(PortalBackend::new())
            } else {
                match AutoBackend::new() {
                    Ok(b) => Box::new(b),
                    Err(_) => return,
                }
            };
            
            // Initialize the backend with a full-screen region first (needed for Portal)
            let init_region = Rectangle::new(0, 0, 1920, 1080);
            if backend.init(init_region).await.is_err() {
                return;
            }
            
            // Obtenir la taille réelle de l'écran via le backend
            let (screen_width, screen_height) = backend.get_screen_size().await
                .unwrap_or((1920, 1080));
            let _ = (screen_width, screen_height); // Used for reference
            
            // Capturer tout l'écran
            if let Ok(frame) = backend.capture_screenshot().await {
                if let Some(arc_buffer) = frame.data.as_arc_buffer() {
                    let frame_data = FrameData {
                        width: frame.width,
                        height: frame.height,
                        data: arc_buffer,
                        format: frame.format,
                    };
                    let _ = new_tx.send(StatsUpdate::Screenshot(frame_data));
                    
                    // Stocker le backend pour réutilisation (évite un nouveau dialogue Portal)
                    if use_wayland {
                        *portal_backend_arc.lock().await = Some(backend);
                    }
                    
                    ctx_clone.request_repaint();
                }
            }
        });
    }
    
    fn apply_selection(&mut self, ctx: &egui::Context) {
        if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
            // Obtenir les dimensions réelles du screenshot
            let (screenshot_width, screenshot_height) = if let Some(texture) = &self.screenshot_texture {
                (texture.size()[0] as f32, texture.size()[1] as f32)
            } else {
                return;
            };
            
            // Obtenir le rectangle où le screenshot a été affiché
            let display_rect = if let Some(rect) = self.screenshot_display_rect {
                rect
            } else {
                return;
            };
            
            // Obtenir le facteur de scaling DPI
            let pixels_per_point = ctx.pixels_per_point();
            let _ = pixels_per_point; // Used for DPI calculations
            
            // Calculer le ratio de scaling exact
            let scale_x = screenshot_width / display_rect.width();
            let scale_y = screenshot_height / display_rect.height();
            
            // Convertir les coordonnées de sélection vers les coordonnées réelles
            let rel_min_x = (start.x.min(end.x) - display_rect.min.x).max(0.0);
            let rel_min_y = (start.y.min(end.y) - display_rect.min.y).max(0.0);
            let rel_max_x = (start.x.max(end.x) - display_rect.min.x).min(display_rect.width());
            let rel_max_y = (start.y.max(end.y) - display_rect.min.y).min(display_rect.height());
            
            // Appliquer le scaling pour obtenir les pixels réels
            let min_x = (rel_min_x * scale_x).round();
            let min_y = (rel_min_y * scale_y).round();
            let max_x = (rel_max_x * scale_x).round();
            let max_y = (rel_max_y * scale_y).round();
            
            // Calculer les dimensions de la région sélectionnée
            let region_width = (max_x - min_x).max(1.0);
            let region_height = (max_y - min_y).max(1.0);
            
            self.x = min_x as i32;
            self.y = min_y as i32;
            self.width = region_width as u32;
            self.height = region_height as u32;
            
            // Sauvegarder la région si l'option est activée
            if self.config.settings.remember_last_region {
                self.config.set_last_region(self.x, self.y, self.width, self.height);
                let _ = self.config.save();
            }
            
            // Sortir du plein écran
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
            
            // Marquer le redimensionnement en attente
            self.pending_resize = Some((self.width, self.height));
            self.resize_frame_count = 0;
            
            ctx.request_repaint();
        }
        
        self.selecting_region = false;
        self.screenshot_texture = None;
        self.selection_start = None;
        self.selection_end = None;
        self.selection_ready = false;
        
        // Programmer l'envoi en arrière-plan après le resize si l'option est activée
        if self.config.settings.auto_send_to_background {
            self.pending_send_to_background = true;
        }
        
        // Activer le mode streaming - le stream sera démarré après le resize
        self.streaming_only = true;
    }
    
    fn process_updates(&mut self, ctx: &egui::Context) {
        // Collecter les updates et ne garder que la dernière frame
        let mut last_frame: Option<FrameData> = None;
        let mut last_screenshot: Option<FrameData> = None;
        
        {
            let mut rx = self.stats_rx.lock().unwrap();
            while let Ok(update) = rx.try_recv() {
                match update {
                    StatsUpdate::Progress { frames, fps, capture_ms, frame_ms, cpu_percent, memory_mb, data_rate_mbps } => {
                        self.frames_captured = frames;
                        self.current_fps = fps;
                        self.current_capture_ms = capture_ms;
                        self.current_frame_ms = frame_ms;
                        self.cpu_usage = cpu_percent;
                        self.memory_mb = memory_mb;
                        self.data_rate_mbps = data_rate_mbps;
                    }
                    StatsUpdate::Frame(frame_data) => {
                        // Garder seulement la dernière frame (skip les anciennes)
                        last_frame = Some(frame_data);
                    }
                    StatsUpdate::Screenshot(frame_data) => {
                        last_screenshot = Some(frame_data);
                    }
                    StatsUpdate::Stopped => {
                        self.capturing = false;
                    }
                }
            }
        }
        
        // Traiter seulement la dernière frame (optimisation majeure)
        if let Some(frame_data) = last_frame {
            if let Ok(image) = self.frame_to_image(&frame_data) {
                // Réutiliser la texture si possible, sinon en créer une nouvelle
                self.texture = Some(ctx.load_texture(
                    "capture",
                    image,
                    egui::TextureOptions::NEAREST, // Plus rapide que LINEAR
                ));
            }
        }
        
        // Traiter le screenshot
        if let Some(frame_data) = last_screenshot {
            if self.selecting_region && self.screenshot_texture.is_none() {
                if let Ok(image) = self.frame_to_image(&frame_data) {
                    self.screenshot_texture = Some(ctx.load_texture(
                        "screenshot",
                        image,
                        Default::default(),
                    ));
                    self.selection_ready = true;
                    
                    // Passer en plein écran pour afficher le screenshot
                    ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                }
            }
        }
    }
    
    fn frame_to_image(&self, frame_data: &FrameData) -> Result<egui::ColorImage, String> {
        let pixel_count = (frame_data.width * frame_data.height) as usize;
        let data = &*frame_data.data;
        
        // Vérifier que nous avons assez de données
        let expected_bytes = pixel_count * 4;
        if data.len() < expected_bytes {
            return Err(format!("Not enough data: got {} bytes, expected {}", data.len(), expected_bytes));
        }
        
        // Check if we need to convert BGRA -> RGBA or if already in RGBA
        let is_rgba = matches!(frame_data.format, PixelFormat::RGBA8888);
        
        // Optimisation: conversion parallèle BGRA -> RGBA avec rayon (si nécessaire)
        let pixels: Vec<egui::Color32> = if is_rgba {
            // Already in RGBA format, just copy
            if pixel_count > 500_000 {
                data.par_chunks_exact(4)
                    .take(pixel_count)
                    .map(|rgba| {
                        egui::Color32::from_rgba_unmultiplied(
                            rgba[0], rgba[1], rgba[2], 255,
                        )
                    })
                    .collect()
            } else {
                data.chunks_exact(4)
                    .take(pixel_count)
                    .map(|rgba| {
                        egui::Color32::from_rgba_unmultiplied(
                            rgba[0], rgba[1], rgba[2], 255,
                        )
                    })
                    .collect()
            }
        } else {
            // Need to convert BGRA -> RGBA
            if pixel_count > 500_000 {
                data.par_chunks_exact(4)
                    .take(pixel_count)
                    .map(|bgra| {
                        egui::Color32::from_rgba_unmultiplied(
                            bgra[2], bgra[1], bgra[0], 255,
                        )
                    })
                    .collect()
            } else {
                data.chunks_exact(4)
                    .take(pixel_count)
                    .map(|bgra| {
                        egui::Color32::from_rgba_unmultiplied(
                            bgra[2], bgra[1], bgra[0], 255,
                        )
                    })
                    .collect()
            }
        };
        
        Ok(egui::ColorImage {
            size: [frame_data.width as usize, frame_data.height as usize],
            pixels,
        })
    }
}

impl eframe::App for RegionApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_updates(ctx);
        
        // Traiter le redimensionnement différé
        if let Some((target_width, target_height)) = self.pending_resize {
            self.resize_frame_count += 1;
            
            // Attendre plus de frames sur Wayland pour laisser le window manager
            // traiter la sortie du plein écran
            let frames_to_wait = if is_wayland() { 15 } else { 5 };
            
            if self.resize_frame_count >= frames_to_wait {
                let window_width = target_width as f32;
                let window_height = target_height as f32;
                
                // Sur Wayland, envoyer plusieurs fois la commande de taille
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                    egui::vec2(window_width, window_height)
                ));
                
                self.pending_resize = None;
                
                // Démarrer le stream maintenant que la fenêtre est redimensionnée
                if !self.capturing {
                    self.start_capture();
                }
                
                // Programmer la minimisation APRES un délai pour que le resize soit appliqué
                if self.pending_send_to_background {
                    self.background_delay_frames = 1;
                }
            }
            
            ctx.request_repaint();
        }
        
        // Traiter la minimisation différée (après le resize)
        if self.pending_send_to_background && self.pending_resize.is_none() {
            self.background_delay_frames += 1;
            // Attendre 20 frames sur Wayland, 10 sur X11
            let frames_to_wait = if is_wayland() { 20 } else { 10 };
            
            if self.background_delay_frames >= frames_to_wait {
                self.pending_send_to_background = false;
                self.background_delay_frames = 0;
                self.send_to_background(ctx);
            }
            ctx.request_repaint();
        }
        
        // Mode sélection - fenêtre plein écran avec screenshot
        if self.selecting_region {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(egui::Color32::BLACK))
                .show(ctx, |ui| {
                    if !self.selection_ready {
                        ui.vertical_centered(|ui| {
                            ui.add_space(300.0);
                            ui.heading(t!("selection.loading"));
                            ui.spinner();
                        });
                        return;
                    }
                    
                    // Afficher le screenshot en fond
                    if let Some(screenshot) = &self.screenshot_texture {
                        let img_size = screenshot.size_vec2();
                        let screen_rect = ui.max_rect();
                        
                        // IMPORTANT: On affiche l'image en l'étirant pour remplir TOUT le screen_rect
                        self.screenshot_display_rect = Some(screen_rect);
                        let _ = img_size; // Used for scaling calculations
                        
                        // Afficher l'image étirée pour remplir tout l'écran
                        ui.put(
                            screen_rect,
                            egui::Image::new((screenshot.id(), screen_rect.size()))
                        );
                    }
                    
                    // Overlay semi-transparent
                    ui.painter().rect_filled(
                        ui.max_rect(),
                        0.0,
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 100),
                    );
                    
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
                        ui.heading(t!("selection.title"));
                        ui.label(t!("selection.hint"));
                    });
                    
                    // Zone de sélection
                    let rect = ui.max_rect();
                    let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
                    
                    if response.drag_started() {
                        if let Some(pos) = response.interact_pointer_pos() {
                            self.selection_start = Some(pos);
                        }
                    }
                    
                    if response.dragged() {
                        if let Some(pos) = response.interact_pointer_pos() {
                            self.selection_end = Some(pos);
                        }
                    }
                    
                    if response.drag_stopped() || ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.apply_selection(ctx);
                    }
                    
                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        // Annuler la sélection et revenir en mode normal
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(DEFAULT_WINDOW_SIZE[0], DEFAULT_WINDOW_SIZE[1])));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
                        self.selecting_region = false;
                        self.screenshot_texture = None;
                    }
                    
                    // Dessiner la sélection
                    if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
                        let sel_rect = egui::Rect::from_two_pos(start, end);

                        let w = sel_rect.width().abs();
                        let h = sel_rect.height().abs();
                        let too_small = w < DEFAULT_WINDOW_SIZE[0] || h < DEFAULT_WINDOW_SIZE[1];

                        let (stroke_color, fill_color) = if too_small {
                            (
                                egui::Color32::from_rgb(255, 60, 60),
                                egui::Color32::from_rgba_unmultiplied(255, 60, 60, 60),
                            )
                        } else {
                            (
                                egui::Color32::from_rgb(0, 255, 0),
                                egui::Color32::from_rgba_unmultiplied(0, 255, 0, 60),
                            )
                        };

                        ui.painter().rect_stroke(
                            sel_rect,
                            0.0,
                            egui::Stroke::new(3.0, stroke_color),
                        );
                        ui.painter().rect_filled(
                            sel_rect,
                            0.0,
                            fill_color,
                        );

                        let wi = w as i32;
                        let hi = h as i32;
                        let x = sel_rect.min.x as i32;
                        let y = sel_rect.min.y as i32;

                        // Info en haut à gauche de la sélection
                        let text_pos = egui::Pos2::new(sel_rect.min.x, sel_rect.min.y - 25.0);
                        let label = if too_small {
                            format!("{}x{} — min {}x{} (Result is too small and resizing is required with deformation)", wi, hi,
                                DEFAULT_WINDOW_SIZE[0] as i32, DEFAULT_WINDOW_SIZE[1] as i32)
                        } else {
                            format!("{}x{} at ({}, {})", wi, hi, x, y)
                        };
                        ui.painter().text(
                            text_pos,
                            egui::Align2::LEFT_BOTTOM,
                            label,
                            egui::FontId::proportional(18.0),
                            if too_small { egui::Color32::from_rgb(255, 120, 120) } else { egui::Color32::WHITE },
                        );
                    }
                });
            return;
        }
        
        // Mode streaming pur - afficher uniquement l'image sans UI
        if self.streaming_only && self.capturing {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().inner_margin(egui::Margin::ZERO))
                .show(ctx, |ui| {
                    // Supprimer tout espacement dans l'UI
                    ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 0.0);
                    
                    if let Some(texture) = &self.texture {
                        // Utiliser TOUT le rect disponible sans aucun padding
                        let ui_rect = ui.max_rect();
                        
                        // Utiliser ui.put() pour placer l'image exactement dans le rect
                        ui.put(
                            ui_rect,
                            egui::Image::new((texture.id(), ui_rect.size()))
                        );
                    }
                    
                    // Boutons en haut à droite (options + arrière-plan)
                    let button_size = egui::vec2(28.0, 28.0);
                    let spacing = 5.0;
                    
                    // Bouton arrière-plan (↓) - seulement sur X11
                    if !Self::is_wayland() {
                        let bg_button_pos = egui::pos2(
                            ui.max_rect().right() - button_size.x * 2.0 - spacing * 2.0 - 5.0,
                            ui.max_rect().top() + 5.0
                        );
                        let bg_button_rect = egui::Rect::from_min_size(bg_button_pos, button_size);
                        
                        let bg_response = ui.allocate_rect(bg_button_rect, egui::Sense::click());
                        
                        let bg_color = if bg_response.hovered() {
                            egui::Color32::from_rgba_unmultiplied(60, 100, 60, 200)
                        } else {
                            egui::Color32::from_rgba_unmultiplied(40, 80, 40, 150)
                        };
                        ui.painter().rect_filled(bg_button_rect, 4.0, bg_color);
                        ui.painter().text(
                            bg_button_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "↓",
                            egui::FontId::proportional(16.0),
                            egui::Color32::WHITE,
                        );
                        
                        if bg_response.clicked() {
                            Self::lower_window_x11();
                        }
                    }
                    
                    // Bouton options (⚙)
                    let button_pos = egui::pos2(ui.max_rect().right() - button_size.x - 5.0, ui.max_rect().top() + 5.0);
                    let button_rect = egui::Rect::from_min_size(button_pos, button_size);
                    
                    let response = ui.allocate_rect(button_rect, egui::Sense::click());
                    
                    // Dessiner le bouton
                    let bg_color = if response.hovered() {
                        egui::Color32::from_rgba_unmultiplied(60, 60, 60, 200)
                    } else {
                        egui::Color32::from_rgba_unmultiplied(40, 40, 40, 150)
                    };
                    ui.painter().rect_filled(button_rect, 4.0, bg_color);
                    ui.painter().text(
                        button_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "⚙",
                        egui::FontId::proportional(16.0),
                        egui::Color32::WHITE,
                    );
                    
                    if response.clicked() {
                        self.show_streaming_options = !self.show_streaming_options;
                    }
                });
            
            // Modal d'options pendant le streaming
            if self.show_streaming_options {
                egui::Window::new(t!("streaming_options.title"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.heading(t!("streaming_options.heading"));
                        ui.separator();
                        
                        // Stats de performance
                        if self.config.settings.show_performance {
                            ui.label(t!("stats.performance"));
                            ui.horizontal(|ui| {
                                ui.label(format!("{}: {:.1}", t!("stats.fps"), self.current_fps));
                                ui.separator();
                                ui.label(format!("{}: {:.2}ms", t!("stats.capture"), self.current_capture_ms));
                            });
                            
                            ui.label(t!("stats.system_resources"));
                            ui.horizontal(|ui| {
                                ui.label(format!("CPU: {:.1}%", self.cpu_usage));
                                ui.separator();
                                ui.label(format!("RAM: {:.1} MB", self.memory_mb));
                            });
                            ui.horizontal(|ui| {
                                ui.label(format!("{}: {:.2} MB/s", t!("stats.datarate"), self.data_rate_mbps));
                                ui.separator();
                                ui.label(format!("{}: {}", t!("stats.frames"), self.frames_captured));
                            });
                            ui.separator();
                        }
                        
                        // Frame rate
                        ui.horizontal(|ui| {
                            ui.label(t!("settings.framerate"));
                            let mut frame_rate = self.config.settings.frame_rate as i32;
                            if ui.add(egui::Slider::new(&mut frame_rate, 15..=120).suffix(" FPS")).changed() {
                                self.config.set_frame_rate(frame_rate as u32);
                            }
                        });
                        
                        ui.checkbox(&mut self.config.settings.show_performance, t!("settings.show_performance_short"));
                        
                        ui.separator();
                        
                        ui.horizontal(|ui| {
                            if ui.button(t!("action.new_selection")).clicked() {
                                self.stop_capture();
                                self.streaming_only = false;
                                self.show_streaming_options = false;
                                self.start_region_selection(ctx);
                            }
                            
                            if ui.button(t!("action.stop")).clicked() {
                                self.stop_capture();
                                self.streaming_only = false;
                                self.show_streaming_options = false;
                                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(DEFAULT_WINDOW_SIZE[0], DEFAULT_WINDOW_SIZE[1])));
                            }
                        });
                        
                        ui.separator();
                        
                        if ui.button(t!("action.close")).clicked() {
                            self.show_streaming_options = false;
                            if let Err(e) = self.config.save() {
                                error!("[Config] Save error: {}", e);
                            }
                        }
                    });
            }
            
            if self.capturing {
                ctx.request_repaint();
            }
            return;
        }
        
        // UI de démarrage - interface simplifiée
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.heading("Region to Share");
                ui.add_space(10.0);
                ui.label(t!("main.subtitle"));
                ui.add_space(30.0);
                
                // Bouton principal de sélection
                if ui.add_sized([200.0, 50.0], egui::Button::new(format!("🖱 {}", t!("action.select_region")))).clicked() {
                    self.start_region_selection(ctx);
                }
                
                ui.add_space(20.0);
                
                // Dernière région si disponible
                if self.config.settings.remember_last_region {
                    if let Some(last) = self.config.get_last_region() {
                        ui.label(t!("main.last_region", w = last.width.to_string(), h = last.height.to_string(), x = last.x.to_string(), y = last.y.to_string()));
                        if ui.button(t!("action.reuse")).clicked() {
                            self.x = last.x;
                            self.y = last.y;
                            self.width = last.width;
                            self.height = last.height;
                            self.streaming_only = true;
                            self.pending_resize = Some((self.width, self.height));
                            self.resize_frame_count = 0;
                            ctx.request_repaint();
                        }
                    }
                }
            });
            
            ui.add_space(20.0);
            ui.separator();
            
            // Paramètres (toujours visibles, compacts)
            ui.collapsing(format!("⚙ {}", t!("settings.title")), |ui| {
                egui::ScrollArea::vertical()
                    .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(t!("settings.framerate"));
                    let mut frame_rate = self.config.settings.frame_rate as i32;
                    if ui.add(egui::Slider::new(&mut frame_rate, 15..=120).suffix(" FPS")).changed() {
                        self.config.set_frame_rate(frame_rate as u32);
                    }
                });
                
                ui.add_space(5.0);
                ui.label(t!("settings.window"));
                ui.horizontal(|ui| {
                    ui.label(t!("settings.opacity"));
                    let mut opacity = self.config.settings.window_opacity;
                    if ui.add(egui::Slider::new(&mut opacity, 0.3..=1.0).show_value(true)).changed() {
                        self.config.set_window_opacity(opacity);
                    }
                });
                
                ui.checkbox(&mut self.config.settings.auto_send_to_background,
                    t!("settings.send_to_background"));
                
                ui.add_space(5.0);
                ui.label(t!("settings.region"));
                ui.checkbox(&mut self.config.settings.remember_last_region, t!("settings.remember_region"));
                ui.checkbox(&mut self.config.settings.auto_use_specific_region,
                    t!("settings.auto_use_region"));
                
                ui.add_space(5.0);
                ui.checkbox(&mut self.config.settings.show_performance, t!("settings.show_performance"));

                ui.add_space(5.0);
                ui.label(t!("settings.language"));
                let languages = [("auto", t!("settings.lang_auto")), ("fr", "Français".into()), ("en", "English".into())];
                let current = if self.config.settings.language.is_empty() {
                    "auto"
                } else {
                    &self.config.settings.language
                };
                let mut selected = current.to_owned();
                egui::ComboBox::from_id_salt("language_selector")
                    .selected_text(languages.iter().find(|(k, _)| *k == selected).map(|(_, v)| v.as_ref()).unwrap_or(selected.as_str()))
                    .show_ui(ui, |ui| {
                        for (key, label) in &languages {
                            ui.selectable_value(&mut selected, key.to_string(), label.as_ref());
                        }
                    });
                if selected != current {
                    let new_lang = if selected == "auto" { String::new() } else { selected.clone() };
                    self.config.settings.language = new_lang;
                    let locale_to_apply = if selected == "auto" {
                        sys_locale::get_locale()
                            .unwrap_or_else(|| String::from("en"))
                            .split(|c: char| c == '-' || c == '_')
                            .next()
                            .unwrap_or("en")
                            .to_owned()
                    } else {
                        selected
                    };
                    rust_i18n::set_locale(&locale_to_apply);
                    let _ = self.config.save();
                }

                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.label(t!("settings.global_shortcut"));
                    let mut shortcut = self.config.settings.global_shortcut.clone();
                    if ui.text_edit_singleline(&mut shortcut).changed() {
                        self.config.settings.global_shortcut = shortcut;
                    }
                });
                ui.label(t!("settings.shortcut_hint"));
                
                ui.add_space(10.0);
                if ui.button(t!("action.save")).clicked() {
                    if let Err(e) = self.config.save() {
                        error!("[Config] Save error: {}", e);
                    }
                }
                }); // end ScrollArea
            });
            
            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                ui.label(t!("main.streaming_hint"));
            });
        });
        
        if self.capturing {
            ctx.request_repaint();
        }
    }
}

async fn capture_task_continuous(
    region: Rectangle,
    target_fps: u32,
    tx: UnboundedSender<StatsUpdate>,
    mut stop_rx: UnboundedReceiver<()>,
    existing_backend: Option<Box<dyn CaptureBackend>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Valider la région avant de commencer
    let region = Rectangle {
        x: region.x.max(0),
        y: region.y.max(0),
        width: region.width.clamp(1, 3840),
        height: region.height.clamp(1, 2160),
    };
    
    // Réutiliser le backend existant ou en créer un nouveau
    let mut backend: Box<dyn CaptureBackend> = if let Some(backend) = existing_backend {
        backend
    } else if is_wayland() {
        Box::new(PortalBackend::new())
    } else {
        Box::new(AutoBackend::new()?)
    };
    
    // Mettre à jour la région sur le backend
    backend.init(region).await?;
    
    let mut profiler = FrameProfiler::new(30);
    let mut frame_count = 0u32;
    let mut frames_skipped = 0u32;
    
    // Calculer le temps minimum entre chaque frame
    let frame_duration = std::time::Duration::from_secs_f64(1.0 / target_fps as f64);
    
    // Stats système
    let mut resource_monitor = ResourceMonitor::new();
    let mut total_bytes_sent: u64 = 0;
    let stats_start = std::time::Instant::now();
    
    // Pré-calculer la taille estimée du buffer
    let estimated_buffer_size = (region.width * region.height * 4) as usize;
    let _ = estimated_buffer_size; // Used for buffer pre-allocation
    
    // Variable pour tracker la dernière frame envoyée
    let mut last_sequence = 0u64;
    
    loop {
        let frame_start = std::time::Instant::now();
        
        // Check stop signal (non-bloquant)
        if stop_rx.try_recv().is_ok() {
            let _ = tx.send(StatsUpdate::Stopped);
            break;
        }
        
        profiler.start_frame();
        
        // Capture - utilise le backend optimisé
        let capture_start = std::time::Instant::now();
        let frame = match backend.capture_frame().await {
            Ok(f) => f,
            Err(e) => {
                error!("[Capture] Frame error: {}", e);
                tokio::time::sleep(frame_duration).await;
                continue;
            }
        };
        let capture_time = capture_start.elapsed();
        
        profiler.record_capture(capture_time);
        
        frame_count += 1;
        
        // Skip si c'est la même frame (basé sur sequence number)
        if frame.sequence == last_sequence {
            frames_skipped += 1;
            trace!("[Capture] Frame skipped (same sequence), total skipped: {}", frames_skipped);
            tokio::time::sleep(frame_duration / 2).await;
            continue;
        }
        last_sequence = frame.sequence;
        
        // Calculer la taille des données
        let frame_bytes = if let Some(buffer) = frame.data.as_buffer() {
            buffer.len() as u64
        } else {
            0
        };
        total_bytes_sent += frame_bytes;
        
        // Stats toutes les 10 frames
        if frame_count % 10 == 0 {
            let stats = profiler.stats();
            let (cpu_percent, memory_mb) = resource_monitor.get_stats();
            
            let elapsed_secs = stats_start.elapsed().as_secs_f64();
            let data_rate_mbps = if elapsed_secs > 0.0 {
                (total_bytes_sent as f64 / 1_000_000.0) / elapsed_secs
            } else {
                0.0
            };
            
            let _ = tx.send(StatsUpdate::Progress {
                frames: frame_count,
                fps: stats.avg_fps,
                capture_ms: stats.avg_capture_ms,
                frame_ms: stats.avg_frame_ms,
                cpu_percent,
                memory_mb,
                data_rate_mbps,
            });
        }
        
        // Envoyer la frame via Arc (zero-copy)
        if let Some(arc_buffer) = frame.data.as_arc_buffer() {
            let frame_data = FrameData {
                width: frame.width,
                height: frame.height,
                data: arc_buffer,
                format: frame.format,
            };
            let _ = tx.send(StatsUpdate::Frame(frame_data));
        }
        
        // Frame pacing intelligent
        let elapsed = frame_start.elapsed();
        if elapsed < frame_duration {
            let sleep_time = frame_duration - elapsed;
            if sleep_time > std::time::Duration::from_micros(500) {
                tokio::time::sleep(sleep_time).await;
            } else {
                while frame_start.elapsed() < frame_duration {
                    std::hint::spin_loop();
                }
            }
        }
    }
    
    Ok(())
}

/// Moniteur de ressources système (CPU, mémoire)
struct ResourceMonitor {
    last_cpu_time: u64,
    last_check: std::time::Instant,
    num_cpus: usize,
}

impl ResourceMonitor {
    fn new() -> Self {
        Self {
            last_cpu_time: Self::get_process_cpu_time(),
            last_check: std::time::Instant::now(),
            num_cpus: Self::get_num_cpus(),
        }
    }
    
    fn get_stats(&mut self) -> (f64, f64) {
        let cpu_percent = self.get_cpu_usage();
        let memory_mb = Self::get_memory_usage();
        (cpu_percent, memory_mb)
    }
    
    fn get_cpu_usage(&mut self) -> f64 {
        let current_cpu_time = Self::get_process_cpu_time();
        let elapsed = self.last_check.elapsed();
        
        if elapsed.as_millis() < 100 {
            return 0.0;
        }
        
        let cpu_delta = current_cpu_time.saturating_sub(self.last_cpu_time);
        let elapsed_ticks = (elapsed.as_secs_f64() * 100.0) as u64; // En centièmes de seconde
        
        // CPU par cœur
        let cpu_per_core = if elapsed_ticks > 0 {
            (cpu_delta as f64 / elapsed_ticks as f64) * 100.0
        } else {
            0.0
        };
        
        // Normaliser par le nombre de cœurs pour avoir le % du CPU total
        let cpu_percent = cpu_per_core / self.num_cpus as f64;
        
        self.last_cpu_time = current_cpu_time;
        self.last_check = std::time::Instant::now();
        
        cpu_percent.min(100.0)
    }
    
    fn get_num_cpus() -> usize {
        // Lire le nombre de CPUs depuis /proc/cpuinfo
        if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
            let count = cpuinfo.lines()
                .filter(|line| line.starts_with("processor"))
                .count();
            if count > 0 {
                return count;
            }
        }
        // Fallback: utiliser std::thread
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    }
    
    fn get_process_cpu_time() -> u64 {
        // Lire /proc/self/stat pour obtenir le temps CPU du processus
        if let Ok(stat) = fs::read_to_string("/proc/self/stat") {
            let parts: Vec<&str> = stat.split_whitespace().collect();
            if parts.len() > 14 {
                let utime: u64 = parts[13].parse().unwrap_or(0);
                let stime: u64 = parts[14].parse().unwrap_or(0);
                return utime + stime;
            }
        }
        0
    }
    
    fn get_memory_usage() -> f64 {
        // Lire /proc/self/status pour obtenir la mémoire RSS
        if let Ok(status) = fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let kb: f64 = parts[1].parse().unwrap_or(0.0);
                        return kb / 1024.0; // Convertir en MB
                    }
                }
            }
        }
        0.0
    }
}
