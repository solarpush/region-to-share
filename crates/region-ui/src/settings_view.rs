//! Tabbed v3 Settings panel (Capture, Recording, Window & HUD, KDE & GNOME, Diagnostics).

use crate::icons::Icon;
use crate::theme::Palette;
use crate::widgets::pill_button;
use egui::{ComboBox, RichText, Slider, Ui};
use region_config::ConfigStore;
use region_core::{
    AudioMode, ContainerFormat, HardwareEncoder, HudMode, PauseScreenMode, UiTheme, VideoCodec,
};
use region_platform::{DesktopEnvironment, DisplayServer, PackageFormat};
use region_recorder::EncoderCapabilities;
use rust_i18n::t;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Capture,
    Recording,
    Display,
    Integrations,
    Diagnostics,
}

/// Actions requested from the settings screen (e.g. install KDE/GNOME integration).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAction {
    Close,
    ClearRestoreToken,
    InstallKdePlasmoid,
    InstallGnomeExtension,
    ThemeChanged,
    LanguageChanged,
    BrowsePauseVideo,
}

pub fn render_settings_view(
    ui: &mut Ui,
    palette: Palette,
    store: &mut ConfigStore,
    active_tab: &mut SettingsTab,
    enc_caps: &EncoderCapabilities,
    integration_feedback: Option<&str>,
) -> Option<SettingsAction> {
    let mut action = None;

    // Header
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(t!("settings.title"))
                .size(18.0)
                .strong()
                .color(palette.text_primary),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if pill_button(
                ui,
                palette,
                Icon::Check,
                &t!("settings.save_close"),
                true,
                palette.accent_share,
            )
            .clicked()
            {
                let _ = store.save();
                action = Some(SettingsAction::Close);
            }
        });
    });

    ui.add_space(8.0);

    // Tab bar
    ui.horizontal_wrapped(|ui| {
        let tabs = [
            (SettingsTab::Capture, t!("settings.tab_capture")),
            (SettingsTab::Recording, t!("settings.tab_recording")),
            (SettingsTab::Display, t!("settings.tab_display")),
            (SettingsTab::Integrations, t!("settings.tab_integrations")),
            (SettingsTab::Diagnostics, t!("settings.tab_diagnostics")),
        ];
        for (tab, label) in tabs {
            let selected = *active_tab == tab;
            let btn = ui.add(
                egui::Button::new(
                    RichText::new(label)
                        .size(12.0)
                        .color(if selected {
                            egui::Color32::WHITE
                        } else {
                            palette.text_secondary
                        }),
                )
                .fill(if selected {
                    palette.accent_share
                } else {
                    palette.bg_elevated
                })
                .rounding(egui::Rounding::same(8.0)),
            );
            if btn.clicked() {
                *active_tab = tab;
            }
        }
    });

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);

    egui::ScrollArea::vertical().show(ui, |ui| match active_tab {
        SettingsTab::Capture => {
            let cfg = &mut store.config;
            ui.horizontal(|ui| {
                ui.label(t!("settings.framerate"));
                let mut fps = cfg.capture.frame_rate as i32;
                if ui.add(Slider::new(&mut fps, 15..=120).suffix(" FPS")).changed() {
                    cfg.capture.frame_rate = fps as u32;
                }
            });
            ui.checkbox(&mut cfg.capture.show_cursor, t!("settings.show_cursor"));
            ui.checkbox(&mut cfg.capture.remember_region, t!("settings.remember_region"));
            ui.checkbox(&mut cfg.capture.auto_use_last_region, t!("settings.auto_use_region"));

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            ui.checkbox(
                &mut cfg.portal.enable_virtual_camera,
                t!("hub.virtual_cam_toggle"),
            );
            ui.label(
                RichText::new(t!("hub.virtual_cam_hint"))
                    .size(11.0)
                    .color(palette.text_muted),
            );

            ui.add_space(6.0);
            let has_token = cfg.portal.restore_token.is_some();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(if has_token {
                        "Portal Restore Token: saved (no repeated prompts)"
                    } else {
                        "Portal Restore Token: none yet"
                    })
                    .size(11.5)
                    .color(palette.text_muted),
                );
                if has_token
                    && ui
                        .small_button(t!("settings.clear_restore_token"))
                        .clicked()
                {
                    action = Some(SettingsAction::ClearRestoreToken);
                }
            });
        }

        SettingsTab::Recording => {
            let rec = &mut store.config.recording;
            ui.horizontal(|ui| {
                ui.label(t!("settings.container"));
                let items = [
                    (ContainerFormat::Mkv, "MKV (Crash-Safe Live)"),
                    (ContainerFormat::Mp4, "MP4 (Fragmented Crash-Safe)"),
                    (ContainerFormat::Webm, "WebM (VP9/AV1)"),
                    (ContainerFormat::Gif, "GIF"),
                ];
                let lbl = items
                    .iter()
                    .find(|(k, _)| *k == rec.container)
                    .map(|(_, v)| *v)
                    .unwrap_or("MKV");
                ComboBox::from_id_salt("rec_container")
                    .selected_text(lbl)
                    .show_ui(ui, |ui| {
                        for (k, v) in items {
                            ui.selectable_value(&mut rec.container, k, v);
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label(t!("settings.codec"));
                let items = [
                    (VideoCodec::Auto, "Auto"),
                    (VideoCodec::H264, "H.264"),
                    (VideoCodec::Hevc, "HEVC (H.265)"),
                    (VideoCodec::Av1, "AV1"),
                    (VideoCodec::Vp9, "VP9"),
                ];
                let lbl = items
                    .iter()
                    .find(|(k, _)| *k == rec.codec)
                    .map(|(_, v)| *v)
                    .unwrap_or("Auto");
                ComboBox::from_id_salt("rec_codec")
                    .selected_text(lbl)
                    .show_ui(ui, |ui| {
                        for (k, v) in items {
                            ui.selectable_value(&mut rec.codec, k, v);
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label(t!("settings.hw_accel"));
                let items = [
                    (HardwareEncoder::Auto, "Auto (GPU -> CPU)"),
                    (HardwareEncoder::Vaapi, "VA-API (Intel / AMD)"),
                    (HardwareEncoder::Nvenc, "NVENC (NVIDIA)"),
                    (HardwareEncoder::Cpu, "CPU Software"),
                ];
                let lbl = items
                    .iter()
                    .find(|(k, _)| *k == rec.hw_accel)
                    .map(|(_, v)| *v)
                    .unwrap_or("Auto");
                ComboBox::from_id_salt("rec_hw")
                    .selected_text(lbl)
                    .show_ui(ui, |ui| {
                        for (k, v) in items {
                            ui.selectable_value(&mut rec.hw_accel, k, v);
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label(t!("settings.audio"));
                let items = [
                    (AudioMode::None, "No Audio"),
                    (AudioMode::Desktop, "Desktop Audio"),
                    (AudioMode::Mic, "Microphone"),
                    (AudioMode::Both, "Desktop + Microphone"),
                ];
                let lbl = items
                    .iter()
                    .find(|(k, _)| *k == rec.audio_mode)
                    .map(|(_, v)| *v)
                    .unwrap_or("No Audio");
                ComboBox::from_id_salt("rec_audio")
                    .selected_text(lbl)
                    .show_ui(ui, |ui| {
                        for (k, v) in items {
                            ui.selectable_value(&mut rec.audio_mode, k, v);
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label(t!("settings.quality_crf"));
                let mut crf = rec.quality_crf as i32;
                if ui.add(Slider::new(&mut crf, 14..=34)).changed() {
                    rec.quality_crf = crf as u8;
                }
            });

            ui.horizontal(|ui| {
                ui.label(t!("settings.output_dir"));
                ui.add(
                    egui::TextEdit::singleline(&mut rec.output_dir)
                        .hint_text("~/Videos/RegionToShare")
                        .desired_width(220.0),
                );
            });

            ui.checkbox(
                &mut rec.remux_mp4_faststart,
                "Optimize MP4 with +faststart on stop",
            );
        }

        SettingsTab::Display => {
            let disp = &mut store.config.display;
            let ui_cfg = &mut store.config.ui;

            ui.horizontal(|ui| {
                ui.label(t!("settings.hud_mode"));
                let items = [
                    (HudMode::DetachedPill, "Detached Pill (Mirror 100% Pure)"),
                    (HudMode::AutoHide, "Auto-Hide on Hover"),
                    (HudMode::TrayOnly, "Panel / CLI Only"),
                ];
                let lbl = items
                    .iter()
                    .find(|(k, _)| *k == disp.hud_mode)
                    .map(|(_, v)| *v)
                    .unwrap_or("Detached Pill");
                ComboBox::from_id_salt("hud_mode_combo")
                    .selected_text(lbl)
                    .show_ui(ui, |ui| {
                        for (k, v) in items {
                            ui.selectable_value(&mut disp.hud_mode, k, v);
                        }
                    });
            });

            ui.checkbox(&mut disp.auto_background, t!("settings.auto_background"));
            ui.checkbox(&mut ui_cfg.show_selection_grid, t!("settings.selection_grid"));
            ui.checkbox(&mut ui_cfg.show_magnifier, t!("settings.selection_magnifier"));

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label(
                RichText::new(t!("settings.pause_section"))
                    .strong()
                    .size(12.5)
                    .color(palette.text_primary),
            );
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label(t!("settings.pause_mode"));
                let msg_lbl = t!("settings.pause_mode_message").to_string();
                let vid_lbl = t!("settings.pause_mode_video").to_string();
                let current_lbl = match disp.pause_mode {
                    PauseScreenMode::Message => msg_lbl.as_str(),
                    PauseScreenMode::Video => vid_lbl.as_str(),
                };
                ComboBox::from_id_salt("pause_mode_combo")
                    .selected_text(current_lbl)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut disp.pause_mode, PauseScreenMode::Message, &msg_lbl);
                        ui.selectable_value(&mut disp.pause_mode, PauseScreenMode::Video, &vid_lbl);
                    });
            });

            match disp.pause_mode {
                PauseScreenMode::Message => {
                    ui.horizontal(|ui| {
                        ui.label(t!("settings.pause_message"));
                        ui.add(
                            egui::TextEdit::singleline(&mut disp.pause_message)
                                .hint_text(t!("pause.default_message"))
                                .desired_width(270.0),
                        );
                    });
                }
                PauseScreenMode::Video => {
                    ui.horizontal(|ui| {
                        ui.label(t!("settings.pause_video"));
                        ui.add(
                            egui::TextEdit::singleline(&mut disp.pause_video_path)
                                .hint_text("/path/to/pause.mp4")
                                .desired_width(205.0),
                        );
                        if ui.button(t!("settings.browse")).clicked() {
                            action = Some(SettingsAction::BrowsePauseVideo);
                        }
                    });
                    let trimmed = disp.pause_video_path.trim();
                    if !trimmed.is_empty() && !std::path::Path::new(trimmed).is_file() {
                        ui.label(
                            RichText::new(format!("⚠ {}", t!("settings.pause_video_missing")))
                                .size(11.0)
                                .color(palette.accent_warn),
                        );
                    }
                }
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(t!("settings.theme"));
                let prev = ui_cfg.theme;
                ComboBox::from_id_salt("theme_combo")
                    .selected_text(format!("{:?}", ui_cfg.theme))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut ui_cfg.theme, UiTheme::Auto, "Auto (System)");
                        ui.selectable_value(&mut ui_cfg.theme, UiTheme::Dark, "Dark (Midnight)");
                        ui.selectable_value(&mut ui_cfg.theme, UiTheme::Light, "Light (Alabaster)");
                    });
                if ui_cfg.theme != prev {
                    action = Some(SettingsAction::ThemeChanged);
                }
            });

            ui.horizontal(|ui| {
                ui.label(t!("settings.language"));
                let current = if ui_cfg.language.is_empty() {
                    "auto".to_string()
                } else {
                    ui_cfg.language.clone()
                };
                let mut selected = current.clone();
                let langs = [
                    ("auto", t!("settings.lang_auto").to_string()),
                    ("fr", "Français".to_string()),
                    ("en", "English".to_string()),
                ];
                let lbl = langs
                    .iter()
                    .find(|(k, _)| *k == selected)
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("Auto");
                ComboBox::from_id_salt("lang_combo")
                    .selected_text(lbl)
                    .show_ui(ui, |ui| {
                        for (k, v) in &langs {
                            ui.selectable_value(&mut selected, (*k).to_string(), v);
                        }
                    });
                if selected != current {
                    ui_cfg.language = if selected == "auto" {
                        String::new()
                    } else {
                        selected
                    };
                    action = Some(SettingsAction::LanguageChanged);
                }
            });
        }

        SettingsTab::Integrations => {
            ui.label(
                RichText::new("Desktop Panel Components (D-Bus: io.github.solarpush.RegionToShare)")
                    .strong()
                    .color(palette.text_primary),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Install the native KDE Plasma 6 Plasmoid or GNOME Shell Quick Settings \
                     extension to start sharing or live recording in 1 click from your top/bottom bar.",
                )
                .size(12.0)
                .color(palette.text_secondary),
            );
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                if pill_button(
                    ui,
                    palette,
                    Icon::Layers,
                    "Install KDE Plasma 6 Plasmoid",
                    true,
                    palette.accent_share,
                )
                .clicked()
                {
                    action = Some(SettingsAction::InstallKdePlasmoid);
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if pill_button(
                    ui,
                    palette,
                    Icon::Sparkles,
                    "Install GNOME Shell Extension",
                    false,
                    palette.accent_purple,
                )
                .clicked()
                {
                    action = Some(SettingsAction::InstallGnomeExtension);
                }
            });

            if let Some(msg) = integration_feedback {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(msg)
                        .size(11.5)
                        .color(palette.accent_live),
                );
            }

            ui.add_space(10.0);
            ui.separator();
            ui.label(
                RichText::new("CLI Quick Commands")
                    .strong()
                    .size(12.0)
                    .color(palette.text_primary),
            );
            ui.label(
                RichText::new(format!(
                    "• {} share --native-portal\n• {} record -o demo.mkv --fps 60\n• {} toggle-record",
                    PackageFormat::detect().launch_command(),
                    PackageFormat::detect().launch_command(),
                    PackageFormat::detect().launch_command(),
                ))
                .monospace()
                .size(11.0)
                .color(palette.text_secondary),
            );
        }

        SettingsTab::Diagnostics => {
            let ds = DisplayServer::detect();
            let de = DesktopEnvironment::detect();
            let pkg = PackageFormat::detect();

            let rows = [
                ("Version", format!("v{}", env!("CARGO_PKG_VERSION"))),
                ("Display Server", ds.label().to_string()),
                ("Desktop Environment", de.label().to_string()),
                ("Package Format", format!("{pkg:?}")),
                (
                    "KDE Native Region Portal",
                    if de.has_native_portal_region_picker() {
                        "Supported (KWin stream_region)".into()
                    } else {
                        "Using Pro Overlay".into()
                    },
                ),
                (
                    "FFmpeg",
                    enc_caps
                        .ffmpeg_version
                        .clone()
                        .unwrap_or_else(|| "Not found in PATH".into()),
                ),
                (
                    "VA-API Render Node",
                    enc_caps
                        .vaapi_render_node
                        .clone()
                        .unwrap_or_else(|| "None".into()),
                ),
                ("Encoders", enc_caps.encoders.join(", ")),
            ];

            for (k, v) in rows {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{k}:"))
                            .strong()
                            .size(12.0)
                            .color(palette.text_secondary),
                    );
                    ui.label(RichText::new(v).monospace().size(11.5).color(palette.text_primary));
                });
            }
        }
    });

    action
}
