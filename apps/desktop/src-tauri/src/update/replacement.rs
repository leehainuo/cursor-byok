use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
    process::{Child, Command, ExitCode},
    thread,
    time::{Duration, Instant},
};

const APPLY_ARG: &str = "--apply-portable-update";
const TARGET_ARG: &str = "--update-target";
const PID_ARG: &str = "--update-wait-pid";
const HANDSHAKE_ARG: &str = "--update-handshake";
pub(super) const READY_ARG: &str = "--portable-update-ready";
const PROCESS_WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const READY_WAIT_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) struct ReplacementRequest {
    target: PathBuf,
    pid: u32,
    handshake: PathBuf,
}

pub(super) fn request_from_args() -> Result<Option<ReplacementRequest>, String> {
    let args = std::env::args_os().collect::<Vec<_>>();
    if !args.iter().any(|arg| arg == APPLY_ARG) {
        return Ok(None);
    }
    let target = PathBuf::from(
        argument_value(&args, TARGET_ARG).ok_or_else(|| format!("{TARGET_ARG} is required"))?,
    );
    let pid = argument_value(&args, PID_ARG)
        .ok_or_else(|| format!("{PID_ARG} is required"))?
        .to_string_lossy()
        .parse::<u32>()
        .map_err(|error| format!("invalid {PID_ARG}: {error}"))?;
    let handshake = PathBuf::from(
        argument_value(&args, HANDSHAKE_ARG)
            .ok_or_else(|| format!("{HANDSHAKE_ARG} is required"))?,
    );
    validate_target(&target).map_err(|error| error.to_string())?;
    Ok(Some(ReplacementRequest {
        target,
        pid,
        handshake,
    }))
}

pub(super) fn ready_marker_from_args() -> Option<PathBuf> {
    let args = std::env::args_os().collect::<Vec<_>>();
    argument_value(&args, READY_ARG).map(PathBuf::from)
}

fn argument_value<'a>(args: &'a [OsString], name: &str) -> Option<&'a OsStr> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .map(OsString::as_os_str)
}

fn validate_target(target: &Path) -> io::Result<()> {
    if !target.is_absolute() || !target.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "update target must be an existing absolute file",
        ));
    }
    let source_name = std::env::current_exe()?
        .file_name()
        .map(OsStr::to_os_string)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "updater has no file name"))?;
    if target.file_name() != Some(source_name.as_os_str()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "update target file name does not match the updater",
        ));
    }
    Ok(())
}

pub(super) fn run(request: ReplacementRequest) -> ExitCode {
    match run_inner(request) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("portable update replacement failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner(request: ReplacementRequest) -> io::Result<()> {
    wait_for_process(request.pid, PROCESS_WAIT_TIMEOUT, &request.handshake)?;
    remove_file_if_exists(&request.handshake)?;
    let source = std::env::current_exe()?;
    let backup = backup_path(&request.target);
    let ready = source.with_extension("ready");
    remove_file_if_exists(&ready)?;

    if let Err(error) = install_staged(&source, &request.target, &backup) {
        relaunch(&request.target);
        return Err(error);
    }
    let mut child = match Command::new(&request.target)
        .arg(READY_ARG)
        .arg(&ready)
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            restore_backup(&request.target, &backup)?;
            relaunch(&request.target);
            return Err(error);
        }
    };

    match wait_until_ready(&mut child, &ready, READY_WAIT_TIMEOUT) {
        Ok(()) => {
            let _ = fs::remove_file(&backup);
            let _ = fs::remove_file(&ready);
            Ok(())
        }
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            restore_backup(&request.target, &backup)?;
            relaunch(&request.target);
            Err(error)
        }
    }
}

fn relaunch(target: &Path) {
    if target.is_file() {
        let _ = Command::new(target).spawn();
    }
}

fn remove_file_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn backup_path(target: &Path) -> PathBuf {
    path_with_suffix(target, ".old")
}

fn pending_path(target: &Path) -> PathBuf {
    path_with_suffix(target, ".new")
}

fn path_with_suffix(target: &Path, suffix: &str) -> PathBuf {
    let mut name = target.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn install_staged(source: &Path, target: &Path, backup: &Path) -> io::Result<()> {
    let pending = pending_path(target);
    remove_file_if_exists(&pending)?;
    fs::copy(source, &pending)?;

    let result = activate_pending(&pending, target, backup);
    if result.is_err() {
        let _ = fs::remove_file(&pending);
    }
    result
}

fn activate_pending(pending: &Path, target: &Path, backup: &Path) -> io::Result<()> {
    remove_file_if_exists(backup)?;
    fs::rename(target, backup)?;
    if let Err(error) = fs::rename(pending, target) {
        if let Err(restore_error) = restore_backup(target, backup) {
            return Err(io::Error::other(format!(
                "failed to install update ({error}) and restore the original executable ({restore_error})"
            )));
        }
        return Err(error);
    }
    Ok(())
}

fn restore_backup(target: &Path, backup: &Path) -> io::Result<()> {
    remove_file_if_exists(target)?;
    fs::rename(backup, target)
}

fn wait_until_ready(child: &mut Child, marker: &Path, timeout: Duration) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if marker.is_file() {
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            return Err(io::Error::other(format!(
                "updated application exited before startup completed: {status}"
            )));
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "updated application did not report a successful startup",
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(windows)]
fn wait_for_process(pid: u32, timeout: Duration, handshake: &Path) -> io::Result<()> {
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        System::Threading::{OpenProcess, WaitForSingleObject},
    };

    const SYNCHRONIZE_ACCESS: u32 = 0x0010_0000;
    let handle = unsafe { OpenProcess(SYNCHRONIZE_ACCESS, 0, pid) };
    if handle.is_null() {
        let error = unsafe { GetLastError() };
        return if error == ERROR_INVALID_PARAMETER {
            fs::write(handshake, b"started")
        } else {
            Err(io::Error::from_raw_os_error(error as i32))
        };
    }
    if let Err(error) = fs::write(handshake, b"started") {
        unsafe { CloseHandle(handle) };
        return Err(error);
    }
    let milliseconds = timeout.as_millis().min(u32::MAX as u128) as u32;
    let result = unsafe { WaitForSingleObject(handle, milliseconds) };
    unsafe { CloseHandle(handle) };
    match result {
        WAIT_OBJECT_0 => Ok(()),
        WAIT_TIMEOUT => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "running application did not exit before the update timeout",
        )),
        _ => Err(io::Error::last_os_error()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_file_can_be_restored() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.exe");
        let target = directory.path().join("target.exe");
        let backup = backup_path(&target);
        fs::write(&source, b"new").unwrap();
        fs::write(&target, b"old").unwrap();

        install_staged(&source, &target, &backup).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(fs::read(&backup).unwrap(), b"old");

        restore_backup(&target, &backup).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"old");
        assert!(!backup.exists());
    }

    #[test]
    fn missing_pending_file_restores_original_after_backup() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target.exe");
        let pending = pending_path(&target);
        let backup = backup_path(&target);
        fs::write(&target, b"old").unwrap();

        assert!(activate_pending(&pending, &target, &backup).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"old");
        assert!(!backup.exists());
    }

    #[test]
    fn missing_source_preserves_original_file() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("missing.exe");
        let target = directory.path().join("target.exe");
        let backup = backup_path(&target);
        fs::write(&target, b"old").unwrap();

        assert!(install_staged(&source, &target, &backup).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"old");
        assert!(!backup.exists());
        assert!(!pending_path(&target).exists());
    }
}
