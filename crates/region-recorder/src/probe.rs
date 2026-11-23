//! Auto-detection AND real 1-frame hardware verification of FFmpeg encoders.
//!
//! Never trusts `ffmpeg -encoders` alone (Fedora ships FFmpeg with `h264_nvenc`
//! and `h264_vaapi` compiled in even when CUDA or Mesa freeworld codecs are
//! absent). Instead, hardware candidates are smoke-tested with a tiny 1-frame
//! `640x360` null encode so only genuinely usable GPU/CPU encoders are chosen.

use log::debug;
use region_core::{ContainerFormat, HardwareEncoder, VideoCodec};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};

/// Discovered FFmpeg and verified encoder capabilities on the host machine.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EncoderCapabilities {
    pub ffmpeg_available: bool,
    pub ffmpeg_version: Option<String>,
    pub vaapi_render_node: Option<String>,
    /// Encoders compiled into `ffmpeg` AND verified usable on this hardware.
    pub encoders: Vec<String>,
}

/// Concrete FFmpeg encoder choice resolved from user preferences + host capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedEncoder {
    pub ffmpeg_name: String,
    pub is_hardware: bool,
    pub uses_vaapi: bool,
    pub vaapi_device: Option<String>,
}

impl EncoderCapabilities {
    /// Probe the host system for `ffmpeg`, DRI VA-API render nodes, and genuinely working encoders.
    pub fn probe() -> Self {
        let mut caps = Self::default();

        #[cfg(target_os = "linux")]
        {
            for idx in 128..=135 {
                let path = format!("/dev/dri/renderD{idx}");
                if Path::new(&path).exists() {
                    caps.vaapi_render_node = Some(path);
                    break;
                }
            }
        }

        let version_out = Command::new("ffmpeg").arg("-version").output();
        match version_out {
            Ok(out) if out.status.success() => {
                caps.ffmpeg_available = true;
                let text = String::from_utf8_lossy(&out.stdout);
                caps.ffmpeg_version = text.lines().next().map(|l| l.trim().to_string());
            }
            _ => return caps,
        }

        let Ok(out) = Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output()
        else {
            return caps;
        };
        if !out.status.success() {
            return caps;
        }
        let text = String::from_utf8_lossy(&out.stdout);

        // (encoder_name, is_hardware, uses_vaapi)
        let candidates: &[(&str, bool, bool)] = &[
            ("h264_nvenc", true, false),
            ("hevc_nvenc", true, false),
            ("av1_nvenc", true, false),
            ("h264_vaapi", true, true),
            ("hevc_vaapi", true, true),
            ("av1_vaapi", true, true),
            ("vp9_vaapi", true, true),
            ("h264_videotoolbox", true, false),
            ("hevc_videotoolbox", true, false),
            ("h264_mf", true, false),
            ("hevc_mf", true, false),
            ("libx264", false, false),
            ("libopenh264", false, false),
            ("libx265", false, false),
            ("libvpx-vp9", false, false),
            ("libvpx", false, false),
            ("libsvtav1", false, false),
            ("mpeg4", false, false),
        ];

        for &(enc, is_hw, uses_vaapi) in candidates {
            if !text.contains(&format!(" {enc} ")) {
                continue;
            }
            if uses_vaapi && caps.vaapi_render_node.is_none() {
                continue;
            }
            if is_hw {
                // Real 1-frame GPU smoke test at 640x360 (meets AMD/Intel AV1 min-width 320px!)
                if Self::smoke_test_encoder(enc, caps.vaapi_render_node.as_deref(), uses_vaapi) {
                    debug!("[Probe] Verified HW encoder: {}", enc);
                    caps.encoders.push(enc.to_string());
                } else {
                    debug!("[Probe] Skipping unusable HW encoder: {}", enc);
                }
            } else {
                caps.encoders.push(enc.to_string());
            }
        }

        caps
    }

    fn smoke_test_encoder(enc: &str, vaapi_dev: Option<&str>, uses_vaapi: bool) -> bool {
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-hide_banner", "-loglevel", "error"]);
        if uses_vaapi {
            if let Some(dev) = vaapi_dev {
                cmd.args(["-vaapi_device", dev]);
            }
        }
        cmd.args([
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=640x360:d=0.04",
            "-frames:v",
            "1",
        ]);
        if uses_vaapi {
            cmd.args(["-vf", "format=nv12,hwupload"]);
        } else {
            cmd.args(["-pix_fmt", "yuv420p"]);
        }
        cmd.args(["-c:v", enc, "-f", "null", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        cmd.status().map(|s| s.success()).unwrap_or(false)
    }

    pub fn has_encoder(&self, name: &str) -> bool {
        self.encoders.iter().any(|e| e == name)
    }

    /// Select the best genuinely working encoder matching `(container, codec, hw_accel, width, height)`.
    pub fn select_encoder_for_size(
        &self,
        container: ContainerFormat,
        codec: VideoCodec,
        hw: HardwareEncoder,
        width: u32,
        height: u32,
    ) -> Option<SelectedEncoder> {
        if !self.ffmpeg_available {
            return None;
        }

        if matches!(container, ContainerFormat::Gif) {
            return Some(SelectedEncoder {
                ffmpeg_name: "gif".to_string(),
                is_hardware: false,
                uses_vaapi: false,
                vaapi_device: None,
            });
        }

        let vaapi_ok = self.vaapi_render_node.is_some();
        // AMD/Intel AV1 VA-API typically requires >= 320x128
        let av1_vaapi_size_ok = width >= 320 && height >= 128;

        let candidates: Vec<(&str, bool, bool)> = match (codec, container, hw) {
            (VideoCodec::Auto, ContainerFormat::Webm, HardwareEncoder::Cpu) => {
                vec![("libvpx-vp9", false, false), ("libsvtav1", false, false), ("libvpx", false, false)]
            }
            (VideoCodec::Auto, ContainerFormat::Webm, _) => vec![
                ("vp9_vaapi", true, true),
                ("av1_vaapi", true, true),
                ("libvpx-vp9", false, false),
                ("libsvtav1", false, false),
                ("libvpx", false, false),
            ],
            // Auto on MKV / MP4 / MpegTs: prefer any verified HW encoder (incl. av1_vaapi on Fedora!),
            // then clean CPU H.264 / OpenH264 / VP9 / MPEG4 fallback
            (VideoCodec::Auto, _, HardwareEncoder::Cpu) => vec![
                ("libx264", false, false),
                ("libopenh264", false, false),
                ("libsvtav1", false, false),
                ("libvpx-vp9", false, false),
                ("mpeg4", false, false),
            ],
            (VideoCodec::Auto, ContainerFormat::MpegTs, _) => vec![
                ("h264_nvenc", true, false),
                ("h264_videotoolbox", true, false),
                ("h264_vaapi", true, true),
                ("libx264", false, false),
                ("libopenh264", false, false),
                ("mpeg4", false, false),
            ],
            (VideoCodec::Auto, _, _) => vec![
                ("h264_nvenc", true, false),
                ("h264_videotoolbox", true, false),
                ("h264_vaapi", true, true),
                ("av1_vaapi", true, true),
                ("hevc_vaapi", true, true),
                ("vp9_vaapi", true, true),
                ("libx264", false, false),
                ("libopenh264", false, false),
                ("libvpx-vp9", false, false),
                ("mpeg4", false, false),
            ],
            (VideoCodec::H264, _, HardwareEncoder::Vaapi) => vec![
                ("h264_vaapi", true, true),
                ("libx264", false, false),
                ("libopenh264", false, false),
            ],
            (VideoCodec::H264, _, HardwareEncoder::Nvenc) => vec![
                ("h264_nvenc", true, false),
                ("libx264", false, false),
                ("libopenh264", false, false),
            ],
            (VideoCodec::H264, _, HardwareEncoder::VideoToolbox) => vec![
                ("h264_videotoolbox", true, false),
                ("libx264", false, false),
                ("libopenh264", false, false),
            ],
            (VideoCodec::H264, _, HardwareEncoder::MediaFoundation) => {
                vec![("h264_mf", true, false), ("libx264", false, false)]
            }
            (VideoCodec::H264, _, HardwareEncoder::Cpu) => vec![
                ("libx264", false, false),
                ("libopenh264", false, false),
                ("mpeg4", false, false),
            ],
            (VideoCodec::H264, _, HardwareEncoder::Auto) => vec![
                ("h264_nvenc", true, false),
                ("h264_videotoolbox", true, false),
                ("h264_vaapi", true, true),
                ("libx264", false, false),
                ("libopenh264", false, false),
                ("mpeg4", false, false),
            ],
            (VideoCodec::Hevc, _, HardwareEncoder::Cpu) => {
                vec![("libx265", false, false), ("libopenh264", false, false)]
            }
            (VideoCodec::Hevc, _, _) => vec![
                ("hevc_nvenc", true, false),
                ("hevc_videotoolbox", true, false),
                ("hevc_vaapi", true, true),
                ("libx265", false, false),
                ("libopenh264", false, false),
            ],
            (VideoCodec::Av1, _, HardwareEncoder::Cpu) => {
                vec![("libsvtav1", false, false), ("libopenh264", false, false)]
            }
            (VideoCodec::Av1, _, _) => vec![
                ("av1_nvenc", true, false),
                ("av1_vaapi", true, true),
                ("libsvtav1", false, false),
                ("libopenh264", false, false),
            ],
            (VideoCodec::Vp9, _, HardwareEncoder::Cpu) => {
                vec![("libvpx-vp9", false, false), ("libvpx", false, false)]
            }
            (VideoCodec::Vp9, _, _) => vec![
                ("vp9_vaapi", true, true),
                ("libvpx-vp9", false, false),
                ("libvpx", false, false),
            ],
        };

        for (name, is_hw, uses_vaapi) in candidates {
            if uses_vaapi && !vaapi_ok {
                continue;
            }
            if name == "av1_vaapi" && !av1_vaapi_size_ok {
                continue;
            }
            if self.has_encoder(name) {
                return Some(SelectedEncoder {
                    ffmpeg_name: name.to_string(),
                    is_hardware: is_hw,
                    uses_vaapi,
                    vaapi_device: if uses_vaapi {
                        self.vaapi_render_node.clone()
                    } else {
                        None
                    },
                });
            }
        }

        None
    }

    pub fn select_encoder(
        &self,
        container: ContainerFormat,
        codec: VideoCodec,
        hw: HardwareEncoder,
    ) -> Option<SelectedEncoder> {
        self.select_encoder_for_size(container, codec, hw, 1280, 720)
    }
}
