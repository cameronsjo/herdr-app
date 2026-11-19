//! FORK(outside-agents): `watchFolder` on Linux asks git about the watched
//! folder. An agent that can write only that folder (`outside-agents/`) can make
//! it a repo with a `.git/config` of its own; git must not run its
//! `core.fsmonitor` command.

use super::linux::watch_directories;

#[test]
fn a_planted_fsmonitor_never_runs() {
    let root = crate::test_tmp().join(format!("herdr-fork-fsmonitor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let folder = root.join("outside-agents");
    std::fs::create_dir_all(&folder).unwrap();
    let git = |args: &[&str]| std::process::Command::new("git").args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"]).arg("-C").arg(&folder).args(args).output().unwrap();
    assert!(git(&["init", "-q", "."]).status.success());
    let ran = root.join("RAN");
    let hook = root.join("hook.sh");
    std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\nexit 1\n", ran.display())).unwrap();
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(git(&["config", "core.fsmonitor", &hook.to_string_lossy()]).status.success());
    std::fs::write(folder.join("x.json"), "{}").unwrap();
    // Without the fork's override, this is the call that ran the hook.
    let _ = watch_directories(&folder);
    let planted = ran.exists();
    let _ = std::fs::remove_dir_all(&root);
    assert!(!planted, "watchFolder's git calls ran the folder's core.fsmonitor");
}
