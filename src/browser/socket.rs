//! FORK(browser): the control socket is this user's only. Agent access to a
//! browser is the person's switch, and an agent reaches it through the
//! socket, so another user must not (`control.rs` calls these two).

use std::io;
use std::path::Path;

/// Binds the socket owner-only: born 0600 (umask around the bind, so there's
/// no moment where it's wider; macOS), then set 0600 again (every Unix).
pub fn bind_private<L>(path: &Path, bind: impl FnOnce(&Path) -> io::Result<L>) -> io::Result<L> {
    #[cfg(target_os = "macos")]
    // SAFETY: umask only swaps the process's file-mode mask; restored right after.
    let mask = unsafe { libc::umask(0o077) };
    let bound = bind(path);
    #[cfg(target_os = "macos")]
    // SAFETY: as above.
    unsafe {
        libc::umask(mask)
    };
    let listener = bound?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
            tracing::warn!("cannot make {} private: {e}", path.display());
        }
    }
    Ok(listener)
}

/// A peer running as another user is dropped before anything is read (macOS:
/// `getpeereid`). Elsewhere the socket's 0600 mode is the only check.
#[cfg(target_os = "macos")]
pub fn same_user(stream: &std::os::unix::net::UnixStream) -> bool {
    use std::os::fd::AsRawFd as _;
    let (mut uid, mut gid) = (0, 0);
    // SAFETY: a live socket fd and two out-params of the right type.
    let ok = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } == 0;
    // SAFETY: geteuid has no preconditions.
    ok && uid == unsafe { libc::geteuid() }
}

/// Elsewhere the socket's 0600 mode is the only check.
#[cfg(not(target_os = "macos"))]
pub fn same_user<S>(_stream: &S) -> bool {
    true
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;
    use std::os::unix::net::{UnixListener, UnixStream};

    #[test]
    fn the_socket_is_owner_only_and_its_owner_gets_through() {
        let dir = std::env::temp_dir().join(format!("herdr-sock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.sock");
        let _ = std::fs::remove_file(&path);
        let listener = bind_private(&path, |p| UnixListener::bind(p)).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let _client = UnixStream::connect(&path).unwrap();
        let (served, _) = listener.accept().unwrap();
        assert!(same_user(&served), "this user's own connection is dropped");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
