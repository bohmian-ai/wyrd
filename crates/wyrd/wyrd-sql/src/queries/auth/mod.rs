//! Tenant-scoped auth queries for `wyrd.auth_*` tables.
//!
//! Functions in this module tree take `&mut TenantConn<'_>`. Database RLS is
//! the load-bearing tenant boundary; selected queries also include explicit
//! `data_tenant_id = $...` predicates when needed for joins or indexes.
//! Keep short single-statement queries inline; put joins, CTEs, batch writes,
//! and reused statements under `queries/auth/sql/` and load them with SQLx's
//! query-file macros.

/// Users and Service accounts a tenant administrator can assign Roles to, in
/// one shape. The tenant administrator and the internal `system` writer are
/// absent: their authority is fixed and never re-assigned. Shared by the
/// principal directory and Role member counts so both describe the same
/// people.
macro_rules! assignable_principals_union {
    () => {
        r"
                SELECT u.id, 'user'::text AS kind, u.status, u.email,
                       u.username AS name, u.display_name,
                       (SELECT i.issuer FROM wyrd.auth_user_identities i
                         WHERE i.data_tenant_id = u.data_tenant_id AND i.user_id = u.id
                         ORDER BY i.created_at, i.issuer LIMIT 1) AS identity_issuer,
                       NULL::text AS restriction_kind, NULL::text AS restriction_name
                  FROM wyrd.auth_users u
                 WHERE u.data_tenant_id = wyrd.current_tenant()
                   AND u.status <> 'deleted'
                UNION ALL
                SELECT s.id, s.principal_kind AS kind, s.status, NULL::text AS email,
                       s.name, NULL::text AS display_name, NULL::text AS identity_issuer,
                       s.restriction_kind, s.restriction_name
                  FROM wyrd.auth_service_accounts s
                 WHERE s.data_tenant_id = wyrd.current_tenant()
                   AND s.principal_kind = 'service'
                   AND s.status <> 'deleted'
        "
    };
}

/// Ids of accounts holding a Role at any scope, directly or through a Group,
/// for the Role rows `r` the predicate selects. Group bindings contribute
/// each member once; a Group without members contributes nothing.
macro_rules! role_holders {
    ($role_predicate:literal) => {
        concat!(
            r"
                SELECT COALESCE(b.user_id, b.service_account_id, m.user_id, m.service_account_id)
                  FROM wyrd.auth_role_bindings b
                  JOIN wyrd.auth_roles r
                    ON r.data_tenant_id = b.data_tenant_id AND r.id = b.role_id
                  LEFT JOIN wyrd.auth_group_memberships m
                    ON m.data_tenant_id = b.data_tenant_id AND m.group_id = b.group_id
                 WHERE b.data_tenant_id = wyrd.current_tenant()
                   AND ",
            $role_predicate
        )
    };
}

pub mod api_keys;
pub mod authority;
pub mod browser_sessions;
pub mod contacts;
pub mod device_authorizations;
pub mod human_connections;
pub mod local_passwords;
pub mod login_policy;
pub mod login_state;
/// Discovery of the users and Service or Agent principals a tenant
/// administrator can assign Roles to.
pub mod principal_directory;
pub mod refresh_tokens;
pub mod revocation;
pub mod role_bindings;
pub mod roles;
pub mod service_accounts;
pub mod trusted_issuers;
pub mod user_identities;
pub mod users;
pub mod workload_bindings;

pub use api_keys::{
    ApiKeyMetadataRow, credential_belongs_to, list_api_key_metadata, page_api_key_metadata,
    revoke_api_key,
};
pub use authority::{
    AuthorityApiKey, AuthorityBinding, AuthorityMembership, AuthorityServiceAccount,
    AuthorityState, authority_state,
};
pub use browser_sessions::{
    BrowserSessionAuthority, LiveBrowserSession, SocialAnchor, insert_browser_session,
    live_browser_session, revoke_browser_session,
};
pub use contacts::{
    CONTACT_CHANNEL, VerifiedContact, insert_verified_contact, live_contact,
    replace_verified_contact, tenant_contacts, verified_contact_for_update,
};
pub use device_authorizations::{
    DevicePoll, approve_device_authorization, delete_device_authorization,
    deny_device_authorization, insert_device_authorization, pending_device_authorization,
    poll_device_authorization,
};
pub use human_connections::{
    HumanConnectionWrite, StoredSecretRow, StoredSecretTable, deactivate_active_human_connection,
    human_candidate_test_is_current, human_connection_in_state, human_connection_is_active,
    insert_human_candidate, live_human_connections, lock_human_connection_slot,
    promote_tested_human_candidate, remove_human_connection, renew_human_connection_secret,
    replace_human_candidate, stamp_human_candidate_tested, stored_secrets, swap_stored_secret,
};
pub use local_passwords::{
    AttemptScope, InvitationRow, PasswordResetRow, attempt_refusal, consume_invitation,
    consume_password_reset, insert_invitation, insert_password_reset, pending_invitations,
    record_attempt_failure, record_invitation_delivery, reissue_invitation, revoke_invitation,
};
pub use login_state::{
    GithubLoginState, LoginState, SocialLoginState, bind_github_login_state,
    consume_github_login_state, consume_login_state, consume_social_login_state,
    insert_github_login_state, insert_login_state, insert_social_login_state,
};
pub use principal_directory::{
    AssignablePrincipalRow, PrincipalFilter, PrincipalStatusCountsRow, assignable_principal,
    count_assignable_principals, list_assignable_principals,
};
pub use refresh_tokens::{
    consume_active_refresh, insert_human_refresh_token, lock_refresh_family, refresh_by_hash,
    refresh_issuance_instant, revoke_refresh, revoke_refresh_chain, revoke_refresh_family,
};
pub use revocation::{suspend_service_account_principal, suspend_user_principal};
pub use role_bindings::{
    AUTHORIZATION_CHANNEL, MembershipRow, RoleBindingRow, SubjectColumns,
    advance_authorization_revision, current_authorization_revision, delete_group,
    delete_group_membership, delete_role_binding, insert_group, insert_group_membership,
    insert_role_binding, list_group_memberships, list_groups, list_role_bindings,
    replace_idp_user_bindings,
};
pub use roles::{
    RoleListRow, RoleRow, delete_role, insert_role, list_roles, role_by_id, role_by_name,
    roles_by_name, set_role_description,
};
pub use service_accounts::{
    ApiKeyLookupRow, ApiKeyOwner, ApiKeyStatus, ServiceAccountPrincipalRow, api_key_by_prefix,
    api_key_owner, api_key_status_by_prefix, delete_service_account, family_account_id,
    insert_api_key, insert_refresh_token, insert_service_account, insert_user_api_key,
    provision_system_principal, service_account_by_id, set_workload_restriction,
    system_principal_id, tenant_admin_principal_id, touch_api_key_last_used, workload_family,
};
pub use trusted_issuers::{
    TrustedIssuerWrite, delete_trusted_issuer, insert_trusted_issuer, swap_trusted_issuer_secret,
    trusted_issuer_by_url, trusted_issuer_exists, trusted_issuers_for_tenant,
    upsert_trusted_issuer,
};
pub use user_identities::{
    LinkedIdentity, insert_user_identity, linked_identity, upsert_user_identity,
    user_id_by_identity,
};
pub use users::{
    LocalCredentialRow, UserProfileRow, UserRow, delete_user, first_local_user, insert_local_user,
    insert_user, local_credential_by_username, local_credential_for_update, local_username,
    refresh_display_name, set_password_hash, user_by_id, user_profile,
};
pub use workload_bindings::{
    WorkloadBindingWrite, delete_workload_binding, delete_workload_bindings_for_issuer,
    insert_workload_binding, upsert_workload_binding, workload_binding_by_key,
    workload_binding_by_subject, workload_bindings_for_tenant,
};
