use std::os::unix::fs::MetadataExt;
use std::path::Path;

fn check_one(path: &Path, user: u32) -> Result<(), String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("{}: {}", path.display(), crate::sys::error_message(&error)))?;
    if metadata.uid() != user && metadata.uid() != 0 {
        return Err(format!("{} belongs to another user", path.display()));
    }
    if metadata.mode() & 0o022 != 0 {
        return Err(format!(
            "{} is writable by other users (fix with: chmod go-w {})",
            path.display(),
            path.display()
        ));
    }
    Ok(())
}

pub(crate) fn check_with_user(path: &Path, user: u32) -> Result<(), String> {
    check_one(path, user)?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        check_one(parent, user)?;
    }
    Ok(())
}

pub(crate) fn check(path: &Path) -> Result<(), String> {
    check_with_user(path, unsafe { libc::geteuid() })
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn writable_by_others_is_rejected() {
        let directory = std::env::temp_dir().join(format!("grfsh_trust_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755)).unwrap();
        let file = directory.join("init.grfsh");
        std::fs::write(&file, "echo hi\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(check(&file).is_ok());

        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o666)).unwrap();
        assert!(
            check(&file)
                .unwrap_err()
                .contains("writable by other users")
        );

        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(
            check_with_user(&file, 54321)
                .unwrap_err()
                .contains("another user")
        );
        let _ = std::fs::remove_dir_all(&directory);
    }
}
