//! FORK(outside-agents): the app home's `outside-agents/` folder is the one a
//! sandboxed agent may write. Code that would otherwise run a program on a
//! folder's say-so (git reading a planted `.git/config`, `.gitignore` or a pipe
//! named like one) checks here first and leaves that folder alone.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long `watchFolder`'s git calls may take before they are stopped.
pub const GIT_DEADLINE: Duration = Duration::from_secs(5);

/// `command`'s output, or None when it can't start or runs past `deadline`
/// (then it is killed). A git call reads ignore files in every untracked
/// folder it walks, so one inside a watched repo (`outside-agents/`, when the
/// app home sits in a repo) can plant a pipe that would hang it for good.
pub fn output_within(command: &mut std::process::Command, deadline: Duration) -> Option<std::process::Output> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().ok()?;
    // Read both pipes on threads, so a full pipe never stalls the child.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut out = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut out);
            }
            out
        })
    };
    let stdout = drain(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let stderr = drain(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    Some(std::process::Output { status, stdout: stdout.join().ok()?, stderr: stderr.join().ok()? })
}

/// True when `path` is the app's `outside-agents/` folder or anything in it.
pub fn untrusted_folder(path: &Path) -> bool {
    within(path, &crate::mods::app_dir().join("outside-agents"))
}

/// True when `path` is `dir` or inside it, links resolved where they exist.
pub fn within(path: &Path, dir: &Path) -> bool {
    let real = |p: &Path| -> PathBuf { std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()) };
    path.starts_with(dir) || real(path).starts_with(real(dir))
}

/// Opens `path` for `readTextFile` only when it is a plain file (a link to
/// one too): never waiting on a pipe, never reading a device. On Unix the
/// open itself doesn't wait (O_NONBLOCK), so a pipe swapped in for a listed
/// name between a mod's listing and its read can't hang the read.
pub fn open_plain(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other("Not a plain file"));
    }
    Ok(file)
}

// libc's O_NONBLOCK, without the dependency on Linux, for the targets the
// fork ships (other Unix targets open without it). Reads of a plain file
// ignore it.
#[cfg(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")))]
const O_NONBLOCK: i32 = 0o4000;
#[cfg(target_os = "macos")]
const O_NONBLOCK: i32 = 0x0004;
#[cfg(not(any(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")), target_os = "macos")))]
const O_NONBLOCK: i32 = 0;

#[cfg(test)]
mod tests {
    use super::{open_plain, within};

    #[cfg(unix)]
    #[test]
    fn a_command_past_its_deadline_is_stopped() {
        let start = std::time::Instant::now();
        assert!(super::output_within(std::process::Command::new("sleep").arg("30"), std::time::Duration::from_millis(200)).is_none());
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        let out = super::output_within(std::process::Command::new("echo").arg("hi"), super::GIT_DEADLINE).unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout, b"hi\n");
    }

    #[cfg(unix)]
    #[test]
    fn a_pipe_or_a_device_is_refused_without_waiting() {
        let root = crate::test_tmp().join(format!("herdr-fork-plain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let pipe = root.join("b.json");
        assert!(std::process::Command::new("mkfifo").arg(&pipe).status().unwrap().success());
        // Without O_NONBLOCK this open waits for a writer forever.
        assert!(open_plain(&pipe).is_err());
        assert!(open_plain(std::path::Path::new("/dev/null")).is_err());
        assert!(open_plain(&root).is_err());
        std::fs::write(root.join("a.json"), "{}").unwrap();
        std::os::unix::fs::symlink(root.join("a.json"), root.join("link.json")).unwrap();
        assert!(open_plain(&root.join("a.json")).is_ok());
        assert!(open_plain(&root.join("link.json")).is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_folder_and_what_is_in_it_are_untrusted() {
        let root = crate::test_tmp().join(format!("herdr-fork-untrusted-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("home/outside-agents");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        assert!(within(&dir, &dir));
        assert!(within(&dir.join("sub"), &dir));
        assert!(!within(&root.join("home"), &dir));
        assert!(!within(&root.join("home/outside-agents-2"), &dir));
        #[cfg(unix)]
        {
            // The same folder reached through a link.
            std::os::unix::fs::symlink(&dir, root.join("link")).unwrap();
            assert!(within(&root.join("link"), &dir));
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
