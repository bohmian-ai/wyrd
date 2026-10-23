//! Wyrd API-key format: generation and parsing of principal-owned keys.

use secrecy::SecretString;
use uuid::Uuid;

/// Parsed API-key credential.
#[derive(Debug, Clone)]
pub struct WyrdApiKey {
    /// Tenant parsed from the key prefix.
    pub tenant_id: wyrd_spec::DataTenantId,
    /// Lookup prefix stored in Postgres.
    pub prefix: String,
    /// Full secret.
    pub secret: SecretString,
}

impl WyrdApiKey {
    /// Generate a plaintext API key for a tenant.
    #[must_use]
    pub fn generate(tenant_id: wyrd_spec::DataTenantId) -> Self {
        let random = Uuid::new_v4().simple().to_string();
        let tenant = tenant_id.as_uuid().simple().to_string();
        let prefix = format!("wyrd_sk_{tenant}_{}", &random[..8]);
        let secret = SecretString::from(format!("{prefix}_{}", Uuid::new_v4().simple()));
        Self {
            tenant_id,
            prefix,
            secret,
        }
    }

    /// Parse the tenant and lookup prefix from a raw key.
    ///
    /// # Errors
    /// Returns an error when the key does not follow Wyrd's prefix shape.
    pub fn parse(raw: &str) -> Result<Self, WyrdApiKeyParseError> {
        let mut parts = raw.split('_');
        if parts.next() != Some("wyrd") || parts.next() != Some("sk") {
            return Err(WyrdApiKeyParseError);
        }
        let tenant = parts.next().ok_or(WyrdApiKeyParseError)?;
        let visible = parts.next().ok_or(WyrdApiKeyParseError)?;
        if parts.next().is_none() {
            return Err(WyrdApiKeyParseError);
        }
        let tenant_uuid = Uuid::parse_str(tenant).map_err(|_| WyrdApiKeyParseError)?;
        let tenant_id =
            wyrd_spec::DataTenantId::try_from(tenant_uuid).map_err(|_| WyrdApiKeyParseError)?;
        Ok(Self {
            tenant_id,
            prefix: format!("wyrd_sk_{tenant}_{visible}"),
            secret: SecretString::from(raw.to_owned()),
        })
    }
}

/// API key parse error.
#[derive(Debug, thiserror::Error)]
#[error("invalid Wyrd API key")]
pub struct WyrdApiKeyParseError;

#[cfg(test)]
mod tests {
    use super::WyrdApiKey;
    use secrecy::ExposeSecret;

    #[test]
    fn generated_api_key_parses_tenant_and_prefix() {
        let tenant = wyrd_spec::DataTenantId::new_v7();
        let key = WyrdApiKey::generate(tenant);
        let parsed = WyrdApiKey::parse(key.secret.expose_secret()).expect("key parses");

        assert_eq!(parsed.tenant_id, tenant);
        assert_eq!(parsed.prefix, key.prefix);
    }

    /// Platform Service keys preserve the reserved `SYSTEM_OWNER` identity.
    #[test]
    fn generated_system_owner_api_key_parses_tenant_and_prefix() {
        let tenant = wyrd_spec::DataTenantId::SYSTEM_OWNER;
        let key = WyrdApiKey::generate(tenant);
        let parsed = WyrdApiKey::parse(key.secret.expose_secret()).expect("system key parses");

        assert_eq!(parsed.tenant_id, tenant);
        assert_eq!(parsed.prefix, key.prefix);
    }

    #[test]
    fn parse_rejects_wrong_prefix() {
        assert!(WyrdApiKey::parse("sk_12345").is_err());
    }

    #[test]
    fn parse_rejects_missing_segments() {
        assert!(WyrdApiKey::parse("wyrd_sk_onlythree").is_err());
    }

    #[test]
    fn parse_rejects_non_uuid_tenant() {
        assert!(WyrdApiKey::parse("wyrd_sk_notauuid_aaaa_bbbb").is_err());
    }
}
