//! Native, recoverable trash only. Never fall back to permanent deletion.
use crate::Result;
use std::path::Path;

#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) fn recycle(path: &Path) -> Result<()> {
    let output = std::process::Command::new("gio").args(["trash", "--"]).arg(path).output()?;
    if !output.status.success() {
        return Err(format!("Corbeille indisponible : {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn recycle_mac(path: &Path) -> Result<std::path::PathBuf> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};
    let text = path.to_str().ok_or("Chemin non UTF-8")?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(text));
    let mut destination = None;
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, Some(&mut destination))
        .map_err(|e| format!("Corbeille indisponible : {e}"))?;
    destination.and_then(|u| u.path()).map(|p| p.to_string().into())
        .ok_or_else(|| "Emplacement de corbeille non retourné".into())
}

#[cfg(target_os = "macos")]
pub(crate) fn recycle(path: &Path) -> Result<()> { recycle_mac(path).map(|_| ()) }

#[cfg(windows)]
pub(crate) fn recycle(path: &Path) -> Result<()> {
    let path = path.to_str().ok_or("Chemin non UTF-8")?;
    // The Shell API requires ordinary drive/UNC names, not Rust's verbatim canonical prefix.
    let path = if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else { path.strip_prefix(r"\\?\").unwrap_or(path).to_owned() };
    std::thread::spawn(move || -> Result<()> {
        use windows::{core::PCWSTR, Win32::{System::Com::*, UI::Shell::*}};
        // A fresh thread owns this apartment and all its COM objects.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            struct Apartment;
            impl Drop for Apartment { fn drop(&mut self) { unsafe { CoUninitialize() } } }
            let _apartment = Apartment;
            let operation: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)?;
            operation.SetOperationFlags(FOF_NO_UI | FOF_NO_CONNECTED_ELEMENTS | FOFX_EARLYFAILURE | FOFX_RECYCLEONDELETE | FOFX_ADDUNDORECORD)?;
            let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)?;
            operation.DeleteItem(&item, None)?;
            operation.PerformOperations()?;
            if operation.GetAnyOperationsAborted()?.as_bool() {
                return Err("Mise à la corbeille annulée".into());
            }
        }
        Ok(())
    }).join().map_err(|_| "Échec du service de corbeille")?
}

#[cfg(all(test, any(windows, target_os = "macos")))]
mod tests {
    #[test]
    #[ignore = "Moves and restores only a synthetic file on disposable CI runners"]
    fn native_recycle_roundtrip() {
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
        let root = std::env::temp_dir().canonicalize().unwrap();
        #[cfg(windows)]
        let root = std::path::PathBuf::from(root.to_str().unwrap().trim_start_matches(r"\\?\"));
        let dir = root.join(format!("saya-trash-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("recoverable-test.saya-fixture");
        let bytes = b"synthetic recoverable file, no user data";
        std::fs::write(&path, bytes).unwrap();
        #[cfg(target_os = "macos")]
        {
            let trashed = super::recycle_mac(&path).unwrap();
            assert!(!path.exists());
            assert_eq!(std::fs::read(&trashed).unwrap(), bytes);
            std::fs::rename(trashed, &path).unwrap();
        }
        #[cfg(windows)]
        {
            super::recycle(&path.canonicalize().unwrap()).unwrap();
            assert!(!path.exists());
            let status = std::process::Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", r#"
$ErrorActionPreference = 'Stop'
$original = $env:SAYA_TEST_TRASH_FILE
$folder = Split-Path -LiteralPath $original
$shell = New-Object -ComObject Shell.Application
$bin = $shell.NameSpace(10)
$matches = @($bin.Items() | Where-Object { $_.ExtendedProperty('System.Recycle.DeletedFrom') -eq $folder })
if ($matches.Count -ne 1) { throw 'Synthetic file not uniquely present in Recycle Bin' }
$matches[0].InvokeVerb('undelete')
for ($i=0; $i -lt 50 -and !(Test-Path -LiteralPath $original); $i++) { Start-Sleep -Milliseconds 200 }
if (!(Test-Path -LiteralPath $original)) { throw 'Restore failed' }
"#]).env("SAYA_TEST_TRASH_FILE", &path).status().unwrap();
            assert!(status.success());
        }
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
