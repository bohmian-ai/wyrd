use wyrd_version::{VersionBump, WyrdVersion, WyrdVersionError};

#[test]
fn parses_basic_semver() {
    let v = WyrdVersion::parse("1.2.3").expect("parse");
    assert_eq!(v.major(), 1);
    assert_eq!(v.minor(), 2);
    assert_eq!(v.patch(), 3);
    assert_eq!(v.to_string(), "1.2.3");
}

#[test]
fn rejects_empty_string() {
    assert!(matches!(
        WyrdVersion::parse(""),
        Err(WyrdVersionError::EmptyVersion)
    ));
}

#[test]
fn rejects_nonsense() {
    assert!(matches!(
        WyrdVersion::parse("not-a-version"),
        Err(WyrdVersionError::InvalidVersion(_))
    ));
}

#[test]
fn bump_major() {
    let v = WyrdVersion::parse("1.2.3").unwrap();
    let n = v.bump(VersionBump::Major).unwrap();
    assert_eq!(n.to_string(), "2.0.0");
}

#[test]
fn bump_minor_clears_patch_and_pre() {
    let v = WyrdVersion::parse("1.2.3-alpha.1").unwrap();
    let n = v.bump(VersionBump::Minor).unwrap();
    assert_eq!(n.to_string(), "1.3.0");
}

#[test]
fn bump_patch() {
    let v = WyrdVersion::parse("1.2.3").unwrap();
    let n = v.bump(VersionBump::Patch).unwrap();
    assert_eq!(n.to_string(), "1.2.4");
}

#[test]
fn set_prerelease() {
    let v = WyrdVersion::parse("1.2.3").unwrap();
    let n = v.bump(VersionBump::Pre("rc.1")).unwrap();
    assert_eq!(n.to_string(), "1.2.3-rc.1");
}

#[test]
fn set_build() {
    let v = WyrdVersion::parse("1.2.3").unwrap();
    let n = v.bump(VersionBump::Build("abc123")).unwrap();
    assert_eq!(n.to_string(), "1.2.3+abc123");
}

#[test]
fn invalid_prerelease() {
    let v = WyrdVersion::parse("1.2.3").unwrap();
    let err = v.bump(VersionBump::Pre("not valid")).unwrap_err();
    assert!(matches!(err, WyrdVersionError::InvalidPrerelease(_)));
}

#[test]
fn round_trip_serde() {
    let v = WyrdVersion::parse("1.2.3-alpha.1+build.42").unwrap();
    let s = serde_json::to_string(&v).unwrap();
    assert_eq!(s, "\"1.2.3-alpha.1+build.42\"");
    let back: WyrdVersion = serde_json::from_str(&s).unwrap();
    assert_eq!(v, back);
}

#[test]
fn current_returns_valid_version() {
    let v = WyrdVersion::current();
    assert!(v.major() > 0 || v.minor() > 0 || v.patch() > 0);
}
