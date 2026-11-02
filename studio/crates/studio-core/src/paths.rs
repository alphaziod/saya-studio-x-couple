//! Per-user locations and packaged resources for Linux, Windows and macOS.

use std::path::PathBuf;

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Home directory; Windows uses USERPROFILE when HOME is absent.
pub fn home_dir() -> Result<PathBuf, String> {
    env_path("HOME").or_else(|| {
        #[cfg(windows)]
        { env_path("USERPROFILE") }
        #[cfg(not(windows))]
        { None }
    }).ok_or_else(|| "Variable d'environnement HOME absente".to_string())
}

/// ComfyUI installation: `SAYA_COMFYUI_DIR`, otherwise `$HOME/AI/ComfyUI`.
pub fn comfyui_dir() -> Result<PathBuf, String> {
    match env_path("SAYA_COMFYUI_DIR") {
        Some(dir) => Ok(dir),
        None => home_dir().map(|home| home.join("AI/ComfyUI")),
    }
}

/// The user's own folder for everything personal the app works with (draw engine, workflows,
/// app settings): `SAYA_CONFIG_DIR`, otherwise `$HOME/Documents/saya studio config`.
/// Its layout is described by the `INDEX.md` it contains.
pub fn config_dir() -> PathBuf {
    env_path("SAYA_CONFIG_DIR").unwrap_or_else(|| {
        home_dir()
            .unwrap_or_default()
            .join("Documents/saya studio config")
    })
}

/// Draw engine directory: `SAYA_ENGINE_DIR`, otherwise `<config>/GROK BUILD/generez`, otherwise
/// the former location `$HOME/Documents/GROK BUILD/generez` (the caller reports "not found").
pub fn engine_dir() -> PathBuf {
    env_path("SAYA_ENGINE_DIR").unwrap_or_else(|| {
        let configured = config_dir().join("GROK BUILD/generez");
        if configured.is_dir() {
            configured
        } else {
            home_dir()
                .unwrap_or_default()
                .join("Documents/GROK BUILD/generez")
        }
    })
}

/// Packaged data (presets, helpers): `/usr/share/saya-studio` for system packages, otherwise
/// `<binary>/../share/saya-studio` (AppImage, Nix store, Flatpak `/app`, Snap).
pub fn share_dir() -> PathBuf {
    if let Some(dir) = RESOURCE_DIR.get() {
        return dir.clone();
    }
    let system = PathBuf::from("/usr/share/saya-studio");
    if system.is_dir() {
        return system;
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|bin| bin.join("../share/saya-studio")))
        .filter(|dir| dir.is_dir())
        .unwrap_or(system)
}

/// Base presets (six-pass workflows, validated characters): `SAYA_STUDIO_PRESETS`, otherwise
/// `<config>/workflows` when it holds a `preset.json`, otherwise the packaged copy.
pub fn presets_dir() -> PathBuf {
    env_path("SAYA_STUDIO_PRESETS").unwrap_or_else(|| {
        let configured = config_dir().join("workflows");
        if configured.join("preset.json").is_file() {
            configured
        } else {
            share_dir().join("presets")
        }
    })
}

/// Pictures folder: `$XDG_PICTURES_DIR`, otherwise `$HOME/Images` when it exists, else `$HOME/Pictures`.
pub fn pictures_dir() -> Result<PathBuf, String> {
    if let Some(dir) = env_path("XDG_PICTURES_DIR") {
        return Ok(dir);
    }
    let home = home_dir()?;
    let images = home.join("Images");
    Ok(if images.is_dir() { images } else { home.join("Pictures") })
}

// The shell supplies Tauri's platform-specific resource directory before startup tasks.
static RESOURCE_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
pub fn set_resource_dir(dir: PathBuf) {
    let _ = RESOURCE_DIR.set(dir);
}

/// Python must be installed by the user; an explicit interpreter also supports virtualenvs.
pub fn python_program() -> String {
    std::env::var("SAYA_PYTHON").ok().filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| if cfg!(windows) { "python" } else { "python3" }.into())
}

#[cfg(test)]
mod platform_tests {
    #[test]
    fn home_is_an_absolute_existing_directory() {
        let home = super::home_dir().unwrap();
        assert!(home.is_absolute());
        assert!(home.is_dir());
    }
    #[test]
    fn database_opens_in_a_native_path_with_spaces() {
        let dir = std::env::temp_dir().join(format!("saya platform {}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = crate::Database::open(&dir.join("studio.sqlite3")).unwrap();
        assert!(db.preferences().is_ok());
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
