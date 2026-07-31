// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    linux_sync_gtk_theme_with_os();

    spider_tauri_lib::run();
}

/// Native menubar on Linux uses GTK colors. Mixed light bar + dark-theme ink
/// (or the reverse) makes labels invisible. Pick one coherent theme that
/// matches the desktop light/dark preference so contrast stays readable.
#[cfg(target_os = "linux")]
fn linux_sync_gtk_theme_with_os() {
    use std::path::Path;
    use std::process::Command;

    // Respect an explicit user/desktop override.
    if std::env::var_os("GTK_THEME").is_some() {
        return;
    }

    let prefer_dark = desktop_prefers_dark();
    let theme = if prefer_dark {
        first_installed_theme(&[
            "adw-gtk3-dark",
            "Adwaita-dark",
            "Adwaita:dark",
            "Breeze-Dark",
        ])
        .unwrap_or_else(|| "Adwaita:dark".into())
    } else {
        first_installed_theme(&["adw-gtk3", "Adwaita", "Breeze"]).unwrap_or_else(|| "Adwaita".into())
    };

    std::env::set_var("GTK_THEME", &theme);
    std::env::set_var(
        "GTK_APPLICATION_PREFER_DARK_THEME",
        if prefer_dark { "1" } else { "0" },
    );

    fn desktop_prefers_dark() -> bool {
        if let Some(scheme) = gsettings_string("org.gnome.desktop.interface", "color-scheme") {
            let s = scheme.to_ascii_lowercase();
            if s.contains("prefer-dark") {
                return true;
            }
            if s.contains("prefer-light") {
                return false;
            }
        }
        if let Some(gtk_theme) = gsettings_string("org.gnome.desktop.interface", "gtk-theme") {
            let t = gtk_theme.to_ascii_lowercase();
            if t.contains("dark") {
                return true;
            }
        }
        // KDE Plasma
        if let Ok(output) = Command::new("kreadconfig5")
            .args([
                "--file",
                "kdeglobals",
                "--group",
                "General",
                "--key",
                "ColorScheme",
            ])
            .output()
        {
            if output.status.success() {
                let value = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
                if value.contains("dark") {
                    return true;
                }
            }
        }
        false
    }

    fn gsettings_string(schema: &str, key: &str) -> Option<String> {
        let output = Command::new("gsettings")
            .args(["get", schema, key])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Some(raw.trim_matches('\'').trim_matches('"').to_string())
    }

    fn first_installed_theme(candidates: &[&str]) -> Option<String> {
        let dirs = [
            "/usr/share/themes",
            "/usr/local/share/themes",
            &format!(
                "{}/.themes",
                std::env::var("HOME").unwrap_or_default()
            ),
            &format!(
                "{}/.local/share/themes",
                std::env::var("HOME").unwrap_or_default()
            ),
        ];
        for name in candidates {
            // `Adwaita:dark` is a variant, not a directory name.
            let dir_name = name.split(':').next().unwrap_or(name);
            let exists = dirs.iter().any(|dir| Path::new(dir).join(dir_name).is_dir())
                || (*name == "Adwaita" || *name == "Adwaita:dark" || *name == "Adwaita-dark");
            if exists {
                return Some((*name).to_string());
            }
        }
        None
    }
}
