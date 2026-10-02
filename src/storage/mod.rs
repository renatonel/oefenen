//! Data directory resolution and the low level file primitives.
//!
//! Everything is stored in plain text files, no database:
//!
//! ```text
//! <data_dir>/
//!   profiles/
//!     <profile_id>/
//!       profile.toml
//!       modules/
//!         <module_id>/
//!           results.jsonl
//! ```
//!
//! * `profile.toml` and settings are rewritten atomically ([`write_atomic`]).
//! * `results.jsonl` is append-only, one JSON object per line ([`jsonl`]).
//!
//! Modules never see this module: they get a [`ModuleStore`], scoped to their
//! own profile and their own id.

pub mod error;
pub mod jsonl;
pub mod module_store;
pub mod paths;
pub mod profiles;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub use error::StorageError;
pub use module_store::ModuleStore;
pub use profiles::{PROFILE_FILE_VERSION, ProfileFile, ProfileStore};

/// Application name used for the platform specific data directory.
pub const APP_NAME: &str = "oefenen";

/// Counter that keeps temp file names unique within one process.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Resolves the data directory.
///
/// Precedence: the `--data-dir` argument, then the platform specific location
/// from [`directories`]. The returned path is created if it does not exist.
pub fn resolve_data_dir(override_path: Option<&Path>) -> Result<PathBuf, StorageError> {
    let data_dir = match override_path {
        Some(path) => path.to_path_buf(),
        None => directories::ProjectDirs::from("", "", APP_NAME)
            .ok_or_else(|| {
                StorageError::DataDirUnavailable("geen platformmap beschikbaar".to_string())
            })?
            .data_dir()
            .to_path_buf(),
    };

    fs::create_dir_all(&data_dir).map_err(|e| StorageError::io(&data_dir, e))?;
    Ok(data_dir)
}

/// Writes `contents` to `path` without ever leaving a half-written file.
///
/// The bytes go to a uniquely named temp file in the same directory (same
/// filesystem, so the rename is atomic), are flushed to disk, and only then
/// replace the target. A reader either sees the old file or the new one.
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), StorageError> {
    let dir = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| StorageError::InvalidId {
            kind: "pad",
            value: path.display().to_string(),
            reason: "pad zonder bovenliggende map".to_string(),
        })?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| StorageError::InvalidId {
            kind: "pad",
            value: path.display().to_string(),
            reason: "pad zonder bestandsnaam".to_string(),
        })?;

    fs::create_dir_all(dir).map_err(|e| StorageError::io(dir, e))?;
    let temp_path = temp_path_for(dir, file_name);

    let write_result = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&temp_path)?;
        file.write_all(contents)?;
        // Flush before the rename: otherwise a crash could leave an empty or
        // partially written file behind under the real name.
        file.sync_all()
    })();

    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(StorageError::io(&temp_path, error));
    }

    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(StorageError::io(path, error));
    }

    Ok(())
}

/// `<dir>/.<file_name>.<pid>-<n>.tmp`
fn temp_path_for(dir: &Path, file_name: &str) -> PathBuf {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(".{file_name}.{}-{counter}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn override_path_wins_and_is_created() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("nested").join("data");
        let resolved = resolve_data_dir(Some(&target)).unwrap();
        assert_eq!(resolved, target);
        assert!(target.is_dir());
    }

    #[test]
    fn without_an_override_a_platform_directory_is_used() {
        let resolved = resolve_data_dir(None).unwrap();
        assert!(resolved.is_absolute());
        assert!(resolved.is_dir());
    }

    #[test]
    fn atomic_write_replaces_the_file_and_cleans_up() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("profile.toml");

        write_atomic(&target, b"eerste").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "eerste");

        write_atomic(&target, b"tweede").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "tweede");

        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["profile.toml".to_string()]);
    }

    #[test]
    fn a_failed_write_leaves_no_temp_file_behind() {
        let dir = TempDir::new().unwrap();
        let locked = dir.path().join("locked");
        fs::create_dir_all(&locked).unwrap();

        let mut permissions = fs::metadata(&locked).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(0o500);
        }
        #[cfg(not(unix))]
        permissions.set_readonly(true);
        fs::set_permissions(&locked, permissions).unwrap();

        let result = write_atomic(&locked.join("profile.toml"), b"inhoud");

        // Restore permissions before inspecting, also on Windows.
        let mut permissions = fs::metadata(&locked).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(0o700);
        }
        #[cfg(not(unix))]
        permissions.set_readonly(false);
        fs::set_permissions(&locked, permissions).unwrap();

        // Running as root ignores permissions; there is nothing to assert then.
        if result.is_ok() {
            return;
        }
        assert!(matches!(result, Err(StorageError::Io { .. })));
        let names: Vec<String> = fs::read_dir(&locked)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.is_empty(),
            "tijdelijke bestanden achtergebleven: {names:?}"
        );
    }

    #[test]
    fn a_target_that_is_a_directory_is_rejected() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("profile.toml");
        fs::create_dir_all(&target).unwrap();
        assert!(write_atomic(&target, b"x").is_err());
        // Writing to a directory path itself is rejected as well.
        assert!(write_atomic(dir.path(), b"x").is_err());
    }
}
