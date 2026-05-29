use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use skald_prompt::media::{MAX_MEDIA_FILE_BYTES, document_path, image_path};
use skald_spec::{MediaKind, MediaSource, SkaldError};

fn temp_path(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("wyrd-{nonce}-{name}"))
}

#[test]
fn image_path_loads_and_encodes() {
    let path = temp_path("image.png");
    fs::write(&path, b"png bytes").expect("write fixture");
    let media = image_path(&path).expect("image loads");
    fs::remove_file(&path).expect("remove fixture");

    assert_eq!(media.kind, MediaKind::Image);
    assert!(matches!(
        media.source,
        MediaSource::Base64 {
            ref mime_type,
            ref data,
        } if mime_type == "image/png" && data == "cG5nIGJ5dGVz"
    ));
}

#[test]
fn document_path_loads_and_encodes() {
    let path = temp_path("doc.pdf");
    fs::write(&path, b"pdf bytes").expect("write fixture");
    let media = document_path(&path).expect("document loads");
    fs::remove_file(&path).expect("remove fixture");

    assert_eq!(media.kind, MediaKind::Document);
    assert!(matches!(
        media.source,
        MediaSource::Base64 {
            ref mime_type,
            ref data,
        } if mime_type == "application/pdf" && data == "cGRmIGJ5dGVz"
    ));
}

#[test]
fn image_path_rejects_directory_and_bad_extension() {
    let dir = temp_path("dir");
    fs::create_dir(&dir).expect("create dir");
    let err = image_path(&dir).expect_err("directory rejected");
    fs::remove_dir(&dir).expect("remove dir");
    assert!(matches!(err, SkaldError::MediaNotRegularFile { .. }));

    let path = temp_path("image.bin");
    fs::write(&path, b"bytes").expect("write fixture");
    let err = image_path(&path).expect_err("extension rejected");
    fs::remove_file(&path).expect("remove fixture");
    assert!(matches!(
        err,
        SkaldError::MediaInvalidExtension {
            kind: MediaKind::Image,
            ..
        }
    ));
}

#[test]
fn image_path_rejects_oversize() {
    let path = temp_path("large.png");
    let file = fs::File::create(&path).expect("create fixture");
    file.set_len(MAX_MEDIA_FILE_BYTES + 1)
        .expect("resize fixture");
    let err = image_path(&path).expect_err("oversize rejected");
    fs::remove_file(&path).expect("remove fixture");

    assert!(matches!(
        err,
        SkaldError::MediaTooLarge {
            size,
            limit: MAX_MEDIA_FILE_BYTES,
            ..
        } if size == MAX_MEDIA_FILE_BYTES + 1
    ));
}
