//! Automated installer & status checker for native KDE Plasma 6 Plasmoid,
//! GNOME Shell Extension, D-Bus activation service, and systemd user unit.

use std::path::{Path, PathBuf};
use std::process::Command;

const PLASMOID_ID: &str = "org.kde.plasma.regiontoshare";
const GNOME_UUID: &str = "region-to-share@solarpush.github.io";

fn locate_integration_dir(sub: &str) -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("integrations").join(sub),
        PathBuf::from("/usr/share/region-to-share/integrations").join(sub),
        PathBuf::from("/app/share/region-to-share/integrations").join(sub),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("../share/region-to-share/integrations").join(sub)))
            .unwrap_or_default(),
    ];
    candidates.into_iter().find(|p| p.exists())
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".local/share")
    })
}

pub fn check_installed() -> (bool, bool) {
    let kde_user = data_dir().join("plasma/plasmoids").join(PLASMOID_ID);
    let kde_sys = PathBuf::from("/usr/share/plasma/plasmoids").join(PLASMOID_ID);
    let kde = kde_user.exists() || kde_sys.exists();

    let gnome_user = data_dir().join("gnome-shell/extensions").join(GNOME_UUID);
    let gnome_sys = PathBuf::from("/usr/share/gnome-shell/extensions").join(GNOME_UUID);
    let gnome = gnome_user.exists() || gnome_sys.exists();
    (kde, gnome)
}

pub fn print_status() -> i32 {
    let (kde, gnome) = check_installed();
    let dbus_svc = data_dir().join("dbus-1/services/io.github.solarpush.RegionToShare.service");
    println!(
        "KDE Plasma 6 Plasmoid ({PLASMOID_ID}): {}",
        if kde { "installed" } else { "not installed" }
    );
    println!(
        "GNOME Shell Extension ({GNOME_UUID}): {}",
        if gnome { "installed" } else { "not installed" }
    );
    println!(
        "D-Bus Activation Service: {}",
        if dbus_svc.exists() {
            format!("installed ({})", dbus_svc.display())
        } else {
            "not installed".into()
        }
    );
    0
}

pub fn install_kde() -> i32 {
    let Some(src) = locate_integration_dir("kde-plasmoid") else {
        eprintln!("Could not locate `integrations/kde-plasmoid` directory.");
        return 1;
    };

    let _ = install_dbus_and_systemd();

    let (already, _) = check_installed();
    let flag = if already { "-u" } else { "-i" };
    let status = Command::new("kpackagetool6")
        .args(["-t", "Plasma/Applet", flag])
        .arg(&src)
        .status();

    match status {
        Ok(st) if st.success() => {
            println!("✓ KDE Plasma 6 Plasmoid installed ({PLASMOID_ID}).");
            println!("  Add it to your panel: Right-click Panel -> Add Widgets -> 'Region to Share'.");
            println!("  Quick test window   : plasmoidviewer -a {}", src.display());
            0
        }
        Ok(st) => {
            eprintln!("kpackagetool6 exited with {st}");
            1
        }
        Err(e) => {
            eprintln!("Failed to run kpackagetool6 ({e}). Is KDE Plasma 6 installed?");
            1
        }
    }
}

pub fn uninstall_kde() -> i32 {
    match Command::new("kpackagetool6")
        .args(["-t", "Plasma/Applet", "-r", PLASMOID_ID])
        .status()
    {
        Ok(st) if st.success() => {
            println!("✓ Uninstalled KDE Plasma 6 Plasmoid ({PLASMOID_ID}).");
            0
        }
        Ok(st) => {
            eprintln!("kpackagetool6 exited with {st}");
            1
        }
        Err(e) => {
            eprintln!("Failed to run kpackagetool6: {e}");
            1
        }
    }
}

pub fn install_gnome() -> i32 {
    let Some(src) = locate_integration_dir("gnome-extension") else {
        eprintln!("Could not locate `integrations/gnome-extension` directory.");
        return 1;
    };

    let _ = install_dbus_and_systemd();

    let dest = data_dir().join("gnome-shell/extensions").join(GNOME_UUID);
    if let Err(e) = copy_dir_recursive(&src, &dest) {
        eprintln!("Failed to copy GNOME extension files: {e}");
        return 1;
    }

    let schemas = dest.join("schemas");
    if schemas.exists() {
        let _ = Command::new("glib-compile-schemas").arg(&schemas).status();
    }

    let _ = Command::new("gnome-extensions")
        .args(["enable", GNOME_UUID])
        .status();

    println!("✓ GNOME Shell Extension installed at {}", dest.display());
    println!("  If running GNOME Wayland, log out/in once or enable it in the Extensions app.");
    0
}

pub fn uninstall_gnome() -> i32 {
    let _ = Command::new("gnome-extensions")
        .args(["disable", GNOME_UUID])
        .status();
    let dest = data_dir().join("gnome-shell/extensions").join(GNOME_UUID);
    if dest.exists() {
        if let Err(e) = std::fs::remove_dir_all(&dest) {
            eprintln!("Failed to remove {}: {e}", dest.display());
            return 1;
        }
    }
    println!("✓ Uninstalled GNOME Shell Extension ({GNOME_UUID}).");
    0
}

pub fn install_dbus_and_systemd() -> i32 {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "region-to-share".to_string());

    let dbus_dir = data_dir().join("dbus-1/services");
    let _ = std::fs::create_dir_all(&dbus_dir);
    let dbus_file = dbus_dir.join("io.github.solarpush.RegionToShare.service");
    let dbus_content = format!(
        "[D-BUS Service]\nName=io.github.solarpush.RegionToShare\nExec={exe} daemon\n"
    );
    if let Err(e) = std::fs::write(&dbus_file, dbus_content) {
        eprintln!("Failed to write D-Bus service file: {e}");
        return 1;
    }

    let systemd_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("systemd/user");
    let _ = std::fs::create_dir_all(&systemd_dir);
    let unit_file = systemd_dir.join("region-to-share.service");
    let unit_content = format!(
        "[Unit]\n\
         Description=Region to Share D-Bus Session Service\n\
         PartOf=graphical-session.target\n\
         After=graphical-session.target\n\n\
         [Service]\n\
         Type=dbus\n\
         BusName=io.github.solarpush.RegionToShare\n\
         ExecStart={exe} daemon\n\
         Restart=on-failure\n\n\
         [Install]\n\
         WantedBy=graphical-session.target\n"
    );
    let _ = std::fs::write(&unit_file, unit_content);

    println!("✓ Installed D-Bus service at {}", dbus_file.display());
    println!("✓ Installed systemd user unit at {}", unit_file.display());
    0
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
