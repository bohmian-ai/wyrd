//! Workspace discovery: a visitor proves an address by email and reads the
//! live workspaces whose User verified it.
//!
//! The request stores nothing: it seals the proof digest, canonical address
//! and a browser binding into the browser's cookie, and answers alike for
//! every address. Verification spends the proof once across replicas and
//! seals a five-minute read of the directory into the same cookie. Every
//! disclosed workspace is revalidated through its tenant's own connection:
//! the User is active and its current contact is the exact address and
//! revision the directory projected, so a stale entry reveals nothing.
//!
//! The directory follows contact changes through the `wyrd_contact`
//! notification: [`SocialSignIn::reconcile_contact`] projects one User and
//! [`SocialSignIn::reconcile_directory`] resynchronizes every live tenant
//! whenever the listener (re)subscribes.

use secrecy::SecretString;
use serde_json::json;
use url::Url;
use uuid::Uuid;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{DiscoveredWorkspace, DiscoveryPage, SecretBearer, Sha256Hex};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;
use wyrd_sql::WyrdPostgres;
use wyrd_sql::queries::auth::{VerifiedContact, live_contact, tenant_contacts};
use wyrd_sql::queries::platform::discovery::{
    directory_candidates, project_contact, projected_users, spend_discovery_proof,
    unproject_contact,
};
use wyrd_sql::queries::platform::tenants::list_active_tenant_ids;

use super::{SignupEnvelope, SocialSignIn};
use crate::browser_sessions::random_secret;
use crate::error::store_error;
use crate::mail::{ApplicationMail, EmailAddress};
use crate::passwords::code_hash;

/// How long a discovery proof stays usable.
const DISCOVERY_EMAIL_TTL: std::time::Duration = std::time::Duration::from_mins(15);

/// How long a spent proof's directory read lasts.
const DISCOVERY_READ_TTL: std::time::Duration = std::time::Duration::from_mins(5);

/// Workspaces on one discovery page.
const DISCOVERY_PAGE: usize = 20;

/// Directory entries read for one page; several Users of one tenant, or
/// entries revalidation drops, share this bound.
const DISCOVERY_CANDIDATES: i64 = 80;

/// The pseudonym scope of directory address keys.
const DIRECTORY_SCOPE: &str = "directory";

impl SocialSignIn {
    /// Mail a discovery proof to `typed`, asked for from `source`, and
    /// return the sealed flow cookie the same browser must present to read
    /// its workspaces.
    ///
    /// The answer is the same whether or not any workspace knows the
    /// address, and whether or not the address's email window let the
    /// message go. The request is admitted in its source's window first. The
    /// message links `/workspaces/verify` on `origin` with the proof in its
    /// fragment and names no workspace.
    ///
    /// # Errors
    /// Returns [`WyrdError::MethodUnavailable`] without a mail relay,
    /// [`WyrdError::Validation`] for an unparsable address,
    /// [`WyrdError::EmailAttempts`] past the source window, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the store, key or relay
    /// fails.
    pub async fn begin_discovery(
        &self,
        mail: &ApplicationMail,
        source: Option<std::net::IpAddr>,
        typed: &str,
        origin: &Url,
    ) -> Result<SecretString, WyrdError> {
        if mail.settings().await?.relay.is_none() {
            return Err(WyrdError::MethodUnavailable {
                message: "workspace discovery is not available on this deployment".to_owned(),
                details: json!({ "reason": "method_unavailable" }),
            });
        }
        let address = EmailAddress::parse(typed, "email")?;
        let send = self
            .admit_email("discovery", source, &address, None)
            .await?;
        let proof = random_secret();
        let cookie = self
            .seal_envelope(&SignupEnvelope::DiscoveryEmail {
                proof_hash: Sha256Hex::digest(proof.as_bytes()),
                address: address.canonical.clone(),
                binding: random_secret(),
                expires_at: self.envelope_deadline(DISCOVERY_EMAIL_TTL).await?,
            })
            .await?;
        let mut link = origin
            .join("workspaces/verify")
            .map_err(|_| super::invalid_state("the deployment origin is invalid"))?;
        link.set_fragment(Some(&format!("proof={proof}")));
        if send {
            mail.send_discovery_proof(&address, link.as_str(), &proof)
                .await?;
        }
        Ok(cookie)
    }

    /// Spend `proof` for the browser presenting the flow `cookie`, and
    /// return the sealed read cookie with the first page of workspaces.
    ///
    /// The same browser repeating a spent proof, as after a lost response,
    /// gets the read it was first granted, until that read's deadline; any
    /// other repeat is refused.
    ///
    /// # Errors
    /// Returns [`WyrdError::OneTimeCodeInvalid`] for a missing or foreign
    /// flow, a wrong proof or a spent one, [`WyrdError::InvalidState`] once
    /// expired, and [`WyrdError::AuthVerifyUnavailable`] when the store or
    /// key fails.
    pub async fn verify_discovery(
        &self,
        postgres: &WyrdPostgres,
        cookie: Option<&str>,
        proof: &SecretBearer,
        origin: &Url,
    ) -> Result<(SecretString, DiscoveryPage), WyrdError> {
        let refused = || WyrdError::OneTimeCodeInvalid {
            message: "this email link is not valid in this browser; ask for a new one".to_owned(),
            details: json!({}),
        };
        let opened = match cookie {
            Some(cookie) => self.open_envelope(cookie).await?,
            None => None,
        };
        let Some(SignupEnvelope::DiscoveryEmail {
            proof_hash,
            address,
            binding,
            expires_at,
        }) = opened
        else {
            return Err(refused());
        };
        if code_hash(proof).ok() != Some(proof_hash) {
            return Err(refused());
        }
        self.require_live(expires_at).await?;
        let read_until = spend_discovery_proof(
            &self.operator,
            proof_hash.as_bytes(),
            Sha256Hex::digest(binding.as_bytes()).as_bytes(),
            DISCOVERY_READ_TTL,
            DISCOVERY_EMAIL_TTL,
        )
        .await
        .map_err(store_error)?
        .ok_or_else(refused)?;
        let page = self
            .discovery_page(postgres, &address, None, origin)
            .await?;
        let read = self
            .seal_envelope(&SignupEnvelope::DiscoveryRead {
                address,
                expires_at: read_until,
            })
            .await?;
        Ok((read, page))
    }

    /// The page after `cursor` of the discovery the read `cookie` holds.
    ///
    /// # Errors
    /// Returns [`WyrdError::OneTimeCodeInvalid`] without a read cookie,
    /// [`WyrdError::InvalidState`] once it expired, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the store or key fails.
    pub async fn discovery_next(
        &self,
        postgres: &WyrdPostgres,
        cookie: Option<&str>,
        cursor: &str,
        origin: &Url,
    ) -> Result<DiscoveryPage, WyrdError> {
        let opened = match cookie {
            Some(cookie) => self.open_envelope(cookie).await?,
            None => None,
        };
        let Some(SignupEnvelope::DiscoveryRead {
            address,
            expires_at,
        }) = opened
        else {
            return Err(WyrdError::OneTimeCodeInvalid {
                message: "no workspace discovery in this browser; ask for a new email".to_owned(),
                details: json!({}),
            });
        };
        self.require_live(expires_at).await?;
        self.discovery_page(postgres, &address, Some(cursor), origin)
            .await
    }

    /// One page of the live workspaces whose active User's current contact
    /// is `address`, after the route key `after`.
    ///
    /// A candidate whose validation cannot complete is omitted, never shown.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the key or the
    /// directory read fails.
    async fn discovery_page(
        &self,
        postgres: &WyrdPostgres,
        address: &str,
        after: Option<&str>,
        origin: &Url,
    ) -> Result<DiscoveryPage, WyrdError> {
        let key = self.pseudonym(DIRECTORY_SCOPE, address.as_bytes()).await?;
        let candidates = directory_candidates(&self.operator, &key, after, DISCOVERY_CANDIDATES)
            .await
            .map_err(store_error)?;
        let exhausted = candidates.len() < usize::try_from(DISCOVERY_CANDIDATES).unwrap_or(0);
        let mut workspaces: Vec<DiscoveredWorkspace> = Vec::new();
        let mut last = None;
        for candidate in candidates {
            if workspaces.len() == DISCOVERY_PAGE
                && last.as_deref() != Some(candidate.slug.as_str())
            {
                return Ok(DiscoveryPage {
                    workspaces,
                    next_cursor: last,
                });
            }
            last = Some(candidate.slug.clone());
            if workspaces
                .last()
                .is_some_and(|shown| shown.tenant.as_str() == candidate.slug)
                || !current_contact(postgres, &candidate, address).await
            {
                continue;
            }
            let Ok(tenant) = candidate.slug.parse::<TenantSlug>() else {
                continue;
            };
            let login_url = origin
                .join(&format!("t/{tenant}/login"))
                .map_err(|_| super::invalid_state("the deployment origin is invalid"))?;
            workspaces.push(DiscoveredWorkspace {
                tenant,
                display_name: candidate.display_name,
                login_url: login_url.to_string(),
            });
        }
        Ok(DiscoveryPage {
            workspaces,
            next_cursor: if exhausted { None } else { last },
        })
    }

    /// Bring `payload`'s directory entry, `<data_tenant_id>:<user_id>`, to
    /// that User's current contact through its tenant connection.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when a read, the key or
    /// a write fails; a malformed payload changes nothing.
    pub async fn reconcile_contact(
        &self,
        postgres: &WyrdPostgres,
        payload: &str,
    ) -> Result<(), WyrdError> {
        let Some((tenant, user)) = payload.split_once(':') else {
            return Ok(());
        };
        let (Ok(tenant), Ok(user)) = (Uuid::parse_str(tenant), Uuid::parse_str(user)) else {
            return Ok(());
        };
        let Ok(tenant_id) = DataTenantId::new(tenant) else {
            return Ok(());
        };
        let mut conn = postgres.tenant_conn(tenant_id).await.map_err(store_error)?;
        let contact = live_contact(&mut conn, user).await.map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;
        self.project(tenant, user, contact.as_ref()).await
    }

    /// Resynchronize the directory with every active tenant's current
    /// contacts, removing entries whose contact is gone. A tenant that cannot
    /// be read is skipped and logged.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the tenant list
    /// cannot be read.
    pub async fn reconcile_directory(&self, postgres: &WyrdPostgres) -> Result<(), WyrdError> {
        for tenant_id in list_active_tenant_ids(&self.operator)
            .await
            .map_err(store_error)?
        {
            if let Err(error) = self.reconcile_tenant(postgres, tenant_id).await {
                tracing::warn!(%error, tenant = %tenant_id, "directory resync skipped a tenant");
            }
        }
        Ok(())
    }

    /// Resynchronize one tenant's entries.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when a read, the key or
    /// a write fails.
    async fn reconcile_tenant(
        &self,
        postgres: &WyrdPostgres,
        tenant_id: DataTenantId,
    ) -> Result<(), WyrdError> {
        let tenant = tenant_id.as_uuid();
        let mut conn = postgres.tenant_conn(tenant_id).await.map_err(store_error)?;
        let contacts = tenant_contacts(&mut conn).await.map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;
        for user in projected_users(&self.operator, tenant)
            .await
            .map_err(store_error)?
        {
            if !contacts.iter().any(|(holder, _)| *holder == user) {
                self.project(tenant, user, None).await?;
            }
        }
        for (user, contact) in &contacts {
            self.project(tenant, *user, Some(contact)).await?;
        }
        Ok(())
    }

    /// Project `contact` as `user`'s entry in `tenant`, or remove the entry
    /// when there is none.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the key or the
    /// write fails.
    async fn project(
        &self,
        tenant: Uuid,
        user: Uuid,
        contact: Option<&VerifiedContact>,
    ) -> Result<(), WyrdError> {
        let canonical =
            contact.and_then(|contact| EmailAddress::parse(&contact.address, "email").ok());
        match (contact, canonical) {
            (Some(contact), Some(address)) => {
                let key = self
                    .pseudonym(DIRECTORY_SCOPE, address.canonical.as_bytes())
                    .await?;
                project_contact(&self.operator, tenant, user, &key, contact.revision)
                    .await
                    .map_err(store_error)
            }
            _ => unproject_contact(&self.operator, tenant, user)
                .await
                .map_err(store_error),
        }
    }
}

/// Whether `candidate`'s User is active and its current contact is exactly
/// `address` at the projected revision, read through its tenant's own
/// connection. Any failure answers `false`.
async fn current_contact(
    postgres: &WyrdPostgres,
    candidate: &wyrd_sql::queries::platform::discovery::DirectoryCandidate,
    address: &str,
) -> bool {
    let Ok(tenant_id) = DataTenantId::new(candidate.data_tenant_id) else {
        return false;
    };
    let read = async {
        let mut conn = postgres.tenant_conn(tenant_id).await.map_err(store_error)?;
        let contact = live_contact(&mut conn, candidate.user_id)
            .await
            .map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;
        Ok::<_, WyrdError>(contact)
    };
    match read.await {
        Ok(Some(contact)) => {
            contact.revision == candidate.contact_revision
                && EmailAddress::parse(&contact.address, "email")
                    .is_ok_and(|current| current.canonical == address)
        }
        Ok(None) => false,
        Err(error) => {
            tracing::warn!(%error, "a directory entry could not be revalidated");
            false
        }
    }
}
