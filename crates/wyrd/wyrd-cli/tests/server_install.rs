//! `wyrd server install` against a mock release service.
//!
//! A [`MockServer`] stands in for the GitHub releases API and its asset
//! downloads, and a fixed test key stands in for the release signing key, so
//! these prove release selection, signature and digest verification, and
//! failure-safe replacement without live GitHub access.

use std::os::unix::fs::PermissionsExt as _;
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

/// Production UI files `scripts/build.sh bundle` stages from the pinned
/// adapter-node build, as `(path, contents)` under the bundle root: the
/// module package declaring its external production dependencies, those
/// dependencies' installed packages, the generated entry and support
/// modules, the generated server chunks they import, and the client
/// version manifest.
const UI_PAYLOAD: [(&str, &str); 10] = [
    (
        "ui/package.json",
        r#"{"name":"wyrd-ui","private":true,"type":"module","dependencies":{"@fontsource/fragment-mono":"5.3.0","jose":"6.2.12"}}"#,
    ),
    (
        "ui/node_modules/@fontsource/fragment-mono/package.json",
        r#"{"name":"@fontsource/fragment-mono","version":"5.3.0"}"#,
    ),
    (
        "ui/node_modules/jose/package.json",
        r#"{"name":"jose","version":"6.2.12","type":"module"}"#,
    ),
    (
        "ui/build/index.js",
        "import { h as handler } from './server/chunks/handler-B.js';\nimport { env } from './env.js';\nimport './shims.js';\n",
    ),
    (
        "ui/build/handler.js",
        "import './shims.js';\nexport { h as handler } from './server/chunks/handler-B.js';\n",
    ),
    (
        "ui/build/env.js",
        "export const env = (name) => process.env[name];\n",
    ),
    ("ui/build/shims.js", "import 'node:buffer';\n"),
    (
        "ui/build/server/chunks/handler-B.js",
        "export const h = () => {};\n",
    ),
    (
        "ui/build/server/chunks/manifest.js-C.js",
        "export const manifest = {};\n",
    ),
    ("ui/build/client/_app/version.json", r#"{"version":"1"}"#),
];

/// Build a gzipped server bundle whose `wyrd-server` prints `version` and
/// whose `ui/` holds the complete production payload [`UI_PAYLOAD`], after
/// `edit` alters the staged content root.
///
/// # Panics
/// Panics when the scratch directory or system `tar` fails.
fn bundle_with(version: &str, edit: impl Fn(&Path)) -> Vec<u8> {
    let work = tempfile::tempdir().expect("scratch directory");
    let content = work.path().join("content");
    for (file, contents) in UI_PAYLOAD {
        let file = content.join(file);
        std::fs::create_dir_all(file.parent().expect("payload file parent")).expect("UI layout");
        std::fs::write(file, contents).expect("UI payload file");
    }
    edit(&content);
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

/// A complete bundle of `version`.
///
/// # Panics
/// Panics when [`bundle_with`] does.
fn bundle(version: &str) -> Vec<u8> {
    bundle_with(version, |_| {})
}

/// Remove the bundle-relative file or directory `file` under `content`.
///
/// # Panics
/// Panics when it does not exist or cannot be removed.
fn remove(content: &Path, file: &str) {
    let path = content.join(file);
    if path.is_dir() {
        std::fs::remove_dir_all(path).expect("remove payload directory");
    } else {
        std::fs::remove_file(path).expect("remove payload file");
    }
}

/// Extract a complete bundle of `version` straight into `versions/<version>`
/// under `root`, as an earlier install would have, after `edit` alters it.
///
/// # Panics
/// Panics when the directory cannot be created or `tar` fails.
fn cache(root: &Path, version: &str, edit: impl Fn(&Path)) {
    let dir = root.join("versions").join(version);
    std::fs::create_dir_all(&dir).expect("cached version directory");
    let archive = root.join(format!(".cache-{version}.tar.gz"));
    std::fs::write(&archive, bundle(version)).expect("cached bundle");
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&dir)
        .status()
        .expect("tar runs");
    assert!(status.success(), "tar extracts the cached bundle");
    std::fs::remove_file(archive).expect("remove cached bundle");
    edit(&dir);
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
/// Panics when no complete installation is active or the binary does not run.
fn run_installed(installer: &ServerInstaller) -> String {
    let installed = installer
        .installed()
        .expect("a complete active installation")
        .expect("an active installation");
    let output = Command::new(installed.binary())
        .output()
        .expect("installed server runs");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// The newest stable, non-draft release by version is installed and run;
/// prereleases, drafts, and newer-by-date older versions are not selected,
/// and an incompatible newest release is refused before download. The
/// install root lies in a config directory that does not exist yet, which
/// installation creates private to the user whatever the umask, so a saved
/// login can later be stored there. A complete cached version activates
/// without downloading its bundle again.
///
/// # Panics
/// Panics when a different release is selected, the refusal differs, or the
/// created config directory is accessible to other users.
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
    let home = tempfile::tempdir().expect("config home");
    let root = home.path().join("wyrd/server");

    let compatible = installer(&server, &root, "0.2.1");
    let installed = compatible.install().await.expect("install succeeds");
    assert_eq!(installed.version, Version::new(0, 2, 3));
    assert_eq!(run_installed(&compatible), "wyrd-server 0.2.3");
    let config_mode = std::fs::metadata(home.path().join("wyrd"))
        .expect("config directory created")
        .permissions()
        .mode();
    assert_eq!(
        config_mode & 0o077,
        0,
        "config directory mode {config_mode:o}"
    );

    let incompatible = installer(&server, &root, "0.1.0");
    let refused = incompatible
        .install()
        .await
        .expect_err("an incompatible CLI refuses the newest release");
    assert_eq!(refused.code(), "WYRD_CLI_409_SERVER_VERSION_INCOMPATIBLE");
    assert!(refused.to_string().contains("0.2.3"), "{refused}");
    assert_eq!(run_installed(&incompatible), "wyrd-server 0.2.3");

    let offline = MockServer::start().await;
    let mut listed = signed_release("v0.2.4");
    listed.assets.retain(|(name, _)| !name.ends_with(".tar.gz"));
    mount(&offline, &[listed]).await;
    cache(&root, "0.2.4", |_| {});
    let reuse = installer(&offline, &root, "0.2.1");
    let reused = reuse
        .install()
        .await
        .expect("a complete cached version activates without a download");
    assert_eq!(reused.version, Version::new(0, 2, 4));
    assert_eq!(run_installed(&reuse), "wyrd-server 0.2.4");
}

/// Assert `refused` is the incomplete-installation refusal naming
/// `versions/<version>` under `root` and telling the user to remove it.
///
/// # Panics
/// Panics when the code, named directory, or recovery differs.
fn assert_incomplete(refused: &wyrd_cli::error::WyrdCliError, root: &Path, version: &str) {
    assert_eq!(
        refused.code(),
        "WYRD_CLI_409_SERVER_INSTALL_INCOMPLETE",
        "{refused}"
    );
    let message = refused.to_string();
    let dir = root.join("versions").join(version);
    assert!(message.contains(&dir.display().to_string()), "{message}");
    assert!(message.contains("remove"), "{message}");
}

/// Each failed update fails closed with a stable code and leaves the prior
/// verified installation active and runnable, including correctly signed
/// bundles whose production UI is absent, empty markers, or missing its
/// generated support modules or a production dependency. Incomplete cached
/// versions are refused before activation and kept, an incomplete active
/// version is no installation, and each refusal's recovery (removing the
/// named version, then installing) succeeds.
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
    let resign = |release: &mut MockRelease, bytes: Vec<u8>| {
        let sums = format!("{:x}  {name}\n", Sha256::digest(&bytes)).into_bytes();
        release.assets[2].1 = signing_key().sign(&sums).to_bytes().to_vec();
        release.assets[0].1 = bytes;
        release.assets[1].1 = sums;
    };
    let not_runnable = |release: &mut MockRelease| resign(release, b"not a gzip archive".to_vec());
    let signed_payload = |release: &mut MockRelease, edit: &dyn Fn(&Path)| {
        resign(release, bundle_with("0.2.1", edit));
    };
    let no_ui =
        |release: &mut MockRelease| signed_payload(release, &|content| remove(content, "ui"));
    let empty_markers = |release: &mut MockRelease| {
        signed_payload(release, &|content| {
            remove(content, "ui");
            std::fs::create_dir_all(content.join("ui/build")).expect("marker layout");
            std::fs::create_dir_all(content.join("ui/node_modules")).expect("marker layout");
            std::fs::write(content.join("ui/package.json"), "{}").expect("marker package");
            std::fs::write(content.join("ui/build/index.js"), "").expect("marker entry");
        });
    };
    let no_server_chunks = |release: &mut MockRelease| {
        signed_payload(release, &|content| remove(content, "ui/build/server"));
    };
    let no_handler = |release: &mut MockRelease| {
        signed_payload(release, &|content| remove(content, "ui/build/handler.js"));
    };
    let no_dependency = |release: &mut MockRelease| {
        signed_payload(release, &|content| remove(content, "ui/node_modules/jose"));
    };
    let cases: [(&str, &Corruption<'_>); 11] = [
        ("WYRD_CLI_503_SERVER_RELEASE_UNAVAILABLE", &missing_target),
        ("WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED", &unsigned),
        ("WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED", &altered_sums),
        ("WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED", &altered_bytes),
        ("WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED", &truncated),
        ("WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED", &not_runnable),
        ("WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED", &no_ui),
        ("WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED", &empty_markers),
        ("WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED", &no_server_chunks),
        ("WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED", &no_handler),
        ("WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED", &no_dependency),
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

    let newer = MockServer::start().await;
    mount(&newer, &[signed_release("v0.2.1")]).await;
    let update = installer(&newer, root.path(), "0.2.0");
    let cached = root.path().join("versions/0.2.1");
    let incomplete_caches: [&dyn Fn(&Path); 2] = [&|dir| remove(dir, "ui"), &|dir| {
        remove(dir, "ui/node_modules");
    }];
    for edit in incomplete_caches {
        cache(root.path(), "0.2.1", edit);
        let refused = update
            .install()
            .await
            .expect_err("an incomplete cached version is not activated");
        assert_incomplete(&refused, root.path(), "0.2.1");
        assert_eq!(run_installed(&update), "wyrd-server 0.2.0");
        assert!(
            cached.join("wyrd-server").is_file(),
            "the refused cache is kept"
        );
        std::fs::remove_dir_all(&cached).expect("follow the recovery");
    }
    update.install().await.expect("recovered install");
    assert_eq!(run_installed(&update), "wyrd-server 0.2.1");

    remove(&cached, "ui/build/server");
    let refused = update
        .installed()
        .expect_err("an incomplete active version is not an installation");
    assert_incomplete(&refused, root.path(), "0.2.1");
    let refused = update
        .install()
        .await
        .expect_err("reinstall does not reactivate the incomplete version");
    assert_incomplete(&refused, root.path(), "0.2.1");
    std::fs::remove_dir_all(&cached).expect("follow the recovery");
    update.install().await.expect("recovered install");
    assert_eq!(run_installed(&update), "wyrd-server 0.2.1");
}
