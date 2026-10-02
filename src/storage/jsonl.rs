//! Append-only JSON Lines storage.
//!
//! One JSON object per line, written with `O_APPEND` so concurrent writers
//! cannot interleave a line. A truncated final line (for example after a power
//! cut) is reported as an error, but skipping it is possible with
//! [`read_all_lossy`].

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::StorageError;

/// Appends one JSON object as a single line.
///
/// Returns the number of records now in the file, so callers can report
/// "record N" without re-reading the whole log.
pub fn append_line<T: Serialize>(path: &Path, value: &T) -> Result<u64, StorageError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| StorageError::io(parent, e))?;
    }
    let mut line = serde_json::to_string(value).map_err(|source| StorageError::JsonWrite {
        path: path.to_path_buf(),
        source,
    })?;
    // `serde_json` never emits newlines inside a compact object, but be
    // explicit: a record is exactly one line.
    line.push('\n');

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| StorageError::io(path, e))?;
    file.write_all(line.as_bytes())
        .map_err(|e| StorageError::io(path, e))?;

    Ok(count_lines(path))
}

/// Number of records currently in the file.
pub fn count_lines(path: &Path) -> u64 {
    match File::open(path) {
        Ok(file) => BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter(|line| !line.trim().is_empty())
            .count() as u64,
        Err(_) => 0,
    }
}

/// Reads every record from `path`.
///
/// A missing file is not an error: it simply holds no records yet.
pub fn read_all<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>, StorageError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = File::open(path).map_err(|e| StorageError::io(path, e))?;
    let mut records = Vec::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| StorageError::io(path, e))?;
        if line.trim().is_empty() {
            continue;
        }
        let record = serde_json::from_str(&line).map_err(|source| StorageError::JsonLine {
            path: path.to_path_buf(),
            line_number: index + 1,
            source,
        })?;
        records.push(record);
    }
    Ok(records)
}

/// Like [`read_all`], but silently drops lines that cannot be parsed.
///
/// Used when showing history to a learner: one damaged record must not make
/// the whole log unreadable.
pub fn read_all_lossy<T: DeserializeOwned>(path: &Path) -> Vec<T> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use tempfile::TempDir;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Sample {
        name: String,
        value: i32,
    }

    fn sample() -> Sample {
        Sample {
            name: "regel één".to_string(),
            value: 1,
        }
    }

    #[test]
    fn append_and_read_back() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested").join("results.jsonl");

        // The parent directory is created on demand.
        assert_eq!(append_line(&path, &sample()).unwrap(), 1);
        assert_eq!(
            append_line(
                &path,
                &Sample {
                    name: "regel twee".into(),
                    value: 2
                }
            )
            .unwrap(),
            2
        );

        let records: Vec<Sample> = read_all(&path).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0], sample());
        assert_eq!(records[1].value, 2);
    }

    #[test]
    fn reads_one_object_per_line() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("results.jsonl");
        append_line(&path, &sample()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 1);

        // Newlines inside values must not split a record.
        append_line(
            &path,
            &Sample {
                name: "a\nb".into(),
                value: 3,
            },
        )
        .unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert_eq!(raw.lines().count(), 2);
        assert_eq!(read_all::<Sample>(&path).unwrap().len(), 2);
    }

    #[test]
    fn missing_file_reads_as_empty() {
        let dir = TempDir::new().unwrap();
        let records: Vec<Sample> = read_all(&dir.path().join("nothing.jsonl")).unwrap();
        assert!(records.is_empty());
        assert!(read_all_lossy::<Sample>(&dir.path().join("nothing.jsonl")).is_empty());
        assert_eq!(count_lines(&dir.path().join("nothing.jsonl")), 0);
    }

    #[test]
    fn a_damaged_line_is_reported_with_its_line_number() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("results.jsonl");
        append_line(&path, &sample()).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{not json}\n")
            .unwrap();

        let error = read_all::<Sample>(&path).unwrap_err();
        match error {
            StorageError::JsonLine { line_number, .. } => assert_eq!(line_number, 2),
            other => panic!("onverwachte fout: {other:?}"),
        }
        // The lossy reader simply skips the damaged line.
        assert_eq!(read_all_lossy::<Sample>(&path).len(), 1);
    }
}
