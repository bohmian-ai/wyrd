//! Trusted issuer resolution for server-tier auth flows.

use wyrd_auth_oidc::{IssuerConfigResolver, TrustedIssuer};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::error::WyrdError;

/// Resolve the tenant's trusted issuer matching `issuer`, failing closed.
///
/// This is the shared decision used by the human login and OIDC callback paths.
/// The workload jwt-bearer path verifies externally and resolves workload
/// bindings directly.
pub async fn trusted_issuer<R>(
    resolver: Option<&R>,
    tenant_id: DataTenantId,
    issuer: &IssuerUrl,
) -> Result<TrustedIssuer, WyrdError>
where
    R: IssuerConfigResolver,
{
    let resolver = resolver.ok_or_else(|| WyrdError::Internal {
        message: "trusted issuer resolver is not configured".to_owned(),
        details: serde_json::json!({}),
    })?;
    let issuers = resolver
        .trusted_issuers(&tenant_id)
        .await
        .map_err(|error| {
            tracing::warn!(
                error = %error,
                tenant_id = %tenant_id,
                "trusted issuer resolution failed"
            );
            WyrdError::AuthVerifyUnavailable {
                message: "trusted issuer resolution unavailable".to_owned(),
                details: serde_json::json!({ "retry_after_seconds": 1 }),
            }
        })?;
    issuers
        .into_iter()
        .find(|candidate| candidate.issuer == *issuer)
        .ok_or_else(|| WyrdError::InvalidToken {
            message: "issuer is not trusted for the resolved tenant".to_owned(),
            details: serde_json::json!({}),
        })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::future::Future;
    use std::time::Duration;

    use wyrd_auth_oidc::error::OidcError;
    use wyrd_auth_oidc::{
        ClaimMapping, ClaimPath, ClientAuth, IssuerConfigResolver, TrustedIssuer,
    };
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::IssuerTokenPolicy;
    use wyrd_spec::auth::IssuerUrl;
    use wyrd_spec::error::WyrdError;

    use super::trusted_issuer;

    #[derive(Debug)]
    struct StubResolver {
        issuers: Vec<TrustedIssuer>,
        fail: bool,
    }

    impl IssuerConfigResolver for StubResolver {
        fn trusted_issuers(
            &self,
            _tenant: &DataTenantId,
        ) -> impl Future<Output = Result<Vec<TrustedIssuer>, OidcError>> + Send {
            let result = if self.fail {
                Err(OidcError::Discovery {
                    issuer: "stub".to_owned(),
                    message: "resolver unavailable".to_owned(),
                })
            } else {
                Ok(self.issuers.clone())
            };
            std::future::ready(result)
        }
    }

    fn stub_issuer(url: &str) -> TrustedIssuer {
        TrustedIssuer {
            tenant_id: DataTenantId::new_v7(),
            issuer: IssuerUrl::new(url.to_owned()).expect("valid issuer url"),
            jwks_uri: url::Url::parse("https://example.com/.well-known/jwks.json")
                .expect("valid url"),
            expected_audience: String::new(),
            client_id: String::new(),
            client_auth: ClientAuth::Public,
            claim_mapping: ClaimMapping {
                subject: ClaimPath::new("sub"),
                email: None,
                groups: None,
            },
            group_role_map: HashMap::new(),
            default_roles: vec![],
            principal_kind: IssuerTokenPolicy::Human,
            jwks_ttl: Duration::from_hours(1),
        }
    }

    #[tokio::test]
    async fn returns_matching_issuer() {
        let tenant = DataTenantId::new_v7();
        let issuer_url = IssuerUrl::new("https://idp.example.com".to_owned()).expect("valid url");
        let resolver = StubResolver {
            issuers: vec![stub_issuer("https://idp.example.com")],
            fail: false,
        };

        let result = trusted_issuer(Some(&resolver), tenant, &issuer_url).await;
        assert!(result.is_ok(), "matching issuer should be returned");
    }

    #[tokio::test]
    async fn returns_invalid_token_when_issuer_not_in_list() {
        let tenant = DataTenantId::new_v7();
        let issuer_url = IssuerUrl::new("https://other.example.com".to_owned()).expect("valid url");
        let resolver = StubResolver {
            issuers: vec![stub_issuer("https://idp.example.com")],
            fail: false,
        };

        let result = trusted_issuer(Some(&resolver), tenant, &issuer_url).await;
        assert!(
            matches!(result, Err(WyrdError::InvalidToken { .. })),
            "non-matching issuer should return InvalidToken, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn returns_unavailable_when_resolver_fails() {
        let tenant = DataTenantId::new_v7();
        let issuer_url = IssuerUrl::new("https://idp.example.com".to_owned()).expect("valid url");
        let resolver = StubResolver {
            issuers: vec![],
            fail: true,
        };

        let result = trusted_issuer(Some(&resolver), tenant, &issuer_url).await;
        assert!(
            matches!(result, Err(WyrdError::AuthVerifyUnavailable { .. })),
            "failed resolver should return AuthVerifyUnavailable, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn returns_internal_error_when_resolver_is_none() {
        let tenant = DataTenantId::new_v7();
        let issuer_url = IssuerUrl::new("https://idp.example.com".to_owned()).expect("valid url");

        let result = trusted_issuer(None::<&StubResolver>, tenant, &issuer_url).await;
        assert!(
            matches!(result, Err(WyrdError::Internal { .. })),
            "missing resolver should return Internal, got: {result:?}"
        );
    }
}
