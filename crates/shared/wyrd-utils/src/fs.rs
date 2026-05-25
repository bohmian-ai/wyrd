//! Filesystem, digest, and directory-copy helpers shared by cards.

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};
use walkdir::WalkDir;
use wyrd_spec::card::data::{DataSchema, DataStats};

use crate::{WyrdUtilsError, WyrdUtilsResult};

/// Require an existing local filesystem path.
///
/// # Errors
/// Returns an error when the path does not exist.
pub fn require_local_path(path: &Path) -> WyrdUtilsResult<()> {
    if path.exists() {
        Ok(())
    } else {
        Err(WyrdUtilsError::LocalPathMissing(path.to_path_buf()))
    }
}

/// Require an existing local file.
///
/// # Errors
/// Returns an error when the path does not exist or is not a file.
pub fn require_local_file(path: &Path) -> WyrdUtilsResult<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(WyrdUtilsError::LocalFileMissing(path.to_path_buf()))
    }
}

/// Compute byte count and SHA-256 stats for a local file.
///
/// # Errors
/// Returns an error when the path is not a file or cannot be read.
pub fn data_stats_for_file(path: &Path, schema: Option<&DataSchema>) -> WyrdUtilsResult<DataStats> {
    let (byte_count, sha256) = digest_file(path)?;
    Ok(DataStats {
        row_count: None,
        col_count: schema.map(|schema| u32::try_from(schema.columns.len()).unwrap_or(u32::MAX)),
        byte_count,
        sha256,
    })
}

/// Compute byte count and SHA-256 stats for a local file or directory.
///
/// Directories are hashed deterministically by sorted relative path and file
/// bytes. The byte count is the sum of file byte counts.
///
/// # Errors
/// Returns an error when the path is missing or cannot be read.
pub fn data_stats_for_path(path: &Path, schema: Option<&DataSchema>) -> WyrdUtilsResult<DataStats> {
    require_local_path(path)?;
    let (byte_count, sha256) = if path.is_file() {
        digest_file(path)?
    } else if path.is_dir() {
        digest_directory(path)?
    } else {
        return Err(WyrdUtilsError::UnsupportedPath(path.to_path_buf()));
    };

    Ok(DataStats {
        row_count: None,
        col_count: schema.map(|schema| u32::try_from(schema.columns.len()).unwrap_or(u32::MAX)),
        byte_count,
        sha256,
    })
}

/// Copy every file under `source` into `dest`, preserving relative paths.
///
/// # Errors
/// Returns an error when walking, directory creation, or file copy fails.
pub fn copy_manifest_files(source: &Path, dest: &Path) -> WyrdUtilsResult<()> {
    require_local_path(source)?;
    for entry in WalkDir::new(source) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let relative = entry.path().strip_prefix(source)?;
        let target = dest.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry.path(), target)?;
    }
    Ok(())
}

fn digest_file(path: &Path) -> WyrdUtilsResult<(u64, String)> {
    require_local_file(path)?;
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut byte_count = 0_u64;
    let mut buffer = [0_u8; 8192];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        byte_count += read as u64;
        hasher.update(&buffer[..read]);
    }

    Ok((byte_count, format!("{:x}", hasher.finalize())))
}

fn digest_directory(path: &Path) -> WyrdUtilsResult<(u64, String)> {
    let mut files = Vec::new();
    for entry in WalkDir::new(path) {
        let entry = entry?;
        if entry.file_type().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }
    files.sort();

    let mut byte_count = 0_u64;
    let mut hasher = Sha256::new();
    for file in files {
        let relative = file.strip_prefix(path)?;
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0]);
        let (file_byte_count, file_sha256) = digest_file(&file)?;
        byte_count += file_byte_count;
        hasher.update(file_sha256.as_bytes());
        hasher.update([0]);
    }

    Ok((byte_count, format!("{:x}", hasher.finalize())))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use crate::fs::{
        copy_manifest_files, data_stats_for_file, data_stats_for_path, require_local_file,
    };
    use crate::uuid7;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("wyrd-utils-{name}-{}", uuid7()));
        fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    #[test]
    fn data_stats_for_file_matches_known_sha256() {
        let dir = temp_dir("stats-file");
        let path = dir.join("fixture.txt");
        fs::write(&path, b"abc").expect("fixture should be written");

        let stats = data_stats_for_file(&path, None).expect("stats should be computed");

        assert_eq!(stats.byte_count, 3);
        assert_eq!(
            stats.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        fs::remove_dir_all(dir).expect("temp dir should be removed");
    }

    #[test]
    fn data_stats_for_path_hashes_directory_deterministically() {
        let dir = temp_dir("stats-dir");
        fs::create_dir_all(dir.join("nested")).expect("nested dir should be created");
        fs::write(dir.join("b.txt"), b"b").expect("b fixture should be written");
        fs::write(dir.join("nested/a.txt"), b"a").expect("a fixture should be written");

        let first = data_stats_for_path(&dir, None).expect("first stats should be computed");
        let second = data_stats_for_path(&dir, None).expect("second stats should be computed");

        assert_eq!(first.byte_count, 2);
        assert_eq!(first.sha256, second.sha256);
        fs::remove_dir_all(dir).expect("temp dir should be removed");
    }

    #[test]
    fn copy_manifest_files_preserves_relative_paths() {
        let source = temp_dir("copy-source");
        let dest = temp_dir("copy-dest");
        fs::create_dir_all(source.join("nested")).expect("nested dir should be created");
        fs::write(source.join("one.txt"), b"one").expect("one fixture should be written");
        fs::write(source.join("nested/two.txt"), b"two").expect("two fixture should be written");

        copy_manifest_files(&source, &dest).expect("files should copy");

        assert_eq!(read(&dest.join("one.txt")), b"one");
        assert_eq!(read(&dest.join("nested/two.txt")), b"two");
        fs::remove_dir_all(source).expect("source should be removed");
        fs::remove_dir_all(dest).expect("dest should be removed");
    }

    #[test]
    fn require_local_file_rejects_missing_path() {
        let path = temp_dir("missing-file").join("missing.txt");
        assert!(require_local_file(&path).is_err());
        fs::remove_dir_all(path.parent().expect("path should have parent"))
            .expect("temp dir should be removed");
    }

    fn read(path: &Path) -> Vec<u8> {
        fs::read(path).expect("fixture should be readable")
    }
}
