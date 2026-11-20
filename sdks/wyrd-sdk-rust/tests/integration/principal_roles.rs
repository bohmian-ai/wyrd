//! A tenant administrator discovers principals and binds and unbinds their
//! scoped roles through `Principals`; each change reaches the principal's
//! next request.
//! One stable Service account runs every release of its family, each run
//! keeping its own exact root.

use std::collections::BTreeSet;
use std::time::Duration;

use secrecy::ExposeSecret as _;
use serde::Deserialize;
use serde_json::json;
use wyrd_ai::auth::{
    AccessSummary, AccountAuthentication, BindingSubject, CreateRoleBindingRequest,
    CreateServicePrincipalRequest, CreateUserInvitationRequest, CreateUserRequest,
    CredentialListQuery, GrantSource, PermissionScope, PrincipalId, PrincipalKindTag,
    PrincipalQuery, Principals, RevokePrincipalRequest, ScopedRoleAssignment, SecretBearer,
    WorkloadFamily, WorkloadKind, WorkloadRestriction,
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

/// Each grant's `(role, source)`, in the summary's order.
fn grants(access: &AccessSummary) -> Vec<(&str, &GrantSource)> {
    access
        .grants
        .iter()
        .map(|grant| (grant.role.as_str(), &grant.source))
        .collect()
}

/// A request binding `role` tenant-wide to the Service or User `id`.
fn bind_all(id: PrincipalId, kind: PrincipalKindTag, role: &str) -> CreateRoleBindingRequest {
    CreateRoleBindingRequest {
        subject: BindingSubject::Principal { id, kind },
        role: role.to_owned(),
        scope: PermissionScope::All,
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
            assignments: vec![ScopedRoleAssignment::tenant("workload")],
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
/// until an administrator binds `editor`, and the unbind reaches its next
/// request. A repeated bind returns the same binding.
///
/// # Panics
/// Panics when a step fails or an outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn bound_editor_reaches_the_next_request_until_unbound() {
    let (deployment, principal, key) = service_principal().await;
    let principals = Principals::connect_with(&deployment.admin())
        .await
        .expect("handle connects");

    let held = principals.access(&principal).await.expect("access reads");
    assert_eq!(held.kind, PrincipalKindTag::Service);
    assert_eq!(grants(&held), [("workload", &GrantSource::Direct)]);
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

    let editor = bind_all(principal, PrincipalKindTag::Service, "editor");
    let granted = principals
        .bind(&editor)
        .await
        .expect("the administrator binds editor");
    assert_eq!(granted.scope, PermissionScope::All);
    let again = principals
        .bind(&editor)
        .await
        .expect("a repeat bind succeeds");
    assert_eq!(again.id, granted.id);
    let held = principals.access(&principal).await.expect("access reads");
    assert_eq!(
        grants(&held),
        [
            ("workload", &GrantSource::Direct),
            ("editor", &GrantSource::Direct)
        ]
    );
    author(&deployment, &key)
        .await
        .expect("editor authors at its next request");

    principals
        .unbind(&granted.id)
        .await
        .expect("the administrator unbinds editor");
    let held = principals.access(&principal).await.expect("access reads");
    assert_eq!(grants(&held), [("workload", &GrantSource::Direct)]);
    let refused = author(&deployment, &key)
        .await
        .expect_err("the unbind reaches the next request");
    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    deployment.shutdown().await;
}

/// The Service's own key, which is no tenant administrator, can neither
/// bind nor unbind a role.
///
/// # Panics
/// Panics when a write succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn only_a_tenant_admin_assigns_roles() {
    let (deployment, principal, key) = service_principal().await;
    let admin = Principals::connect_with(&deployment.admin())
        .await
        .expect("handle connects");
    let workload_binding =
        admin.access(&principal).await.expect("access reads").grants[0].binding_id;
    let principals = Principals::connect_with(&deployment.client(&key))
        .await
        .expect("handle connects");

    let granted = principals
        .bind(&bind_all(principal, PrincipalKindTag::Service, "editor"))
        .await
        .expect_err("a workload cannot bind");
    let revoked = principals
        .unbind(&workload_binding)
        .await
        .expect_err("a workload cannot unbind");

    assert_eq!(granted.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    assert_eq!(revoked.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    deployment.shutdown().await;
}

/// A tenant administrator creates a local User holding a scoped role and
/// invites another with one; policy, duplicate and unknown-role inputs are
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
        assignments: vec![ScopedRoleAssignment::tenant(role)],
    };

    let created = admin
        .create_user(&request("Development", "development", "admin"))
        .await
        .expect("an administrator creates a User");
    assert_eq!(created.username, "development");
    let access = admin
        .access(&created.principal_id)
        .await
        .expect("access reads");
    assert_eq!(grants(&access), [("admin", &GrantSource::Direct)]);
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
            "an unknown role",
        ),
    ] {
        let error = admin.create_user(&refused).await.expect_err(why);
        assert_eq!(error.code(), "WYRD_SPEC_400_VALIDATION", "{why}");
    }

    let invited = admin
        .invite_user(&CreateUserInvitationRequest {
            assignments: vec![ScopedRoleAssignment::tenant("editor")],
        })
        .await
        .expect("an administrator invites");
    assert_eq!(invited.invitation_code.expose().len(), 43);
    assert_eq!(
        invited.invitation.assignments,
        [ScopedRoleAssignment::tenant("editor")]
    );
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
            assignments: vec![ScopedRoleAssignment::tenant("admin")],
        })
        .await
        .expect_err("a workload cannot invite");
    assert_eq!(create.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    assert_eq!(invite.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    deployment.shutdown().await;
}

/// A signed-in user discovered by email keeps its identity-provider binding
/// beside a direct binding of the same role; unbinding removes only the
/// direct one.
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
        .bind(&bind_all(id, PrincipalKindTag::User, "viewer"))
        .await
        .expect("the administrator binds viewer directly");
    let access = principals.access(&id).await.expect("access reads");
    assert_eq!(
        grants(&access),
        [
            ("viewer", &GrantSource::Provider),
            ("viewer", &GrantSource::Direct)
        ]
    );
    principals
        .unbind(&granted.id)
        .await
        .expect("the administrator unbinds viewer");
    let access = principals.access(&id).await.expect("access reads");
    assert_eq!(grants(&access), [("viewer", &GrantSource::Provider)]);
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
            assignments: vec![ScopedRoleAssignment::tenant("workload")],
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

/// Viewer bound at the `code-review` Verifier family lets a local User, signed
/// in through the CLI device login with no identity provider, read that
/// family and nothing else: the other family is absent from listing and
/// refused on a direct read.
///
/// # Panics
/// Panics when a step fails or an outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn a_family_bound_viewer_reads_only_that_family() {
    use wyrd_ai::cards::{CardKind, CardSelector, ListCardsRequest};
    use wyrd_ai::config::ClientConfig;
    use wyrd_ai::environment::Environment;
    use wyrd_ai::{GlobalConfig, WyrdClient};
    use wyrd_spec::auth::CardPermissionScope;
    use wyrd_spec::ids::CardName;
    use wyrd_testing::human_login::{FIXTURE_TENANT_SLUG, HumanSso};

    let deployment = Deployment::start().await;
    let cards = deployment.cards().await;
    declare(&cards, "cards/repository_declaration/code-review.yaml").await;
    declare(&cards, "cards/repository_declaration/unrelated.yaml").await;
    let payments = CardName::new("code-review").expect("valid name");
    let other = CardName::new("unrelated-check").expect("valid name");
    let admin = Principals::connect_with(&deployment.admin())
        .await
        .expect("handle connects");
    let password = "alice payments password";
    admin
        .create_user(&CreateUserRequest {
            username: "alice".to_owned(),
            password: SecretBearer::new(password.to_owned()),
            assignments: vec![ScopedRoleAssignment {
                role: "viewer".to_owned(),
                scope: PermissionScope::Card(CardPermissionScope::family(
                    CardKind::Verifier,
                    payments.clone(),
                )),
            }],
        })
        .await
        .expect("an administrator creates Alice");
    let sso = HumanSso::new(
        deployment
            .server()
            .base_url()
            .expect("bound server has a URL"),
    );
    let config = wyrd_testing::human_login::private_config_home();
    sso.save_local_login(config.path(), FIXTURE_TENANT_SLUG, "alice", password)
        .await;
    let saved = ClientConfig::from_environment(
        Environment::from([(
            "WYRD_CONFIG_HOME",
            config.path().to_str().expect("UTF-8 config home"),
        )]),
        &GlobalConfig::default(),
        deployment.server().base_url(),
        None,
    );
    let own = Cards::connect_with(&WyrdClient::with_config(saved).expect("client builds"))
        .await
        .expect("Alice connects with her saved login");

    let listed = own
        .list(ListCardsRequest {
            kind: Some(CardKind::Verifier),
            name: None,
            version_range: None,
            status: None,
            filter: None,
            include_prerelease: false,
            limit: None,
            cursor: None,
        })
        .await
        .expect("Alice lists Verifiers");
    let names: Vec<&str> = listed.items.iter().map(|card| card.name.as_str()).collect();
    assert_eq!(names, ["code-review"]);
    own.get(CardSelector::Named {
        kind: CardKind::Verifier,
        name: payments,
        version: None,
    })
    .await
    .expect("Alice reads the payments family");
    let refused = own
        .get(CardSelector::Named {
            kind: CardKind::Verifier,
            name: other,
            version: None,
        })
        .await
        .expect_err("the other family is hidden");
    assert_eq!(refused.code(), "WYRD_REGISTRY_404_CARD_NOT_FOUND");
    deployment.shutdown().await;
}
