//! Verified, failure-safe installation of official server releases.
//!
//! Layout under the install root (`<wyrd config dir>/server` by default):
//!
//! ```text
//! versions/<version>/   one fully verified, extracted bundle per version
//! current -> versions/<version>   the active installation
//! ```
//!
//! A release is downloaded and extracted into a private staging directory,
//! renamed into `versions/` only after every check passes, and activated by
//! atomically replacing the `current` symlink. Any failure drops the staging
//! directory and leaves `current` untouched.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ed25519_dalek::pkcs8::DecodePublicKey as _;
use ed25519_dalek::{Signature, VerifyingKey};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use url::Url;

use crate::error::WyrdCliError;

/// Official release listing: the Wyrd repository's GitHub releases.
const OFFICIAL_RELEASES: &str =
    "https://api.github.com/repos/bohmian-ai/wyrd/releases?per_page=100";

/// Public half of the Ed25519 key the release workflow signs `checksums.txt`
/// with. Rotating the key requires a new CLI release.
const OFFICIAL_SIGNING_KEY: &str = include_str!("release-signing-key.pem");

/// Release asset listing every bundle's SHA-256 digest.
const CHECKSUMS: &str = "checksums.txt";

/// Raw 64-byte Ed25519 signature over [`CHECKSUMS`].
const CHECKSUMS_SIGNATURE: &str = "checksums.txt.sig";

/// Installs official server releases for one host target under one root.
///
/// Holds the release service, the trusted signing key, the install root,
/// and the client version that decides compatibility, so one value performs
/// every install and lookup with the same trust and location.
#[derive(Debug)]
pub struct ServerInstaller {
    /// HTTP client for the release listing and asset downloads.
    http: reqwest::Client,
    /// GitHub-shaped releases listing endpoint.
    releases: Url,
    /// Key every release's checksums must be signed by.
    signing_key: VerifyingKey,
    /// Directory holding `versions/` and the `current` link.
    root: PathBuf,
    /// Version of this CLI; only a release on its compatible line installs.
    cli_version: Version,
    /// Rust target triple whose bundle is installed.
    target: &'static str,
}

/// One verified installation on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledServer {
    /// Release version.
    pub version: Version,
    /// Extracted bundle directory holding `wyrd-server` and `ui/`.
    pub dir: PathBuf,
}

impl InstalledServer {
    /// Path of the installed `wyrd-server` executable.
    #[must_use]
    pub fn binary(&self) -> PathBuf {
        self.dir.join("wyrd-server")
    }
}

/// One entry of the GitHub releases listing.
#[derive(Debug, Deserialize)]
struct Release {
    /// Release tag, `v<semver>` for versioned releases.
    tag_name: String,
    /// Unpublished draft.
    draft: bool,
    /// Marked prerelease by its publisher.
    prerelease: bool,
    /// Published assets.
    assets: Vec<Asset>,
}

/// One published release asset.
#[derive(Debug, Deserialize)]
struct Asset {
    /// File name.
    name: String,
    /// Direct download URL.
    browser_download_url: Url,
}

impl Release {
    /// Download URL of the asset named `name`, if published.
    fn asset(&self, name: &str) -> Option<&Url> {
        self.assets
            .iter()
            .find(|asset| asset.name == name)
            .map(|asset| &asset.browser_download_url)
    }
}

impl ServerInstaller {
    /// The installer for official releases on this host.
    ///
    /// Uses the Wyrd GitHub releases, the embedded release signing key, this
    /// CLI's version, and `<wyrd config dir>/server` as the root.
    ///
    /// # Errors
    /// Returns [`WyrdCliError::ServerHostUnsupported`] for a host without an
    /// official bundle, and [`WyrdCliError::Io`] when the Wyrd config
    /// directory cannot be resolved or the HTTP client cannot be built.
    ///
    /// # Panics
    /// Panics only if the compiled-in release URL, signing key, or package
    /// version is malformed, which is a build defect.
    pub fn official() -> Result<Self, WyrdCliError> {
        let target = host_target()?;
        let root = wyrd_client::environment::Environment::Process
            .config_dir()
            .ok_or_else(|| WyrdCliError::Io {
                source: std::io::Error::other(
                    "cannot resolve the Wyrd config directory: set WYRD_CONFIG_HOME, XDG_CONFIG_HOME, or HOME",
                ),
            })?
            .join("server");
        Self::new(
            Url::parse(OFFICIAL_RELEASES).expect("the official releases URL is valid"),
            VerifyingKey::from_public_key_pem(OFFICIAL_SIGNING_KEY)
                .expect("the embedded release signing key is an Ed25519 public key PEM"),
            root,
            Version::parse(env!("CARGO_PKG_VERSION")).expect("the CLI package version is semver"),
            target,
        )
    }

    /// An installer for an explicit release service, key, root, client
    /// version, and target triple.
    ///
    /// # Errors
    /// Returns [`WyrdCliError::Io`] when TLS or the HTTP client cannot be
    /// initialized.
    pub fn new(
        releases: Url,
        signing_key: VerifyingKey,
        root: PathBuf,
        cli_version: Version,
        target: &'static str,
    ) -> Result<Self, WyrdCliError> {
        let io = |error: String| WyrdCliError::Io {
            source: std::io::Error::other(error),
        };
        wyrd_tls::install_crypto_provider().map_err(|error| io(error.to_string()))?;
        let http = reqwest::Client::builder()
            .user_agent(concat!("wyrd-cli/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| io(error.to_string()))?;
        Ok(Self {
            http,
            releases,
            signing_key,
            root,
            cli_version,
            target,
        })
    }

    /// The active installation, if one exists.
    ///
    /// Reads the `current` link; a missing link, unparsable version, or
    /// missing executable means no usable installation.
    #[must_use]
    pub fn installed(&self) -> Option<InstalledServer> {
        let link = std::fs::read_link(self.root.join("current")).ok()?;
        let version = Version::parse(link.file_name()?.to_str()?).ok()?;
        let installed = InstalledServer {
            dir: self.version_dir(&version),
            version,
        };
        installed.binary().is_file().then_some(installed)
    }

    /// Install the newest stable release and make it the active installation.
    ///
    /// Selects the highest stable, non-draft `v<semver>` release, refuses it
    /// unless it shares this CLI's compatible version line, then downloads,
    /// verifies, extracts, and activates it. An already-extracted version is
    /// activated without downloading again.
    ///
    /// # Errors
    /// - [`WyrdCliError::ServerReleaseUnavailable`]: listing or download
    ///   failed, no stable release exists, or it has no bundle for this host.
    /// - [`WyrdCliError::ServerVersionIncompatible`]: the newest release is
    ///   outside this CLI's compatible line.
    /// - [`WyrdCliError::ServerReleaseUnverified`]: the checksums are
    ///   missing or not signed by the release key, the bundle digest differs,
    ///   or the bundle does not extract to a runnable server.
    /// - [`WyrdCliError::Io`]: local filesystem failure.
    ///
    /// Every failure, including cancellation, leaves the previous `current`
    /// installation active and no runnable partial version.
    pub async fn install(&self) -> Result<InstalledServer, WyrdCliError> {
        let (version, release) = self.latest_release().await?;
        if !compatible(&self.cli_version, &version) {
            return Err(WyrdCliError::ServerVersionIncompatible {
                cli: self.cli_version.clone(),
                server: version,
            });
        }
        let dir = self.version_dir(&version);
        if !dir.join("wyrd-server").is_file() {
            let versions = self.root.join("versions");
            std::fs::create_dir_all(&versions).map_err(io)?;
            let staging = tempfile::Builder::new()
                .prefix(".staging-")
                .tempdir_in(&self.root)
                .map_err(io)?;
            let bundle = self.download_verified(&release, staging.path()).await?;
            let extracted = staging.path().join("server");
            extract(&bundle, &extracted)?;
            std::fs::rename(&extracted, &dir).map_err(io)?;
        }
        self.activate(&version)?;
        Ok(InstalledServer { version, dir })
    }

    /// Directory a verified `version` is extracted to.
    fn version_dir(&self, version: &Version) -> PathBuf {
        self.root.join("versions").join(version.to_string())
    }

    /// The highest stable, non-draft versioned release.
    ///
    /// # Errors
    /// Returns [`WyrdCliError::ServerReleaseUnavailable`] when the listing
    /// cannot be fetched or parsed, or holds no stable versioned release.
    // ponytail: first page of 100 releases only; paginate when Wyrd ships more.
    async fn latest_release(&self) -> Result<(Version, Release), WyrdCliError> {
        let releases: Vec<Release> = self
            .get(&self.releases)
            .await?
            .json()
            .await
            .map_err(|error| unavailable(format!("release listing is invalid: {error}")))?;
        releases
            .into_iter()
            .filter(|release| !release.draft && !release.prerelease)
            .filter_map(|release| {
                let version = Version::parse(release.tag_name.strip_prefix('v')?).ok()?;
                version.pre.is_empty().then_some((version, release))
            })
            .max_by(|(left, _), (right, _)| left.cmp(right))
            .ok_or_else(|| unavailable("no stable Wyrd release is published".to_owned()))
    }

    /// Download this host's bundle of `release` into `staging` and verify it.
    ///
    /// Verifies the checksums' Ed25519 signature against the trusted key
    /// before trusting any digest, then streams the bundle to disk while
    /// hashing it and compares the result to its signed digest.
    ///
    /// # Errors
    /// Returns [`WyrdCliError::ServerReleaseUnavailable`] when the release
    /// has no bundle for this host or a download fails, and
    /// [`WyrdCliError::ServerReleaseUnverified`] when the checksums or their
    /// signature are missing or invalid or the bundle digest differs.
    async fn download_verified(
        &self,
        release: &Release,
        staging: &Path,
    ) -> Result<PathBuf, WyrdCliError> {
        let name = format!("wyrd-server-{}.tar.gz", self.target);
        let bundle_url = release.asset(&name).ok_or_else(|| {
            unavailable(format!(
                "release {} has no bundle for {}",
                release.tag_name, self.target
            ))
        })?;
        let metadata = |asset| {
            release
                .asset(asset)
                .ok_or_else(|| unverified(format!("release {} has no {asset}", release.tag_name)))
        };
        let checksums = self.fetch(metadata(CHECKSUMS)?).await?;
        let signature = self.fetch(metadata(CHECKSUMS_SIGNATURE)?).await?;
        let signature = Signature::from_slice(&signature).map_err(|_| {
            unverified(format!("{CHECKSUMS_SIGNATURE} is not an Ed25519 signature"))
        })?;
        self.signing_key
            .verify_strict(&checksums, &signature)
            .map_err(|_| {
                unverified(format!("{CHECKSUMS} is not signed by the Wyrd release key"))
            })?;
        let expected = digest_for(&checksums, &name)?;

        let path = staging.join(&name);
        let mut response = self.get(bundle_url).await?;
        let mut file = std::fs::File::create(&path).map_err(io)?;
        let mut hasher = Sha256::new();
        // ponytail: blocking chunk writes on the CLI's own runtime; fine for one foreground download.
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| unavailable(format!("{name} download failed: {error}")))?
        {
            hasher.update(&chunk);
            file.write_all(&chunk).map_err(io)?;
        }
        file.sync_all().map_err(io)?;
        if format!("{:x}", hasher.finalize()) != expected {
            return Err(unverified(format!(
                "{name} does not match its signed digest"
            )));
        }
        Ok(path)
    }

    /// Point `current` at `version` by atomically replacing the link.
    ///
    /// # Errors
    /// Returns [`WyrdCliError::Io`] when the link cannot be written or
    /// renamed; the previous link is then unchanged.
    fn activate(&self, version: &Version) -> Result<(), WyrdCliError> {
        let next = self.root.join(".current.next");
        let _ = std::fs::remove_file(&next);
        std::os::unix::fs::symlink(Path::new("versions").join(version.to_string()), &next)
            .map_err(io)?;
        std::fs::rename(&next, self.root.join("current")).map_err(io)
    }

    /// Fetch a small release asset into memory.
    ///
    /// # Errors
    /// Returns [`WyrdCliError::ServerReleaseUnavailable`] when the request or
    /// body transfer fails.
    async fn fetch(&self, url: &Url) -> Result<Vec<u8>, WyrdCliError> {
        let body = self
            .get(url)
            .await?
            .bytes()
            .await
            .map_err(|error| unavailable(format!("{url} download failed: {error}")))?;
        Ok(body.to_vec())
    }

    /// Send a GET and require a success status.
    ///
    /// # Errors
    /// Returns [`WyrdCliError::ServerReleaseUnavailable`] on a transport
    /// failure or non-success status.
    async fn get(&self, url: &Url) -> Result<reqwest::Response, WyrdCliError> {
        self.http
            .get(url.clone())
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|error| unavailable(format!("{url}: {error}")))
    }
}

/// Whether a server `version` is on this CLI's compatible line.
///
/// Uses Cargo's caret rule symmetrically: the same major version, the same
/// minor while major is 0, and the same patch while both are 0.
fn compatible(cli: &Version, server: &Version) -> bool {
    cli.major == server.major
        && (cli.major != 0
            || (cli.minor == server.minor && (cli.minor != 0 || cli.patch == server.patch)))
}

/// The official bundle target triple for this host.
///
/// # Errors
/// Returns [`WyrdCliError::ServerHostUnsupported`] for any host other than
/// macOS or glibc Linux on x86_64 or aarch64.
fn host_target() -> Result<&'static str, WyrdCliError> {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    match (os, arch, cfg!(target_env = "musl")) {
        ("macos", "aarch64", _) => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64", _) => Ok("x86_64-apple-darwin"),
        ("linux", "x86_64", false) => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64", false) => Ok("aarch64-unknown-linux-gnu"),
        _ => Err(WyrdCliError::ServerHostUnsupported { os, arch }),
    }
}

/// The lowercase hex digest `checksums` records for `name`.
///
/// Accepts `shasum`/`sha256sum` lines: `<hex>  <name>` or `<hex> *<name>`.
///
/// # Errors
/// Returns [`WyrdCliError::ServerReleaseUnverified`] when no line names it.
fn digest_for(checksums: &[u8], name: &str) -> Result<String, WyrdCliError> {
    String::from_utf8_lossy(checksums)
        .lines()
        .filter_map(|line| line.split_once(char::is_whitespace))
        .find(|(_, file)| file.trim_start().trim_start_matches('*') == name)
        .map(|(digest, _)| digest.to_ascii_lowercase())
        .ok_or_else(|| unverified(format!("{CHECKSUMS} has no digest for {name}")))
}

/// Extract a verified bundle into `dest` with the system `tar`.
///
/// # Errors
/// Returns [`WyrdCliError::ServerReleaseUnverified`] when `tar` fails or
/// the result holds no `wyrd-server` file, and [`WyrdCliError::Io`] when
/// `dest` cannot be created or `tar` cannot start.
fn extract(bundle: &Path, dest: &Path) -> Result<(), WyrdCliError> {
    std::fs::create_dir_all(dest).map_err(io)?;
    let status = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(bundle)
        .arg("-C")
        .arg(dest)
        .status()
        .map_err(io)?;
    if !status.success() || !dest.join("wyrd-server").is_file() {
        return Err(unverified(
            "bundle did not extract to a runnable server".to_owned(),
        ));
    }
    Ok(())
}

/// A [`WyrdCliError::ServerReleaseUnavailable`] carrying `detail`.
fn unavailable(detail: String) -> WyrdCliError {
    WyrdCliError::ServerReleaseUnavailable { detail }
}

/// A [`WyrdCliError::ServerReleaseUnverified`] carrying `detail`.
fn unverified(detail: String) -> WyrdCliError {
    WyrdCliError::ServerReleaseUnverified { detail }
}

/// A [`WyrdCliError::Io`] wrapping `source`.
fn io(source: std::io::Error) -> WyrdCliError {
    WyrdCliError::Io { source }
}

/// Pure selection and parsing rules.
#[cfg(test)]
mod tests {
    use super::*;

    /// The caret rule is symmetric and tightens below 1.0.
    #[test]
    fn compatibility_follows_the_caret_line() {
        let v = |text| Version::parse(text).expect("version");
        assert!(compatible(&v("1.2.0"), &v("1.9.3")));
        assert!(!compatible(&v("1.2.0"), &v("2.0.0")));
        assert!(compatible(&v("0.2.1"), &v("0.2.3")));
        assert!(compatible(&v("0.2.3"), &v("0.2.1")));
        assert!(!compatible(&v("0.1.0"), &v("0.2.0")));
        assert!(!compatible(&v("0.0.1"), &v("0.0.2")));
    }

    /// Both checksum line styles resolve; an absent name is unverified.
    #[test]
    fn digest_lines_resolve_by_exact_name() {
        let sums = b"AB12  a.tar.gz\ncd34 *b.tar.gz\n";
        assert_eq!(digest_for(sums, "a.tar.gz").expect("a"), "ab12");
        assert_eq!(digest_for(sums, "b.tar.gz").expect("b"), "cd34");
        assert_eq!(
            digest_for(sums, "c.tar.gz").expect_err("c").code(),
            "WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED"
        );
    }

    /// The embedded key verifies a signature made the way the release
    /// workflow makes it (`openssl pkeyutl -sign -rawin` with the private
    /// key), and rejects the same signature over altered checksums.
    #[test]
    fn official_key_verifies_openssl_release_signatures() {
        let key = VerifyingKey::from_public_key_pem(OFFICIAL_SIGNING_KEY).expect("embedded key");
        let checksums = b"abc123  wyrd-server-x86_64-unknown-linux-gnu.tar.gz\n";
        let signature = Signature::from_bytes(&[
            0xfe, 0x64, 0xe6, 0xbb, 0x32, 0xa0, 0x3f, 0x1c, 0xa6, 0x87, 0x4c, 0x83, 0x5b, 0xfb,
            0x6c, 0x0f, 0x16, 0x7b, 0xd8, 0xa0, 0x95, 0x67, 0x9a, 0xc3, 0x09, 0xbc, 0x56, 0x6e,
            0x72, 0xac, 0x98, 0xda, 0x67, 0x86, 0x2c, 0x23, 0x26, 0x27, 0x25, 0x20, 0xc1, 0xc1,
            0x76, 0x3e, 0xf0, 0x0c, 0x99, 0x96, 0x5f, 0x61, 0xd0, 0x72, 0x38, 0x26, 0xb7, 0xa5,
            0xd1, 0x8d, 0xd3, 0x7e, 0xae, 0x60, 0x71, 0x04,
        ]);
        key.verify_strict(checksums, &signature)
            .expect("release signature verifies");
        assert!(
            key.verify_strict(
                b"abc124  wyrd-server-x86_64-unknown-linux-gnu.tar.gz\n",
                &signature
            )
            .is_err()
        );
    }
}
