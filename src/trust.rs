use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

const STICKY: u32 = 0o1000;
const LINK_LIMIT: usize = 40;

fn check_one(path: &Path, user: u32) -> Result<(), String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("{}: {}", path.display(), crate::sys::error_message(&error)))?;
    if metadata.uid() != user && metadata.uid() != 0 {
        return Err(format!("{} belongs to another user", path.display()));
    }
    let shared_directory = metadata.is_dir() && metadata.mode() & STICKY != 0;
    if metadata.mode() & 0o022 != 0 && !shared_directory {
        return Err(format!(
            "{} is writable by other users (fix with: chmod go-w {})",
            path.display(),
            path.display()
        ));
    }
    Ok(())
}

fn check_ancestors(directory: &Path, user: u32) -> Result<(), String> {
    let resolved = std::fs::canonicalize(directory).map_err(|error| {
        format!(
            "{}: {}",
            directory.display(),
            crate::sys::error_message(&error)
        )
    })?;
    let absolute = std::env::current_dir()
        .map(|current| current.join(directory))
        .unwrap_or_else(|_| directory.to_path_buf());
    absolute
        .ancestors()
        .chain(resolved.ancestors())
        .try_for_each(|ancestor| check_one(ancestor, user))
}

fn parent_of(path: &Path) -> PathBuf {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

pub(crate) fn check_with_user(path: &Path, user: u32) -> Result<(), String> {
    check_one(path, user)?;
    let mut current = path.to_path_buf();
    for _ in 0..LINK_LIMIT {
        let parent = parent_of(&current);
        check_ancestors(&parent, user)?;
        let Ok(target) = std::fs::read_link(&current) else {
            return Ok(());
        };
        current = parent.join(target);
    }
    Err(format!(
        "{}: too many levels of symbolic links",
        path.display()
    ))
}

pub(crate) fn check(path: &Path) -> Result<(), String> {
    check_with_user(path, unsafe { libc::geteuid() })
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn private_directory(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755)).unwrap();
        directory
    }

    fn set_mode(path: &Path, mode: u32) {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn writable_by_others_is_rejected() {
        let directory = private_directory("grfsh_trust");
        let file = directory.join("init.grfsh");
        std::fs::write(&file, "echo hi\n").unwrap();
        set_mode(&file, 0o644);
        assert!(check(&file).is_ok());

        set_mode(&file, 0o666);
        assert!(
            check(&file)
                .unwrap_err()
                .contains("writable by other users")
        );

        set_mode(&file, 0o644);
        assert!(
            check_with_user(&file, 54321)
                .unwrap_err()
                .contains("another user")
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn symlinks_are_checked_where_they_point() {
        let directory = private_directory("grfsh_trust_link");
        let shared = directory.join("shared");
        let home = directory.join("home");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        set_mode(&home, 0o755);
        let rc = shared.join("rc");
        std::fs::write(&rc, "echo hi\n").unwrap();
        set_mode(&rc, 0o644);
        let link = home.join(".grfshrc");
        std::os::unix::fs::symlink("../shared/rc", &link).unwrap();
        set_mode(&shared, 0o755);
        assert!(check(&link).is_ok());
        set_mode(&shared, 0o777);
        let problem = check(&link).unwrap_err();
        assert!(
            problem.contains("shared is writable by other users"),
            "{problem}"
        );
        set_mode(&shared, 0o1777);
        assert!(check(&link).is_ok());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn plugin_ancestors_are_checked() {
        let directory = private_directory("grfsh_trust_plugins");
        let plugin = directory.join("plugins/tool");
        std::fs::create_dir_all(&plugin).unwrap();
        set_mode(&plugin, 0o755);
        let init = plugin.join("init.grfsh");
        std::fs::write(&init, "echo hi\n").unwrap();
        set_mode(&init, 0o644);
        set_mode(&directory.join("plugins"), 0o755);
        assert!(check(&init).is_ok());
        set_mode(&directory.join("plugins"), 0o777);
        let problem = check(&init).unwrap_err();
        assert!(problem.contains("plugins is writable"), "{problem}");
        let _ = std::fs::remove_dir_all(&directory);
    }
}
