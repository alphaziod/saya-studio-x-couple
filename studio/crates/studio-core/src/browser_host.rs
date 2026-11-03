//! The existing Civitai native-messaging protocol, served directly by the installed executable.
use crate::Result;
use serde_json::{json, Value};
use std::{io::{Read, Write}, path::{Path, PathBuf}};

pub const HOST: &str = "com.saya.studio";
pub const ORIGIN: &str = "chrome-extension://hgbfcjoeaddihdpccfhcgeohohfejidm/";
const MAX_MESSAGE: usize = 1024 * 1024;

/// Chrome supplies the extension origin as argv[1]. No UI or bridge is started in host mode.
pub fn try_run() -> Option<i32> {
    let origin = std::env::args().nth(1)?;
    if !origin.starts_with("chrome-extension://") { return None; }
    if origin != ORIGIN { return Some(1); }
    let Some(token) = token_path() else { return Some(1); };
    Some(if serve(&mut std::io::stdin().lock(), &mut std::io::stdout().lock(), &token).is_ok() { 0 } else { 1 })
}

fn token_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SAYA_CIVITAI_TOKEN_FILE").filter(|v| !v.is_empty()) { return Some(path.into()); }
    #[cfg(any(windows, target_os = "macos"))]
    let base = dirs::data_dir()?;
    #[cfg(not(any(windows, target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from)
        .unwrap_or_else(|| crate::paths::home_dir().unwrap_or_default().join(".local/share"));
    Some(base.join("studio.saya.desktop/civitai-token"))
}

fn serve(input: &mut impl Read, output: &mut impl Write, token_file: &Path) -> Result<()> {
    loop {
        let mut header = [0; 4];
        if input.read(&mut header[..1])? == 0 { return Ok(()); }
        input.read_exact(&mut header[1..])?;
        let length = u32::from_le_bytes(header) as usize;
        if length == 0 || length > MAX_MESSAGE { return Err("Invalid native message length".into()); }
        let mut body = vec![0; length];
        input.read_exact(&mut body)?;
        let message: Value = serde_json::from_slice(&body)?;
        if !message.is_object() { return Err("Native message must be an object".into()); }
        let kind = message["type"].as_str().or_else(|| message["action"].as_str()).unwrap_or("").trim();
        let token = std::fs::read_to_string(token_file).unwrap_or_default().trim().to_owned();
        let response = match kind {
            "ping" | "status" => json!({"ok": true, "host": HOST, "tokenAvailable": !token.is_empty()}),
            "get_token" | "token" if !token.is_empty() => json!({"ok": true, "host": HOST, "token": token}),
            "get_token" | "token" => json!({"ok": false, "error": "jeton Saya Studio introuvable"}),
            _ => json!({"ok": false, "error": "action inconnue"}),
        };
        let bytes = serde_json::to_vec(&response)?;
        output.write_all(&(bytes.len() as u32).to_le_bytes())?;
        output.write_all(&bytes)?;
        output.flush()?;
    }
}

#[cfg(any(windows, target_os = "macos"))]
pub fn register(data: &Path, executable: &Path) -> Result<()> {
    let manifest = json!({"name": HOST, "description": "Saya Studio Civitai token host",
        "path": executable, "type": "stdio", "allowed_origins": [ORIGIN]});
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    #[cfg(target_os = "macos")]
    {
        let base = dirs::data_dir().ok_or("Dossier utilisateur absent")?;
        for browser in ["Google/Chrome", "Google/ChromeForTesting", "Chromium", "BraveSoftware/Brave-Browser", "Microsoft Edge"] {
            let directory = base.join(browser).join("NativeMessagingHosts");
            std::fs::create_dir_all(&directory)?;
            crate::fsutil::write_atomic(&directory.join(format!("{HOST}.json")), &bytes)?;
        }
        let _ = data;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let manifest = data.join(format!("{HOST}.json"));
        crate::fsutil::write_atomic(&manifest, &bytes)?;
        for browser in [r"Google\Chrome", "Chromium", r"BraveSoftware\Brave-Browser", r"Microsoft\Edge"] {
            for view in ["/reg:32", "/reg:64"] {
                let key = format!(r"HKCU\Software\{browser}\NativeMessagingHosts\{HOST}");
                let output = std::process::Command::new("reg.exe").creation_flags(0x08000000)
                    .args(["add", &key, "/ve", "/t", "REG_SZ", "/d"]).arg(&manifest).args(["/f", view]).output()?;
                if !output.status.success() { return Err(format!("Native host registration failed: {}", String::from_utf8_lossy(&output.stderr)).into()); }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(value: Value) -> Vec<u8> {
        let bytes = serde_json::to_vec(&value).unwrap();
        [(bytes.len() as u32).to_le_bytes().to_vec(), bytes].concat()
    }
    #[test]
    fn native_host_protocol_is_framed_and_bounded() {
        let path = std::env::temp_dir().join(format!("native-host-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, "synthetic-token\n").unwrap();
        let mut input = std::io::Cursor::new(frame(json!({"type":"get_token"})));
        let mut output = vec![];
        serve(&mut input, &mut output, &path).unwrap();
        let length = u32::from_le_bytes(output[..4].try_into().unwrap()) as usize;
        assert_eq!(output.len(), length + 4);
        assert_eq!(serde_json::from_slice::<Value>(&output[4..]).unwrap()["token"], "synthetic-token");
        for bytes in [vec![0,0,0,0], ((MAX_MESSAGE+1) as u32).to_le_bytes().to_vec(), vec![2,0,0,0,b'{'], frame(json!([]))] {
            assert!(serve(&mut std::io::Cursor::new(bytes), &mut vec![], &path).is_err());
        }
        std::fs::remove_file(&path).unwrap();
        let mut output = vec![];
        serve(&mut std::io::Cursor::new(frame(json!({"type":"get_token"}))), &mut output, &path).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&output[4..]).unwrap()["ok"], false);
    }
}
