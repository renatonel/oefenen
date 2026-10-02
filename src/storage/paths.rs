//! Names of the files and directories we create, plus the rules that keep
//! identifiers usable as a single path segment.
//!
//! Every path we build from user- or module-provided data goes through
//! [`safe_join`], so a module can never read or write outside its own directory.

use std::path::{Path, PathBuf};

use super::StorageError;

/// Directory that holds one sub-directory per profile.
pub const PROFILES_DIR: &str = "profiles";
/// Per-profile metadata file.
pub const PROFILE_FILE: &str = "profile.toml";
/// Directory inside a profile that holds the per-module data.
pub const MODULES_DIR: &str = "modules";
/// Append-only result log written by the modules.
pub const RESULTS_FILE: &str = "results.jsonl";

/// Longest accepted identifier. Also keeps profile directories comfortable to
/// type or copy by hand.
pub const MAX_ID_LEN: usize = 48;
/// Longest display name we accept.
pub const MAX_NAME_LEN: usize = 32;

/// Checks that `value` can be used as a single, harmless path segment.
///
/// Ids become directory names, so the rule is deliberately strict: lowercase
/// ASCII letters, digits and dashes only. That rules out `.`, `..`, path
/// separators, absolute paths and non-ASCII surprises on every platform.
pub fn validate_segment(kind: &'static str, value: &str) -> Result<(), StorageError> {
    if value.is_empty() {
        return Err(StorageError::InvalidId {
            kind,
            value: value.to_string(),
            reason: "mag niet leeg zijn".to_string(),
        });
    }
    if value.len() > MAX_ID_LEN {
        return Err(StorageError::InvalidId {
            kind,
            value: value.to_string(),
            reason: format!("mag maximaal {MAX_ID_LEN} tekens lang zijn"),
        });
    }
    if let Some(bad) = value.chars().find(|c| !is_id_char(*c)) {
        return Err(StorageError::InvalidId {
            kind,
            value: value.to_string(),
            reason: format!("het teken {bad:?} is niet toegestaan (gebruik a-z, 0-9 en -)"),
        });
    }
    Ok(())
}

fn is_id_char(value: char) -> bool {
    value.is_ascii_lowercase() || value.is_ascii_digit() || value == '-'
}

/// Joins `segments` onto `base`, refusing anything that could escape `base`.
///
/// The check is lexical on purpose: the base directory may not exist yet.
pub fn safe_join(base: &Path, segments: &[&str]) -> Result<PathBuf, StorageError> {
    let mut path = base.to_path_buf();
    for segment in segments {
        reject_traversal(segment)?;
        path.push(segment);
    }
    // Belt and braces: `Path::join` cleans `..`, so re-check the result.
    if !path.starts_with(base) {
        return Err(StorageError::PathEscapes {
            base: base.to_path_buf(),
            path,
        });
    }
    Ok(path)
}

fn reject_traversal(segment: &str) -> Result<(), StorageError> {
    let is_separated = segment.contains('/') || segment.contains('\\');
    let is_absolute = Path::new(segment).is_absolute() || segment.starts_with('/');
    let is_relative_marker = segment == "." || segment == "..";
    if is_separated || is_absolute || is_relative_marker || segment.contains('\0') {
        return Err(StorageError::InvalidId {
            kind: "pad",
            value: segment.to_string(),
            reason: "mag geen pad zijn".to_string(),
        });
    }
    Ok(())
}

/// Turns arbitrary text into a readable path fragment: lowercase, ASCII, dashes.
pub fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    let mut pending_dash = false;
    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(character.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
        if slug.len() >= MAX_ID_LEN {
            break;
        }
    }
    if slug.is_empty() {
        "profiel".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_readable_ids() {
        validate_segment("profiel", "sem-3f9a2c71").unwrap();
        validate_segment("module", "tafel-1").unwrap();
    }

    #[test]
    fn rejects_path_like_ids() {
        for bad in [
            "", ".", "..", "../evil", "a/b", "a\\b", "/abs", "C:\\win", "Sem", "s m",
        ] {
            let result = validate_segment("profiel", bad);
            assert!(result.is_err(), "{bad:?} zou geweigerd moeten worden");
        }
    }

    #[test]
    fn rejects_ids_that_are_too_long() {
        let long = "a".repeat(MAX_ID_LEN + 1);
        assert!(validate_segment("profiel", &long).is_err());
    }

    #[test]
    fn safe_join_builds_paths_inside_the_base() {
        let base = Path::new("/data/profiles/sem-1");
        let joined = safe_join(base, &[MODULES_DIR, "tafel-1", RESULTS_FILE]).unwrap();
        assert_eq!(
            joined,
            Path::new("/data/profiles/sem-1/modules/tafel-1/results.jsonl")
        );
    }

    #[test]
    fn safe_join_rejects_traversal_and_absolute_segments() {
        let base = Path::new("/data/profiles/sem-1");
        for bad in [
            "..",
            ".",
            "../evil",
            "../../etc",
            "/etc/passwd",
            "a/b",
            "a\\b",
        ] {
            let result = safe_join(base, &[MODULES_DIR, bad]);
            assert!(result.is_err(), "{bad:?} zou geweigerd moeten worden");
        }
    }

    #[test]
    fn slugify_produces_safe_readable_slugs() {
        assert_eq!(slugify("Sem"), "sem");
        assert_eq!(slugify("Sém van Dijk"), "s-m-van-dijk");
        assert_eq!(slugify("  Jan-Jansen 2  "), "jan-jansen-2");
        assert_eq!(slugify("?!"), "profiel");
        assert_eq!(slugify(""), "profiel");
        // A slug that came out of `slugify` is always a valid id.
        validate_segment("profiel", &slugify("Æther Øvre 9")).unwrap();
    }

    #[test]
    fn slugify_truncates_to_the_max_id_length() {
        let long = "a".repeat(120);
        assert_eq!(slugify(&long).len(), MAX_ID_LEN);
    }
}
