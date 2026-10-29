//! Configures process resource limits before the desktop runtime starts.

#[cfg(unix)]
use std::io;

#[cfg(unix)]
pub(crate) const REQUESTED_OPEN_FILE_LIMIT: u64 = 65_536;

#[cfg(unix)]
pub(crate) struct OpenFileLimit {
    pub(crate) previous: u64,
    pub(crate) effective: u64,
    pub(crate) hard: u64,
}

#[cfg(unix)]
pub(crate) fn raise_open_file_limit() -> io::Result<OpenFileLimit> {
    let mut limits = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `limits` points to writable memory for one `rlimit` value.
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limits) } != 0 {
        return Err(io::Error::last_os_error());
    }

    let previous = limits.rlim_cur;
    let target = limits
        .rlim_max
        .min(REQUESTED_OPEN_FILE_LIMIT as libc::rlim_t);
    if previous < target {
        let requested = libc::rlimit {
            rlim_cur: target,
            rlim_max: limits.rlim_max,
        };
        // SAFETY: `requested` is a valid `rlimit` value and does not raise the hard limit.
        if unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &requested) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }

    let mut effective = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `effective` points to writable memory for one `rlimit` value.
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut effective) } != 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(OpenFileLimit {
        previous,
        effective: effective.rlim_cur,
        hard: effective.rlim_max,
    })
}
