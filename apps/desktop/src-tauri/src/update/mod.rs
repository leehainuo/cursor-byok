use std::process::ExitCode;

#[cfg(target_os = "windows")]
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    process::Command,
};
#[cfg(any(target_os = "windows", test))]
use std::{
    io::{Cursor, Read},
    path::Path,
};

use serde::Serialize;
use tauri::AppHandle;

#[cfg(target_os = "windows")]
use tauri_plugin_updater::UpdaterExt;

#[cfg(target_os = "windows")]
mod replacement;

#[cfg(target_os = "windows")]
const PORTABLE_UPDATE_ENDPOINT: &str =
    "https://github.com/leookun/cursor-byok/releases/latest/download/portable-latest.json";
#[cfg(any(target_os = "windows", test))]
const WINDOWS_PAYLOAD_NAME: &str = "cursor-byok-desktop.exe";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PortableUpdateInfo {
    version: String,
}

pub fn run_replacement_if_requested() -> Option<ExitCode> {
    #[cfg(target_os = "windows")]
    {
        match replacement::request_from_args() {
            Ok(Some(request)) => return Some(replacement::run(request)),
            Ok(None) => {}
            Err(error) => {
                eprintln!("invalid portable update replacement request: {error}");
                return Some(ExitCode::FAILURE);
            }
        }
    }
    None
}

pub(crate) fn signal_ready_if_requested() -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    if let Some(path) = replacement::ready_marker_from_args() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, b"ready")?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn check_portable_update(
    app: AppHandle,
) -> Result<Option<PortableUpdateInfo>, String> {
    #[cfg(target_os = "windows")]
    {
        let update = portable_update(&app).await?;
        Ok(update.map(|update| PortableUpdateInfo {
            version: update.version,
        }))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        Err("portable updates are only supported on Windows".into())
    }
}

#[tauri::command]
pub(crate) async fn install_portable_update(
    app: AppHandle,
    expected_version: String,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let update = portable_update(&app)
            .await?
            .ok_or_else(|| "the selected update is no longer available".to_string())?;
        if update.version != expected_version {
            return Err(format!(
                "available update changed from {expected_version} to {}",
                update.version
            ));
        }

        let target = std::env::current_exe()
            .map_err(|error| format!("failed to locate the running executable: {error}"))?;
        ensure_target_writable(&target)
            .map_err(|error| format!("the application directory is not writable: {error}"))?;

        let bytes = update
            .download(|_, _| {}, || {})
            .await
            .map_err(|error| format!("failed to download or verify the update: {error}"))?;
        let payload = extract_windows_payload(&bytes)
            .map_err(|error| format!("invalid Windows update archive: {error}"))?;
        let staged = stage_payload(&target, &payload)
            .map_err(|error| format!("failed to stage the update: {error}"))?;

        let handshake = staged.with_extension("started");
        let _ = fs::remove_file(&handshake);
        let mut replacement = Command::new(&staged)
            .arg("--apply-portable-update")
            .arg("--update-target")
            .arg(&target)
            .arg("--update-wait-pid")
            .arg(std::process::id().to_string())
            .arg("--update-handshake")
            .arg(&handshake)
            .spawn()
            .map_err(|error| format!("failed to start the update replacement process: {error}"))?;
        wait_for_replacement_start(&mut replacement, &handshake).await?;

        app.exit(0);
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, expected_version);
        Err("portable updates are only supported on Windows".into())
    }
}

#[cfg(target_os = "windows")]
async fn portable_update(app: &AppHandle) -> Result<Option<tauri_plugin_updater::Update>, String> {
    let endpoint = PORTABLE_UPDATE_ENDPOINT
        .parse()
        .map_err(|error| format!("invalid portable update endpoint: {error}"))?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|error| format!("failed to configure the updater: {error}"))?
        .build()
        .map_err(|error| format!("failed to initialize the updater: {error}"))?;
    updater
        .check()
        .await
        .map_err(|error| format!("failed to check for updates: {error}"))
}

#[cfg(target_os = "windows")]
async fn wait_for_replacement_start(
    child: &mut std::process::Child,
    handshake: &Path,
) -> Result<(), String> {
    let wait = async {
        loop {
            if handshake.is_file() {
                return Ok(());
            }
            if let Some(status) = child
                .try_wait()
                .map_err(|error| format!("failed to inspect replacement process: {error}"))?
            {
                return Err(format!(
                    "update replacement process exited before it was ready: {status}"
                ));
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    };
    let result = match tokio::time::timeout(std::time::Duration::from_secs(5), wait).await {
        Ok(result) => result,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("update replacement process did not become ready".into());
        }
    };
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

#[cfg(target_os = "windows")]
fn ensure_target_writable(target: &Path) -> std::io::Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| std::io::Error::other("application executable has no parent directory"))?;
    let probe = parent.join(format!(
        ".cursor-byok-update-write-test-{}",
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)?;
    file.write_all(b"test")?;
    drop(file);
    fs::remove_file(probe)
}

#[cfg(any(target_os = "windows", test))]
fn extract_windows_payload(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("failed to open ZIP: {error}"))?;
    if archive.len() != 1 {
        return Err("archive must contain exactly one file".into());
    }
    let mut entry = archive
        .by_index(0)
        .map_err(|error| format!("failed to read ZIP entry: {error}"))?;
    let name = Path::new(entry.name())
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "archive entry has an invalid file name".to_string())?;
    if name != WINDOWS_PAYLOAD_NAME || entry.is_dir() {
        return Err(format!(
            "expected {WINDOWS_PAYLOAD_NAME}, found {}",
            entry.name()
        ));
    }
    let mut payload = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut payload)
        .map_err(|error| format!("failed to extract executable: {error}"))?;
    if payload.len() < 2 || &payload[..2] != b"MZ" {
        return Err("payload is not a Windows executable".into());
    }
    Ok(payload)
}

#[cfg(target_os = "windows")]
fn stage_payload(target: &Path, payload: &[u8]) -> std::io::Result<PathBuf> {
    let directory = tempfile::Builder::new()
        .prefix("cursor-byok-portable-update-")
        .tempdir()?;
    let name = target
        .file_name()
        .ok_or_else(|| std::io::Error::other("application executable has no file name"))?;
    let path = directory.path().join(name);
    fs::write(&path, payload)?;
    let _ = directory.keep();
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn update_zip(name: &str, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut bytes);
            archive
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            archive.write_all(payload).unwrap();
            archive.finish().unwrap();
        }
        bytes.into_inner()
    }

    #[test]
    fn extracts_the_single_expected_windows_executable() {
        let bytes = update_zip(WINDOWS_PAYLOAD_NAME, b"MZpayload");
        assert_eq!(extract_windows_payload(&bytes).unwrap(), b"MZpayload");
    }

    #[test]
    fn rejects_unexpected_or_non_executable_payloads() {
        let wrong_name = update_zip("other.exe", b"MZpayload");
        assert!(extract_windows_payload(&wrong_name).is_err());
        let wrong_content = update_zip(WINDOWS_PAYLOAD_NAME, b"not an executable");
        assert!(extract_windows_payload(&wrong_content).is_err());
    }
}
