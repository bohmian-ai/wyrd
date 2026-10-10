//! `wyrd server install` against a mock release service.
//!
//! A [`MockServer`] stands in for the GitHub releases API and its asset
//! downloads, and a fixed test key stands in for the release signing key, so
//! these prove release selection, signature and digest verification, and
//! failure-safe replacement without live GitHub access.

use std::path::Path;
use std::process::Command;

use ed25519_dalek::{Signer, SigningKey};
use semver::Version;
use sha2::{Digest, Sha256};
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_cli::server::ServerInstaller;

/// Host target every mock release publishes a bundle for.
const TARGET: &str = "x86_64-unknown-linux-gnu";

/// A change that breaks one mock release in a specific way.
type Corruption<'a> = dyn Fn(&mut MockRelease) + 'a;

/// Fixed release signing key for the mock release service.
fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

/// One mock release: its tag, flags, and published assets.
struct MockRelease {
    /// Release tag, such as `v0.2.0`.
    tag: &'static str,
    /// Whether GitHub marks the release a draft.
    draft: bool,
    /// Whether GitHub marks the release a prerelease.
    prerelease: bool,
    /// Asset names and bytes published on the release.
    assets: Vec<(String, Vec<u8>)>,
}

/// Build a gzipped server bundle whose `wyrd-server` prints `version`.
///
/// # Panics
/// Panics when the scratch directory or system `tar` fails.
fn bundle(version: &str) -> Vec<u8> {
    let work = tempfile::tempdir().expect("scratch directory");
    let content = work.path().join("content");
    std::fs::create_dir_all(content.join("ui/build")).expect("bundle layout");
    let binary = content.join("wyrd-server");
    std::fs::write(&binary, format!("#!/bin/sh\necho wyrd-server {version}\n")).expect("binary");
    std::fs::set_permissions(
        &binary,
        <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755),
    )
    .expect("binary mode");
    let archive = work.path().join("bundle.tar.gz");
    let status = Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&content)
        .arg(".")
        .status()
        .expect("tar runs");
    assert!(status.success(), "tar packs the bundle");
    std::fs::read(archive).expect("bundle bytes")
}

/// A correctly signed release of `version` for [`TARGET`].
fn signed_release(tag: &'static str) -> MockRelease {
    let name = format!("wyrd-server-{TARGET}.tar.gz");
    let bytes = bundle(tag.trim_start_matches('v'));
    let sums = format!("{:x}  {name}\n", Sha256::digest(&bytes)).into_bytes();
    let signature = signing_key().sign(&sums).to_bytes().to_vec();
    MockRelease {
        tag,
        draft: false,
        prerelease: false,
        assets: vec![
            (name, bytes),
            ("checksums.txt".to_owned(), sums),
            ("checksums.txt.sig".to_owned(), signature),
        ],
    }
}

/// Serve `releases` from `server` in GitHub's releases API shape.
async fn mount(server: &MockServer, releases: &[MockRelease]) {
    let mut listing = Vec::new();
    for release in releases {
        let mut assets = Vec::new();
        for (name, bytes) in &release.assets {
            let route = format!("/download/{}/{name}", release.tag);
            Mock::given(method("GET"))
                .and(path(route.clone()))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes.clone()))
                .mount(server)
                .await;
            assets.push(serde_json::json!({
                "name": name,
                "browser_download_url": format!("{}{route}", server.uri()),
            }));
        }
        listing.push(serde_json::json!({
            "tag_name": release.tag,
            "draft": release.draft,
            "prerelease": release.prerelease,
            "assets": assets,
        }));
    }
    Mock::given(method("GET"))
        .and(path("/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(listing))
        .mount(server)
        .await;
}

/// An installer for `cli_version` against `server`, installing under `root`.
///
/// # Panics
/// Panics when the installer cannot be constructed.
fn installer(server: &MockServer, root: &Path, cli_version: &str) -> ServerInstaller {
    ServerInstaller::new(
        Url::parse(&format!("{}/releases", server.uri())).expect("mock releases URL"),
        signing_key().verifying_key(),
        root.to_path_buf(),
        Version::parse(cli_version).expect("CLI version"),
        TARGET,
    )
    .expect("installer")
}

/// Run the active installation's `wyrd-server` and return what it printed.
///
/// # Panics
/// Panics when no installation is active or the binary does not run.
fn run_installed(installer: &ServerInstaller) -> String {
    let installed = installer.installed().expect("an active installation");
    let output = Command::new(installed.binary())
        .output()
        .expect("installed server runs");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// The newest stable, non-draft release by version is installed and run;
/// prereleases, drafts, and newer-by-date older versions are not selected,
/// and an incompatible newest release is refused before download.
///
/// # Panics
/// Panics when a different release is selected or the refusal differs.
#[tokio::test]
async fn server_install_selects_latest_stable() {
    let server = MockServer::start().await;
    let mut prerelease = signed_release("v0.3.0-rc.1");
    prerelease.prerelease = true;
    let mut draft = signed_release("v0.4.0");
    draft.draft = true;
    mount(
        &server,
        &[
            signed_release("v0.2.0"),
            prerelease,
            draft,
            signed_release("v0.2.3"),
            signed_release("v0.1.9"),
        ],
    )
    .await;
    let root = tempfile::tempdir().expect("install root");

    let compatible = installer(&server, root.path(), "0.2.1");
    let installed = compatible.install().await.expect("install succeeds");
    assert_eq!(installed.version, Version::new(0, 2, 3));
    assert_eq!(run_installed(&compatible), "wyrd-server 0.2.3");

    let incompatible = installer(&server, root.path(), "0.1.0");
    let refused = incompatible
        .install()
        .await
        .expect_err("an incompatible CLI refuses the newest release");
    assert_eq!(refused.code(), "WYRD_CLI_409_SERVER_VERSION_INCOMPATIBLE");
    assert!(refused.to_string().contains("0.2.3"), "{refused}");
    assert_eq!(run_installed(&incompatible), "wyrd-server 0.2.3");
}

/// Each failed update fails closed with a stable code and leaves the prior
/// verified installation active and runnable.
///
/// # Panics
/// Panics when a failure is accepted, its code differs, or the previous
/// installation stops running.
#[tokio::test]
async fn server_install_preserves_previous_on_failure() {
    let root = tempfile::tempdir().expect("install root");
    let first = MockServer::start().await;
    mount(&first, &[signed_release("v0.2.0")]).await;
    installer(&first, root.path(), "0.2.0")
        .install()
        .await
        .expect("initial install");

    let name = format!("wyrd-server-{TARGET}.tar.gz");
    let missing_target = |release: &mut MockRelease| release.assets.retain(|(n, _)| *n != name);
    let unsigned = |release: &mut MockRelease| {
        release.assets.retain(|(n, _)| n != "checksums.txt.sig");
    };
    let altered_sums = |release: &mut MockRelease| {
        let sums = &mut release.assets[1].1;
        sums[0] = if sums[0] == b'0' { b'1' } else { b'0' };
    };
    let altered_bytes = |release: &mut MockRelease| {
        let bytes = &mut release.assets[0].1;
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
    };
    let truncated = |release: &mut MockRelease| {
        let bytes = &mut release.assets[0].1;
        bytes.truncate(bytes.len() / 2);
    };
    let not_runnable = |release: &mut MockRelease| {
        let bytes = b"not a gzip archive".to_vec();
        let sums = format!("{:x}  {name}\n", Sha256::digest(&bytes)).into_bytes();
        release.assets[2].1 = signing_key().sign(&sums).to_bytes().to_vec();
        release.assets[0].1 = bytes;
        release.assets[1].1 = sums;
    };
    let cases: [(&str, &Corruption<'_>); 6] = [
        ("WYRD_CLI_503_SERVER_RELEASE_UNAVAILABLE", &missing_target),
        ("WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED", &unsigned),
        ("WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED", &altered_sums),
        ("WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED", &altered_bytes),
        ("WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED", &truncated),
        ("WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED", &not_runnable),
    ];
    for (code, corrupt) in cases {
        let server = MockServer::start().await;
        let mut release = signed_release("v0.2.1");
        corrupt(&mut release);
        mount(&server, &[release]).await;
        let update = installer(&server, root.path(), "0.2.0");
        let refused = update.install().await.expect_err("a bad update fails");
        assert_eq!(refused.code(), code, "{refused}");
        assert_eq!(run_installed(&update), "wyrd-server 0.2.0");
        assert!(
            !root.path().join("versions/0.2.1").exists(),
            "no partial installation"
        );
    }

    let gone = MockServer::start().await;
    let update = installer(&gone, root.path(), "0.2.0");
    let refused = update.install().await.expect_err("no releases listed");
    assert_eq!(refused.code(), "WYRD_CLI_503_SERVER_RELEASE_UNAVAILABLE");
    assert_eq!(run_installed(&update), "wyrd-server 0.2.0");
}
