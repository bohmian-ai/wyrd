//! A tenant administrator discovers principals and grants and revokes their
//! Roles through `Principals`; each change reaches the principal's next token.
//! One stable Service account runs every release of its family, each run
//! keeping its own exact root.

use std::collections::BTreeSet;
use std::time::Duration;

use secrecy::ExposeSecret as _;
use serde::Deserialize;
use serde_json::json;
use wyrd_ai::auth::{
    AccountAuthentication, CreateServicePrincipalRequest, CreateUserInvitationRequest,
    CreateUserRequest, CredentialListQuery, PrincipalId, PrincipalKindTag, PrincipalQuery,
    Principals, RevokePrincipalRequest, RoleAssignment, RoleSource, SecretBearer, WorkloadFamily,
    WorkloadKind, WorkloadRestriction,
};
use wyrd_ai::bifrost::{BifrostOptions, QueryParam};
use wyrd_ai::cards::Cards;
use wyrd_ai::observe::Run;
use wyrd_ai::state::WyrdState;
use wyrd_ai::{Bifrost, WyrdError};
use wyrd_testing::Bootstrap;
use wyrd_testing::server::WyrdTestServer;

use crate::support::{Deployment, declare, fixture, pull, runtime_account};

/// A tenant-wide query over a built-in table, which `workload` permits.
const TENANT_WIDE: &str = "SELECT COUNT(*) AS n FROM bifrost.drift.observations";

/// A Card only an author can register.
const AUTHORED: &str = "cards/gateway_inference/ask-prompt.yaml";

/// One `COUNT(*)` result.
#[derive(Debug, Deserialize)]
struct Count {
    /// The counted rows.
    n: i64,
}

/// One `(role, source)` assignment.
fn assignment(role: &str, source: RoleSource) -> RoleAssignment {
    RoleAssignment {
        role: role.to_owned(),
        source,
    }
}

/// A deployment with a new Service account holding only `workload`, its
/// principal id, and its first key.
///
/// # Panics
/// Panics when the account cannot be created.
async fn service_principal() -> (Deployment, PrincipalId, String) {
    let deployment = Deployment::start().await;
    let created = Principals::connect_with(&deployment.admin())
        .await
        .expect("handle connects")
        .create_service_principal(&CreateServicePrincipalRequest {
            name: "observed-service-runtime".to_owned(),
            roles: vec!["workload".to_owned()],
            description: None,
            workload_restriction: None,
        })
        .await
        .expect("the Service account is created");
    let key = created.credential.expose().to_owned();
    (deployment, created.principal_id, key)
}

/// Register [`AUTHORED`] as the principal holding `key`, through a fresh
/// client so the call carries the principal's next token.
///
/// # Errors
/// Returns the server's refusal of the key's exchange, the registration
/// error, or `WYRD_PERMISSION_403_DENIED_RBAC` when the principal's Roles do
/// not include Card authoring.
async fn author(deployment: &Deployment, key: &str) -> Result<(), WyrdError> {
    Cards::connect_with(&deployment.client(key))
        .await?
        .declare_from_path(&fixture(AUTHORED))
        .await
        .map(|_| ())
}

/// A new Service key holds direct `workload`: it queries but cannot author
/// until an administrator grants `editor`, and the revoke reaches its next
/// token. Grants and revokes are idempotent.
///
/// # Panics
/// Panics when a step fails or an outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn granted_editor_reaches_the_next_token_until_revoked() {
    let (deployment, principal, key) = service_principal().await;
    let principals = Principals::connect_with(&deployment.admin())
        .await
        .expect("handle connects");

    let held = principals.roles(&principal).await.expect("roles list");
    assert_eq!(held.kind, PrincipalKindTag::Service);
    assert_eq!(held.roles, [assignment("workload", RoleSource::Direct)]);
    let counted: Vec<Count> =
        Bifrost::connect_with(&deployment.client(&key), BifrostOptions::default())
            .await
            .expect("Bifrost connects")
            .sql_as(TENANT_WIDE, &[])
            .await
            .expect("workload queries tenant-wide");
    assert_eq!(counted.len(), 1);
    assert!(counted[0].n >= 0);
    let refused = author(&deployment, &key)
        .await
        .expect_err("workload cannot author");
    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");

    let granted = principals
        .grant_role(&principal, "editor")
        .await
        .expect("the administrator grants editor");
    assert!(granted.changed);
    assert_eq!(
        granted.roles,
        [
            assignment("editor", RoleSource::Direct),
            assignment("workload", RoleSource::Direct)
        ]
    );
    let again = principals
        .grant_role(&principal, "editor")
        .await
        .expect("a repeat grant succeeds");
    assert!(!again.changed);
    author(&deployment, &key)
        .await
        .expect("editor authors at its next token");

    let revoked = principals
        .revoke_role(&principal, "editor")
        .await
        .expect("the administrator revokes editor");
    assert!(revoked.changed);
    assert_eq!(revoked.roles, [assignment("workload", RoleSource::Direct)]);
    let refused = author(&deployment, &key)
        .await
        .expect_err("the revoke reaches the next token");
    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    deployment.shutdown().await;
}

/// The Service's own key, which is no tenant administrator, can neither
/// grant nor revoke a Role.
///
/// # Panics
/// Panics when a write succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn only_a_tenant_admin_assigns_roles() {
    let (deployment, principal, key) = service_principal().await;
    let principals = Principals::connect_with(&deployment.client(&key))
        .await
        .expect("handle connects");

    let granted = principals
        .grant_role(&principal, "editor")
        .await
        .expect_err("a workload cannot grant");
    let revoked = principals
        .revoke_role(&principal, "workload")
        .await
        .expect_err("a workload cannot revoke");

    assert_eq!(granted.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    assert_eq!(revoked.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    deployment.shutdown().await;
}

/// A tenant administrator creates a local User holding the named Role and
/// invites another by Role name; policy, duplicate and unknown-Role inputs are
/// refused, and a workload that is no administrator can do neither.
///
/// # Panics
/// Panics when a step fails or an outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn administrators_create_and_invite_users() {
    let (deployment, _, key) = service_principal().await;
    let admin = Principals::connect_with(&deployment.admin())
        .await
        .expect("handle connects");
    let request = |username: &str, password: &str, role: &str| CreateUserRequest {
        username: username.to_owned(),
        password: SecretBearer::new(password.to_owned()),
        roles: vec![role.to_owned()],
    };

    let created = admin
        .create_user(&request("Development", "development", "admin"))
        .await
        .expect("an administrator creates a User");
    assert_eq!(created.username, "development");
    let roles = admin
        .roles(&created.principal_id)
        .await
        .expect("roles read");
    assert_eq!(roles.roles, vec![assignment("admin", RoleSource::Direct)]);
    for (refused, why) in [
        (
            request("development", "development", "admin"),
            "a taken username",
        ),
        (
            request("short", "seven77", "admin"),
            "a password under eight characters",
        ),
        (
            request("unknown", "development", "no-such-role"),
            "an unknown Role",
        ),
    ] {
        let error = admin.create_user(&refused).await.expect_err(why);
        assert_eq!(error.code(), "WYRD_SPEC_400_VALIDATION", "{why}");
    }

    let invited = admin
        .invite_user(&CreateUserInvitationRequest {
            roles: vec!["editor".to_owned()],
        })
        .await
        .expect("an administrator invites");
    assert_eq!(invited.invitation_code.expose().len(), 43);
    assert_eq!(invited.invitation.role_ids.len(), 1);
    assert!(invited.signup_url.as_str().ends_with("/signup"));

    let workload = Principals::connect_with(&deployment.client(&key))
        .await
        .expect("handle connects");
    let create = workload
        .create_user(&request("intruder", "development", "admin"))
        .await
        .expect_err("a workload cannot create Users");
    let invite = workload
        .invite_user(&CreateUserInvitationRequest {
            roles: vec!["admin".to_owned()],
        })
        .await
        .expect_err("a workload cannot invite");
    assert_eq!(create.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    assert_eq!(invite.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    deployment.shutdown().await;
}

/// A signed-in user discovered by email keeps its identity-provider Role
/// beside a direct grant of the same Role; revoking removes only the direct
/// one.
///
/// # Panics
/// Panics when a step fails or an outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn direct_and_idp_user_assignments_coexist() {
    let deployment = Deployment::start().await;
    let Bootstrap::User { id, .. } = deployment
        .server()
        .bootstrap_user("pr-person", &["viewer"])
        .await
        .expect("user signs in")
    else {
        panic!("user bootstrap returned a machine principal");
    };
    let principals = Principals::connect_with(&deployment.admin())
        .await
        .expect("handle connects");

    let page = principals
        .list(&PrincipalQuery {
            email: Some("pr-person@test.wyrd".to_owned()),
            ..PrincipalQuery::default()
        })
        .await
        .expect("discovery lists");
    assert_eq!(page.principals.len(), 1);
    assert_eq!(page.principals[0].principal_id, id);
    assert_eq!(page.principals[0].kind, PrincipalKindTag::User);
    assert_eq!(page.next, None);

    let granted = principals
        .grant_role(&id, "viewer")
        .await
        .expect("the administrator grants viewer directly");
    assert!(granted.changed);
    assert_eq!(
        granted.roles,
        [
            assignment("viewer", RoleSource::Direct),
            assignment("viewer", RoleSource::Idp)
        ]
    );
    let revoked = principals
        .revoke_role(&id, "viewer")
        .await
        .expect("the administrator revokes viewer");
    assert!(revoked.changed);
    assert_eq!(revoked.roles, [assignment("viewer", RoleSource::Idp)]);
    deployment.shutdown().await;
}

/// A local User reads their own profile and key metadata, page by page,
/// through their own restricted key, and revokes it; the key never carries
/// more than its ceiling and a Service is never given a human profile.
///
/// # Panics
/// Panics when the deployment fails to start or any expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn users_manage_personal_keys() {
    let deployment = Deployment::start().await;
    let ceiling = [
        "personal_credentials:write",
        "cards:read",
        "personal_credentials:read",
    ];
    let (user, key) = deployment
        .server()
        .bootstrap_user_key("keeper", &["editor"], &ceiling)
        .await
        .expect("a local User creates a key");
    let own = Principals::connect_with(&deployment.client(key.expose_secret()))
        .await
        .expect("handle connects");

    let me = own.me().await.expect("the User reads their profile");
    assert_eq!(me.principal_id, user);
    assert_eq!(me.principal_kind, PrincipalKindTag::User);
    assert_eq!(me.username.as_deref(), Some("keeper"));
    assert_eq!(me.authentication, AccountAuthentication::Password);
    assert_eq!(
        me.permissions,
        [
            "cards:read",
            "personal_credentials:read",
            "personal_credentials:write"
        ]
    );

    let page = own
        .list_credentials_page(&user, CredentialListQuery::default())
        .await
        .expect("the User pages their keys");
    assert_eq!(page.next_cursor, None);
    let [listed] = page.credentials.as_slice() else {
        panic!("one key: {page:?}");
    };
    assert_eq!(listed.name.as_deref(), Some("keeper"));
    assert_eq!(
        listed.permission_ceiling.as_deref(),
        Some(&me.permissions[..])
    );
    assert!(listed.expires_at.is_some());
    let unpaged = own
        .list_credentials(&user)
        .await
        .expect_err("a User lists their keys page by page");
    assert_eq!(unpaged.code(), "WYRD_SPEC_400_VALIDATION");
    let issued = own
        .issue_credential(&user)
        .await
        .expect_err("a User key is created only from a recent sign-in");
    assert_eq!(issued.status(), 400);

    // More than a page of keys created in one instant pages exactly once.
    let mut expected: BTreeSet<_> = deployment
        .server()
        .seed_user_keys(user, 105)
        .await
        .expect("keys seed")
        .into_iter()
        .collect();
    expected.insert(listed.id);
    let mut seen = BTreeSet::new();
    let mut cursor = None;
    loop {
        let page = own
            .list_credentials_page(
                &user,
                CredentialListQuery {
                    limit: Some(100),
                    cursor,
                },
            )
            .await
            .expect("the User pages their keys");
        assert!(page.credentials.len() <= 100);
        for key in &page.credentials {
            assert!(seen.insert(key.id), "{} listed twice", key.id);
        }
        let Some(next) = page.next_cursor else { break };
        cursor = Some(next);
    }
    assert_eq!(seen, expected);

    let admin = Principals::connect_with(&deployment.admin())
        .await
        .expect("handle connects");
    let complete = admin
        .list_credentials(&user)
        .await
        .expect("an administrator lists completely");
    assert_eq!(
        complete
            .credentials
            .iter()
            .map(|key| key.id)
            .collect::<BTreeSet<_>>(),
        expected
    );
    let service = admin
        .me()
        .await
        .expect_err("a Service has no human profile");
    assert_eq!(service.code(), "WYRD_PERMISSION_403_DENIED_RBAC");

    own.revoke_credential(&user, listed.id)
        .await
        .expect("the User revokes their key");
    let revoked = Principals::connect_with(&deployment.client(key.expose_secret()))
        .await
        .expect_err("a revoked key no longer exchanges");
    assert_eq!(revoked.status(), 401);
    deployment.shutdown().await;
}

/// Access-token lifetime of the stable-account story: five seconds longer
/// than the client's 30-second refresh skew, so a cached token is renewed
/// after [`PAST_REUSE_WINDOW`].
const ACCESS_TTL: chrono::Duration = chrono::Duration::seconds(35);

/// Wait after which a cached token is inside the refresh skew.
const PAST_REUSE_WINDOW: Duration = Duration::from_secs(6);

/// One `bifrost.drift.observations` row's subject and run.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Observed {
    /// The observed Card.
    subject_card_uid: String,
    /// The run the sample belongs to.
    run_id: String,
}

/// Submit one `observed-drift` latency sample for `run`'s Model.
///
/// # Panics
/// Panics when the submission is refused.
async fn observe(run: &Run) {
    run.for_card("model")
        .expect("model view resolves")
        .observe()
        .submit("observed-drift", json!({ "latency": 12.5 }))
        .await
        .expect("the sample is submitted");
}

/// One administrator-provisioned Service account runs two releases of its
/// family: it is restricted to the family, a second account cannot claim the
/// family, one client of its key drives a state per release concurrently and
/// across token renewal with each run's samples landing on the shared Model,
/// a root of another family is refused, and revoking the account ends every
/// exchange of its key.
///
/// # Panics
/// Panics when a step fails or an outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn stable_accounts_preserve_execution_identity() {
    let deployment =
        Deployment::start_with(WyrdTestServer::builder().with_access_ttl(ACCESS_TTL)).await;
    let cards = deployment.cards().await;
    declare(&cards, "cards/observe_a_run/observed-model.yaml").await;
    let first = declare(&cards, "cards/observe_a_run/observed-service.yaml").await;
    let second = declare(&cards, "cards/observe_a_run/observed-service-2.yaml").await;
    let (first_bundle, second_bundle) = (
        pull(&cards, &first.root).await,
        pull(&cards, &second.root).await,
    );
    let admin = deployment.admin();
    let principals = Principals::connect_with(&admin)
        .await
        .expect("handle connects");
    let (account, key) = runtime_account(&admin, &first.root).await;
    let family = WorkloadRestriction::Family(WorkloadFamily {
        kind: WorkloadKind::Service,
        name: first.root.name.clone(),
    });

    let listed = principals
        .list(&PrincipalQuery {
            name: Some("observed-service-runtime".to_owned()),
            ..PrincipalQuery::default()
        })
        .await
        .expect("principals list")
        .principals;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].principal_id, account);
    assert_eq!(listed[0].workload_restriction.as_ref(), Some(&family));
    let spare = principals
        .create_service_principal(&CreateServicePrincipalRequest {
            name: "spare-runtime".to_owned(),
            roles: vec!["workload".to_owned()],
            description: None,
            workload_restriction: Some(WorkloadRestriction::Unrestricted),
        })
        .await
        .expect("an unrestricted account is created");
    let conflict = principals
        .set_workload_restriction(&spare.principal_id, &family)
        .await
        .expect_err("the family already has its runtime account");
    assert_eq!(conflict.code(), "WYRD_AUTH_409_WORKLOAD_BINDING_CONFLICT");

    let client = deployment.client(&key);
    let states = [&first_bundle, &second_bundle].map(|bundle| {
        WyrdState::from_path_with(bundle.path().join("bundle"), client.clone())
            .expect("bundle loads offline")
    });
    for state in &states {
        state.start_bifrost().await.expect("Bifrost starts");
    }
    let runs = [states[0].run(), states[1].run()];
    tokio::join!(observe(&runs[0]), observe(&runs[1]));
    tokio::time::sleep(PAST_REUSE_WINDOW).await;
    tokio::join!(observe(&runs[0]), observe(&runs[1]));
    for state in &states {
        state.shutdown().await.expect("samples drain");
    }
    deployment
        .server()
        .flush_bifrost()
        .await
        .expect("rows publish");
    let model = states[0]
        .card_ref("model")
        .expect("model resolves")
        .uid
        .clone()
        .expect("hydrated Card carries its UID")
        .to_string();
    let bifrost = Bifrost::connect_with(&admin, BifrostOptions::default())
        .await
        .expect("Bifrost connects");
    for run in &runs {
        let run_id = run.run_id().to_string();
        let rows: Vec<Observed> = bifrost
            .sql_as(
                "SELECT subject_card_uid, run_id FROM bifrost.drift.observations WHERE run_id = $1",
                &[QueryParam::String(run_id.clone())],
            )
            .await
            .expect("rows read back");
        let expected = Observed {
            subject_card_uid: model.clone(),
            run_id,
        };
        assert_eq!(rows, [expected.clone(), expected]);
    }

    let foreign = states[0]
        .card_ref("agent")
        .expect("agent resolves")
        .uid
        .clone()
        .expect("hydrated Card carries its UID");
    let refused = client
        .for_workload(foreign)
        .access_token()
        .await
        .expect_err("a root outside the family is refused");
    assert_eq!(refused.code(), "WYRD_AUTH_401_WORKLOAD_UID_REFUSED");

    principals
        .revoke_principal(
            &account,
            &RevokePrincipalRequest {
                principal_kind: PrincipalKindTag::Service,
                reason: "the Service is retired".to_owned(),
            },
        )
        .await
        .expect("the account is revoked");
    let revoked = deployment
        .client(&key)
        .access_token()
        .await
        .expect_err("a revoked account's key no longer exchanges");
    assert_eq!(revoked.code(), "WYRD_AUTH_401_API_KEY_INVALID");
    deployment.shutdown().await;
}
