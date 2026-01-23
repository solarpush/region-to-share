use eframe::egui;
use region_core::{Rectangle, PixelFormat};
use region_core::performance::FrameProfiler;
use region_capture::{CaptureBackend, AutoBackend};
use region_config::Config;
use std::sync::Arc;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use std::sync::Mutex as StdMutex;
use std::fs;

fn main() -> Result<(), eframe::Error> {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 350.0])
            .with_title("Region to Share")
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "Region to Share",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(RegionApp::new(runtime)))
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
    _format: PixelFormat,
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
    fn new(runtime: tokio::runtime::Runtime) -> Self {
        let (_tx, rx) = mpsc::unbounded_channel();
        
        let config = Config::new();
        
        // Charger la dernière région si disponible
        let (x, y, width, height) = if let Some(last_region) = config.get_last_region() {
            println!("Chargement de la dernière région: {}x{} at ({}, {})", 
                last_region.width, last_region.height, last_region.x, last_region.y);
            (last_region.x, last_region.y, last_region.width, last_region.height)
        } else {
            (560, 240, 800, 600)
        };
        
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
            streaming_only: false,
            screenshot_display_rect: None,
            pending_resize: None,
            resize_frame_count: 0,
            show_streaming_options: false,
            cpu_usage: 0.0,
            memory_mb: 0.0,
            data_rate_mbps: 0.0,
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
        runtime.spawn(async move {
            if let Err(e) = capture_task_continuous(region, frame_rate, new_tx, stop_rx).await {
                eprintln!("Capture error: {}", e);
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
        
        // Passer en plein écran sans bordures (comme l'app Python)
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        
        // Créer un nouveau channel pour la capture du screenshot
        let (new_tx, new_rx) = mpsc::unbounded_channel();
        *self.stats_rx.lock().unwrap() = new_rx;
        
        let runtime = self.runtime.clone();
        let ctx_clone = ctx.clone();
        
        runtime.spawn(async move {
            println!("Démarrage capture screenshot plein écran...");
            let mut backend = match AutoBackend::new() {
                Ok(b) => {
                    println!("Backend créé avec succès");
                    b
                },
                Err(e) => {
                    eprintln!("Failed to create backend: {}", e);
                    return;
                }
            };
            
            // Obtenir la taille réelle de l'écran via le backend (compatible X11 et portal)
            let (screen_width, screen_height) = match backend.get_screen_size().await {
                Ok(size) => {
                    println!("Taille de l'écran détectée: {}x{}", size.0, size.1);
                    size
                },
                Err(e) => {
                    eprintln!("Impossible d'obtenir la taille de l'écran: {}", e);
                    (1920, 1080) // Fallback
                }
            };
            
            // Utiliser capture_screenshot pour capturer TOUT l'écran
            println!("Capture du screenshot {}x{}...", screen_width, screen_height);
            match backend.capture_screenshot().await {
                Ok(frame) => {
                    println!("Screenshot capturé: {}x{}", frame.width, frame.height);
                    if let Some(arc_buffer) = frame.data.as_arc_buffer() {
                    let frame_data = FrameData {
                        width: frame.width,
                        height: frame.height,
                        data: arc_buffer,
                        _format: frame.format,
                    };
                    println!("Envoi du screenshot via channel...");
                    let _ = new_tx.send(StatsUpdate::Screenshot(frame_data));
                    ctx_clone.request_repaint();
                    println!("Screenshot envoyé et repaint demandé");
                } else {
                    eprintln!("Pas de buffer dans la frame");
                }
            },
            Err(e) => {
                eprintln!("Échec de la capture de screenshot: {}", e);
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
                println!("ERREUR: Pas de screenshot texture!");
                return;
            };
            
            // Obtenir le rectangle où le screenshot a été affiché
            let display_rect = if let Some(rect) = self.screenshot_display_rect {
                rect
            } else {
                println!("ERREUR: Pas de display rect sauvegardé!");
                return;
            };
            
            // Obtenir le facteur de scaling DPI
            let pixels_per_point = ctx.pixels_per_point();
            
            println!("\n=== DEBUG SELECTION ===");
            println!("Screenshot réel: {}x{}", screenshot_width, screenshot_height);
            println!("Display rect (points logiques): {:?}", display_rect);
            println!("Pixels per point (DPI scale): {}", pixels_per_point);
            println!("Sélection start: {:?}, end: {:?}", start, end);
            
            // Calculer le ratio de scaling exact
            let scale_x = screenshot_width / display_rect.width();
            let scale_y = screenshot_height / display_rect.height();
            
            println!("Scale ratio: X={:.4}, Y={:.4}", scale_x, scale_y);
            
            // Convertir les coordonnées de sélection (dans l'espace display_rect)
            // vers les coordonnées réelles du screenshot
            // Les coordonnées sont relatives au display_rect.min
            let rel_min_x = (start.x.min(end.x) - display_rect.min.x).max(0.0);
            let rel_min_y = (start.y.min(end.y) - display_rect.min.y).max(0.0);
            let rel_max_x = (start.x.max(end.x) - display_rect.min.x).min(display_rect.width());
            let rel_max_y = (start.y.max(end.y) - display_rect.min.y).min(display_rect.height());
            
            // Appliquer le scaling pour obtenir les pixels réels
            let min_x = (rel_min_x * scale_x).round();
            let min_y = (rel_min_y * scale_y).round();
            let max_x = (rel_max_x * scale_x).round();
            let max_y = (rel_max_y * scale_y).round();
            
            // Calculer les dimensions de la région sélectionnée dans l'espace réel
            let region_width = (max_x - min_x).max(1.0);
            let region_height = (max_y - min_y).max(1.0);
            
            self.x = min_x as i32;
            self.y = min_y as i32;
            self.width = region_width as u32;
            self.height = region_height as u32;
            
            println!("Region selectionnee (pixels reels): {}x{} at ({}, {})", self.width, self.height, self.x, self.y);
            
            // Sauvegarder la région si l'option est activée
            if self.config.settings.remember_last_region {
                self.config.set_last_region(self.x, self.y, self.width, self.height);
                if let Err(e) = self.config.save() {
                    eprintln!("Erreur lors de la sauvegarde de la config: {}", e);
                }
            }
            
            // Sortir du plein écran
            println!("Sortie du plein écran...");
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
            
            // Marquer le redimensionnement en attente - on va le faire après quelques frames
            // pour laisser le temps au window manager de traiter la sortie du plein écran
            self.pending_resize = Some((self.width, self.height));
            self.resize_frame_count = 0;
            
            println!("Redimensionnement programmé: {}x{} pixels", self.width, self.height);
            
            // Forcer un repaint
            ctx.request_repaint();
        }
        
        self.selecting_region = false;
        self.screenshot_texture = None;
        self.selection_start = None;
        self.selection_end = None;
        self.selection_ready = false;
        
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
                }
            }
        }
    }
    
    fn frame_to_image(&self, frame_data: &FrameData) -> Result<egui::ColorImage, String> {
        // Conversion BGRA -> RGBA ultra-optimisée avec chunks
        let pixel_count = (frame_data.width * frame_data.height) as usize;
        let data = &*frame_data.data;
        
        // Pré-allouer avec exact_chunks pour permettre au compilateur d'optimiser
        let pixels: Vec<egui::Color32> = data.chunks_exact(4)
            .take(pixel_count)
            .map(|chunk| {
                // BGRA -> RGBA : swap B et R, ignorer alpha
                egui::Color32::from_rgba_unmultiplied(
                    chunk[2], // R (was B)
                    chunk[1], // G
                    chunk[0], // B (was R)
                    255,      // A opaque
                )
            })
            .collect();
        
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
            
            // Attendre 5 frames avant de redimensionner pour laisser le window manager
            // traiter la sortie du plein écran
            if self.resize_frame_count >= 5 {
                let window_width = target_width as f32;
                let window_height = target_height as f32;
                
                println!("\n=== REDIMENSIONNEMENT DIFFERE ===");
                println!("Frame count: {}", self.resize_frame_count);
                println!("Taille cible: {}x{}", window_width, window_height);
                
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                    egui::vec2(window_width, window_height)
                ));
                
                self.pending_resize = None;
                
                // Démarrer le stream maintenant que la fenêtre est redimensionnée
                if !self.capturing {
                    self.start_capture();
                }
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
                            ui.heading("Capture de l'ecran...");
                            ui.spinner();
                        });
                        return;
                    }
                    
                    // Afficher le screenshot en fond
                    if let Some(screenshot) = &self.screenshot_texture {
                        let img_size = screenshot.size_vec2();
                        let screen_rect = ui.max_rect();
                        
                        // Log une seule fois
                        static mut SELECTION_LOGGED: bool = false;
                        unsafe {
                            if !SELECTION_LOGGED {
                                println!("\n=== MODE SELECTION ===");
                                println!("UI rect (points logiques): {:?}", screen_rect);
                                println!("Screenshot size (pixels): {:?}", img_size);
                                SELECTION_LOGGED = true;
                            }
                        }
                        
                        // IMPORTANT: On affiche l'image en l'étirant pour remplir TOUT le screen_rect
                        // Donc le screenshot_display_rect EST le screen_rect
                        // Le ratio de conversion est: screenshot_pixels / screen_rect_points
                        self.screenshot_display_rect = Some(screen_rect);
                        
                        // Afficher l'image étiée pour remplir tout l'écran
                        // (comme le fait la version Python)
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
                        ui.heading("Selectionnez une region");
                        ui.label("Cliquez et glissez pour selectionner | Echap pour annuler");
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
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(900.0, 700.0)));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
                        self.selecting_region = false;
                        self.screenshot_texture = None;
                    }
                    
                    // Dessiner la sélection
                    if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
                        let sel_rect = egui::Rect::from_two_pos(start, end);
                        
                        // Rectangle de sélection
                        ui.painter().rect_stroke(
                            sel_rect,
                            0.0,
                            egui::Stroke::new(3.0, egui::Color32::from_rgb(0, 255, 0)),
                        );
                        ui.painter().rect_filled(
                            sel_rect,
                            0.0,
                            egui::Color32::from_rgba_unmultiplied(0, 255, 0, 60),
                        );
                        
                        let w = (sel_rect.width()).abs() as i32;
                        let h = (sel_rect.height()).abs() as i32;
                        let x = sel_rect.min.x as i32;
                        let y = sel_rect.min.y as i32;
                        
                        // Info en haut à gauche de la sélection
                        let text_pos = egui::Pos2::new(sel_rect.min.x, sel_rect.min.y - 25.0);
                        ui.painter().text(
                            text_pos,
                            egui::Align2::LEFT_BOTTOM,
                            format!("{}x{} at ({}, {})", w, h, x, y),
                            egui::FontId::proportional(18.0),
                            egui::Color32::WHITE,
                        );
                    }
                });
            return;
        }
        
        // Mode streaming pur - afficher uniquement l'image sans UI
        if self.streaming_only && self.capturing {
            // Log de la taille réelle de la fenêtre (seulement la première frame)
            static mut LOGGED: bool = false;
            unsafe {
                if !LOGGED {
                    let window_size = ctx.screen_rect().size();
                    let pixels_per_point = ctx.pixels_per_point();
                    println!("\n=== FENETRE STREAMING ===");
                    println!("Taille fenêtre (screen_rect): {}x{} (points logiques)", window_size.x, window_size.y);
                    println!("Taille fenêtre en pixels: {}x{}", window_size.x * pixels_per_point, window_size.y * pixels_per_point);
                    println!("Région capturée: {}x{} at ({}, {})", self.width, self.height, self.x, self.y);
                    println!("Pixels per point: {}", pixels_per_point);
                    LOGGED = true;
                }
            }
            
            egui::CentralPanel::default()
                .frame(egui::Frame::none().inner_margin(egui::Margin::ZERO))
                .show(ctx, |ui| {
                    // Supprimer tout espacement dans l'UI
                    ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 0.0);
                    
                    if let Some(texture) = &self.texture {
                        // Utiliser TOUT le rect disponible sans aucun padding
                        let ui_rect = ui.max_rect();
                        
                        // Log détaillé une seule fois
                        static mut DETAIL_LOGGED: bool = false;
                        unsafe {
                            if !DETAIL_LOGGED {
                                let img_size = egui::vec2(texture.size()[0] as f32, texture.size()[1] as f32);
                                println!("Taille texture: {}x{}", img_size.x, img_size.y);
                                println!("UI max_rect: {:?}", ui_rect);
                                println!("========================\n");
                                DETAIL_LOGGED = true;
                            }
                        }
                        
                        // Utiliser ui.put() pour placer l'image exactement dans le rect,
                        // en étirant l'image pour remplir tout l'espace disponible (1:1 pixel)
                        ui.put(
                            ui_rect,
                            egui::Image::new((texture.id(), ui_rect.size()))
                        );
                    }
                    
                    // Bouton options en haut à droite (petit, semi-transparent)
                    let button_size = egui::vec2(28.0, 28.0);
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
                egui::Window::new("Options")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.heading("Options de streaming");
                        ui.separator();
                        
                        // Stats de performance
                        if self.config.settings.show_performance {
                            ui.label("📊 Performance");
                            ui.horizontal(|ui| {
                                ui.label(format!("FPS: {:.1}", self.current_fps));
                                ui.separator();
                                ui.label(format!("Capture: {:.2}ms", self.current_capture_ms));
                            });
                            
                            ui.label("💻 Ressources système");
                            ui.horizontal(|ui| {
                                ui.label(format!("CPU: {:.1}%", self.cpu_usage));
                                ui.separator();
                                ui.label(format!("RAM: {:.1} MB", self.memory_mb));
                            });
                            ui.horizontal(|ui| {
                                ui.label(format!("Débit: {:.2} MB/s", self.data_rate_mbps));
                                ui.separator();
                                ui.label(format!("Frames: {}", self.frames_captured));
                            });
                            ui.separator();
                        }
                        
                        // Frame rate
                        ui.horizontal(|ui| {
                            ui.label("Frame Rate:");
                            let mut frame_rate = self.config.settings.frame_rate as i32;
                            if ui.add(egui::Slider::new(&mut frame_rate, 15..=120).suffix(" FPS")).changed() {
                                self.config.set_frame_rate(frame_rate as u32);
                            }
                        });
                        
                        ui.checkbox(&mut self.config.settings.show_performance, "Afficher performances");
                        
                        ui.separator();
                        
                        ui.horizontal(|ui| {
                            if ui.button("Nouvelle sélection").clicked() {
                                self.stop_capture();
                                self.streaming_only = false;
                                self.show_streaming_options = false;
                                self.start_region_selection(ctx);
                            }
                            
                            if ui.button("Arrêter").clicked() {
                                self.stop_capture();
                                self.streaming_only = false;
                                self.show_streaming_options = false;
                                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(400.0, 300.0)));
                            }
                        });
                        
                        ui.separator();
                        
                        if ui.button("Fermer").clicked() {
                            self.show_streaming_options = false;
                            if let Err(e) = self.config.save() {
                                eprintln!("Erreur sauvegarde: {}", e);
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
                ui.label("Capturez et partagez une région de votre écran");
                ui.add_space(30.0);
                
                // Bouton principal de sélection
                if ui.add_sized([200.0, 50.0], egui::Button::new("🖱 Sélectionner une région")).clicked() {
                    self.start_region_selection(ctx);
                }
                
                ui.add_space(20.0);
                
                // Dernière région si disponible
                if self.config.settings.remember_last_region {
                    if let Some(last) = self.config.get_last_region() {
                        ui.label(format!("Dernière région: {}x{} à ({}, {})", last.width, last.height, last.x, last.y));
                        if ui.button("Réutiliser").clicked() {
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
            ui.collapsing("⚙ Paramètres", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Frame Rate:");
                    let mut frame_rate = self.config.settings.frame_rate as i32;
                    if ui.add(egui::Slider::new(&mut frame_rate, 15..=120).suffix(" FPS")).changed() {
                        self.config.set_frame_rate(frame_rate as u32);
                    }
                });
                
                ui.checkbox(&mut self.config.settings.remember_last_region, "Se souvenir de la dernière région");
                ui.checkbox(&mut self.config.settings.show_performance, "Afficher les performances");
                
                if ui.button("💾 Sauvegarder").clicked() {
                    if let Err(e) = self.config.save() {
                        eprintln!("Erreur: {}", e);
                    }
                }
            });
            
            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                ui.label("La fenêtre de streaming peut être partagée dans Meet, Discord, OBS...");
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
) -> Result<(), Box<dyn std::error::Error>> {
    // Valider la région avant de commencer
    let region = Rectangle {
        x: region.x.max(0),
        y: region.y.max(0),
        width: region.width.min(3840).max(1),
        height: region.height.min(2160).max(1),
    };
    
    let mut backend = AutoBackend::new()?;
    backend.init(region).await?;
    
    let mut profiler = FrameProfiler::new(30);
    let mut frame_count = 0u32;
    
    // Calculer le temps minimum entre chaque frame
    let frame_duration = std::time::Duration::from_secs_f64(1.0 / target_fps as f64);
    
    // Stats système
    let mut resource_monitor = ResourceMonitor::new();
    let mut total_bytes_sent: u64 = 0;
    let stats_start = std::time::Instant::now();
    
    println!("Starting continuous stream: {}x{} at ({}, {}) - Target: {} FPS ({:.2}ms/frame)", 
        region.width, region.height, region.x, region.y, target_fps, frame_duration.as_secs_f64() * 1000.0);
    
    loop {
        let frame_start = std::time::Instant::now();
        
        if stop_rx.try_recv().is_ok() {
            println!("Stopping stream...");
            let _ = tx.send(StatsUpdate::Stopped);
            break;
        }
        
        profiler.start_frame();
        
        let capture_start = std::time::Instant::now();
        let frame = backend.capture_frame().await?;
        let capture_time = capture_start.elapsed();
        
        profiler.record_capture(capture_time);
        
        frame_count += 1;
        
        // Calculer la taille des données
        let frame_bytes = if let Some(buffer) = frame.data.as_buffer() {
            buffer.len() as u64
        } else {
            0
        };
        total_bytes_sent += frame_bytes;
        
        if frame_count % 10 == 0 {
            let stats = profiler.stats();
            
            // Calculer les stats système
            let (cpu_percent, memory_mb) = resource_monitor.get_stats();
            
            // Calculer le débit en MB/s
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
            
            // Log périodique dans la console
            if frame_count % 100 == 0 {
                println!("[Stats] FPS: {:.1} | CPU: {:.1}% | RAM: {:.1}MB | Débit: {:.2}MB/s | Frames: {}", 
                    stats.avg_fps, cpu_percent, memory_mb, data_rate_mbps, frame_count);
            }
        }
        
        // Envoyer la frame seulement si le receiver peut la traiter
        // (skip si le channel est surchargé pour éviter l'accumulation)
        if let Some(arc_buffer) = frame.data.as_arc_buffer() {
            // On envoie la frame - Arc::clone ne copie pas les données
            let frame_data = FrameData {
                width: frame.width,
                height: frame.height,
                data: arc_buffer, // Réutilise l'Arc, pas de copie
                _format: frame.format,
            };
            // Envoyer et ignorer si échec
            let _ = tx.send(StatsUpdate::Frame(frame_data));
        }
        
        // Attendre pour respecter le frame rate cible
        let elapsed = frame_start.elapsed();
        if elapsed < frame_duration {
            tokio::time::sleep(frame_duration - elapsed).await;
        }
    }
    
    println!("Stream stopped: {} frames total", frame_count);
    
    Ok(())
}

/// Moniteur de ressources système (CPU, mémoire)
struct ResourceMonitor {
    last_cpu_time: u64,
    last_check: std::time::Instant,
}

impl ResourceMonitor {
    fn new() -> Self {
        Self {
            last_cpu_time: Self::get_process_cpu_time(),
            last_check: std::time::Instant::now(),
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
        
        let cpu_percent = if elapsed_ticks > 0 {
            (cpu_delta as f64 / elapsed_ticks as f64) * 100.0
        } else {
            0.0
        };
        
        self.last_cpu_time = current_cpu_time;
        self.last_check = std::time::Instant::now();
        
        cpu_percent.min(100.0)
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
