//! Test certificate authority for the private Bifrost peer plane.
//!
//! Wyrd never issues certificates in production: a deployment provides the peer
//! CA and one dual-EKU leaf per replica. The harnesses need the same shape
//! without a static fixture, because a simulated pod must present its own
//! distinct leaf while every replica still verifies one shared DNS SAN. This
//! module mints exactly that: one in-memory CA plus as many leaves as a test
//! asks for.

use std::path::Path;

use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, SanType,
};

use crate::server::TestBifrostPeerTls;

/// Failure while minting or writing test peer certificate material.
#[derive(Debug, thiserror::Error)]
pub enum PeerCaError {
    /// `rcgen` rejected the requested certificate parameters or key.
    #[error("peer certificate generation failed: {0}")]
    Generate(String),
    /// A PEM file could not be written under the requested directory.
    #[error("peer certificate material could not be written: {0}")]
    Write(String),
}

/// One dual-EKU leaf identity issued to a single simulated replica.
///
/// The same leaf is presented as the private listener's server certificate and
/// as the client certificate on every outbound peer dial, which is exactly the
/// deployment contract: one process, one peer identity, both directions.
#[derive(Clone)]
pub struct BifrostPeerLeaf {
    /// PEM certificate chain for this replica.
    certificate_pem: String,
    /// PEM PKCS#8 private key paired with the certificate.
    private_key_pem: String,
}

impl std::fmt::Debug for BifrostPeerLeaf {
    /// Formats certificate size only; the private key never reaches logs.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BifrostPeerLeaf")
            .field("certificate_bytes", &self.certificate_pem.len())
            .field("private_key", &"[REDACTED]")
            .finish()
    }
}

impl BifrostPeerLeaf {
    /// Returns the PEM certificate chain presented by this replica.
    #[must_use]
    pub fn certificate_pem(&self) -> &str {
        &self.certificate_pem
    }

    /// Returns the PEM private key paired with this replica's certificate.
    #[must_use]
    pub fn private_key_pem(&self) -> &str {
        &self.private_key_pem
    }
}

/// In-memory certificate authority shared by every replica in one test topology.
///
/// A test holds exactly one of these. Membership in its trust domain is what
/// admits the private transport; it deliberately carries no runtime `NodeId`
/// and grants no operation authority, so a test can prove that a valid
/// certificate alone is not enough to call a peer RPC.
pub struct BifrostPeerCa {
    /// PEM certificate of the authority every replica trusts.
    ca_certificate_pem: String,
    /// Signing issuer retained so later replicas get leaves from the same CA.
    issuer: Issuer<'static, KeyPair>,
    /// Shared DNS SAN carried by every issued leaf.
    server_name: String,
}

impl std::fmt::Debug for BifrostPeerCa {
    /// Formats the shared identity only; the CA signing key never reaches logs.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BifrostPeerCa")
            .field("server_name", &self.server_name)
            .finish_non_exhaustive()
    }
}

impl BifrostPeerCa {
    /// Mints one authority whose leaves all carry `server_name` as their DNS SAN.
    ///
    /// Keeping the SAN constant across replicas mirrors deployment: membership
    /// addresses are per-pod, but TLS verifies one configured peer server name,
    /// so a per-pod address change never requires reissuing certificates.
    ///
    /// # Errors
    ///
    /// Returns [`PeerCaError::Generate`] when key generation or self-signing fails.
    pub fn generate(server_name: &str) -> Result<Self, PeerCaError> {
        let key = KeyPair::generate().map_err(|error| PeerCaError::Generate(error.to_string()))?;
        let mut params = CertificateParams::new(Vec::<String>::new())
            .map_err(|error| PeerCaError::Generate(error.to_string()))?;
        params
            .distinguished_name
            .push(DnType::CommonName, "Wyrd Bifrost Peer Test CA");
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let certificate = params
            .self_signed(&key)
            .map_err(|error| PeerCaError::Generate(error.to_string()))?;
        Ok(Self {
            ca_certificate_pem: certificate.pem(),
            issuer: Issuer::new(params, key),
            server_name: server_name.to_owned(),
        })
    }

    /// Returns the PEM trust root every replica and every dial verifies against.
    #[must_use]
    pub fn ca_certificate_pem(&self) -> &str {
        &self.ca_certificate_pem
    }

    /// Returns the DNS SAN carried by every leaf this authority issues.
    #[must_use]
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Issues one distinct leaf usable as both server and client identity.
    ///
    /// `common_name` distinguishes replicas in certificate inspection and audit
    /// fingerprints; it is deliberately not compared to any Wyrd `NodeId`.
    ///
    /// # Errors
    ///
    /// Returns [`PeerCaError::Generate`] when key generation or signing fails.
    pub fn issue_leaf(&self, common_name: &str) -> Result<BifrostPeerLeaf, PeerCaError> {
        let key = KeyPair::generate().map_err(|error| PeerCaError::Generate(error.to_string()))?;
        let mut params = CertificateParams::new(Vec::<String>::new())
            .map_err(|error| PeerCaError::Generate(error.to_string()))?;
        params
            .distinguished_name
            .push(DnType::CommonName, common_name);
        params.is_ca = IsCa::NoCa;
        params.subject_alt_names = vec![SanType::DnsName(
            self.server_name.clone().try_into().map_err(|_| {
                PeerCaError::Generate("peer server name is not a DNS name".to_owned())
            })?,
        )];
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ];
        let certificate = params
            .signed_by(&key, &self.issuer)
            .map_err(|error| PeerCaError::Generate(error.to_string()))?;
        Ok(BifrostPeerLeaf {
            certificate_pem: certificate.pem(),
            private_key_pem: key.serialize_pem(),
        })
    }

    /// Writes one replica's CA, certificate, and key under `directory`.
    ///
    /// `label` names the files so several replicas can share a directory during
    /// in-process tests; a child process is given its own private root instead.
    ///
    /// # Errors
    ///
    /// Returns [`PeerCaError::Generate`] when issuance fails or
    /// [`PeerCaError::Write`] when a PEM file cannot be written.
    pub fn materialize(
        &self,
        directory: &Path,
        label: &str,
    ) -> Result<TestBifrostPeerTls, PeerCaError> {
        let leaf = self.issue_leaf(label)?;
        let certificate_path = directory.join(format!("{label}-peer-cert.pem"));
        let private_key_path = directory.join(format!("{label}-peer-key.pem"));
        let ca_path = directory.join("bifrost-peer-ca.pem");
        let write = |path: &Path, contents: &str| -> Result<(), PeerCaError> {
            std::fs::write(path, contents).map_err(|error| PeerCaError::Write(error.to_string()))
        };
        write(&certificate_path, leaf.certificate_pem())?;
        write(&private_key_path, leaf.private_key_pem())?;
        write(&ca_path, &self.ca_certificate_pem)?;
        Ok(TestBifrostPeerTls {
            certificate_path,
            private_key_path,
            ca_path,
            server_name: self.server_name.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::BifrostPeerCa;

    /// Every issued leaf is distinct while sharing one authority and DNS SAN.
    ///
    /// # Panics
    ///
    /// Panics when generation fails or two replicas receive identical material.
    #[test]
    fn issued_leaves_are_distinct_under_one_authority() {
        let authority = BifrostPeerCa::generate("bifrost-peer.test").expect("authority generates");
        let first = authority.issue_leaf("oracle-0").expect("first leaf issues");
        let second = authority
            .issue_leaf("oracle-1")
            .expect("second leaf issues");

        assert_ne!(first.certificate_pem(), second.certificate_pem());
        assert_ne!(first.private_key_pem(), second.private_key_pem());
        assert!(authority.ca_certificate_pem().contains("BEGIN CERTIFICATE"));
        assert_eq!(authority.server_name(), "bifrost-peer.test");
    }

    /// Materializing writes a complete replica identity under one directory.
    ///
    /// # Panics
    ///
    /// Panics when generation or any PEM write fails.
    #[test]
    fn materialize_writes_a_complete_replica_identity() {
        let directory = tempfile::tempdir().expect("temporary root");
        let authority = BifrostPeerCa::generate("bifrost-peer.test").expect("authority generates");
        let tls = authority
            .materialize(directory.path(), "oracle-0")
            .expect("material writes");

        assert!(tls.certificate_path.is_file());
        assert!(tls.private_key_path.is_file());
        assert!(tls.ca_path.is_file());
        assert_eq!(tls.server_name, "bifrost-peer.test");
    }
}
