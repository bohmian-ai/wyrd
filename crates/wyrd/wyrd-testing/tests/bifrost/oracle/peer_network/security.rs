//! Peer workload authentication, and the ordering it must hold against decode.

use secrecy::ExposeSecret as _;
use wyrd_testing::bifrost::process_cluster::{
    BifrostProcessCluster, PeerProbeCredential, PeerProbeFraming, PeerProbePlan, PeerProbeService,
    ProcessNodeTarget,
};
use wyrd_testing::server::PeerPrincipalShape;

use super::support::PeerJourneyError;

/// Path of the compiled child every simulated pod runs.
const NODE_BINARY: &str = env!("CARGO_BIN_EXE_bifrost_peer_test_node");

/// Both private adapters share one authentication layer that runs to a verdict
/// before any request body is polled, admits exactly the configured peer
/// Service principal, and is indifferent to first-frame layout.
///
/// # Panics
///
/// Panics when any scenario in the table fails, naming the scenario.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn peer_authentication_precedes_body_admission() {
    prove_peer_authentication_precedes_body_admission()
        .await
        .expect("peer authentication journey");
}

/// Drives every peer-authentication scenario against one live topology.
///
/// # Errors
///
/// Returns the first scenario failure, which names the claim that broke.
async fn prove_peer_authentication_precedes_body_admission() -> Result<(), PeerJourneyError> {
    let mut cluster = BifrostProcessCluster::start(
        NODE_BINARY,
        &[ProcessNodeTarget::Oracle, ProcessNodeTarget::Oracle],
    )
    .await?;

    let wrong = WrongPrincipals::provision(&cluster).await?;
    let destination = cluster.nodes()[1].ready_report().advertise_addr.clone();

    refused_identities_never_reach_a_body(&mut cluster, &destination, &wrong)?;
    the_configured_principal_is_admitted_on_both_adapters(&mut cluster, &destination)?;
    first_frame_layout_does_not_change_admission(&mut cluster, &destination)?;

    cluster.shutdown();
    Ok(())
}

/// The deliberately wrong peer credentials one journey seeds.
///
/// Each is a real, exchangeable API key: the refusal under test is an
/// authorization verdict on a verified principal, not a malformed token.
struct WrongPrincipals {
    /// A different SYSTEM_OWNER service that also holds the peer permission.
    alternate: String,
    /// A SYSTEM_OWNER service holding no peer permission.
    unpermitted: String,
    /// A data-tenant service holding the peer permission.
    tenant_scoped: String,
}

impl WrongPrincipals {
    /// Seeds all three near-miss principals against the shared fixture.
    ///
    /// # Errors
    ///
    /// Returns the provisioning failure unchanged.
    async fn provision(cluster: &BifrostProcessCluster) -> Result<Self, PeerJourneyError> {
        let alternate = cluster
            .provision_peer_principal(PeerPrincipalShape::AlternateService)
            .await?;
        let unpermitted = cluster
            .provision_peer_principal(PeerPrincipalShape::WithoutPermission)
            .await?;
        let tenant_scoped = cluster
            .provision_peer_principal(PeerPrincipalShape::TenantScoped)
            .await?;
        Ok(Self {
            alternate: alternate.expose_secret().to_owned(),
            unpermitted: unpermitted.expose_secret().to_owned(),
            tenant_scoped: tenant_scoped.expose_secret().to_owned(),
        })
    }

    /// Returns each wrong identity with the refusal it must produce.
    fn cases(&self) -> Vec<(&'static str, PeerProbeCredential, &'static str)> {
        vec![
            (
                "no credential at all",
                PeerProbeCredential::Absent,
                "Unauthenticated",
            ),
            (
                "an unverifiable bearer",
                PeerProbeCredential::Invalid,
                "Unauthenticated",
            ),
            (
                "a different platform service",
                PeerProbeCredential::ApiKey(self.alternate.clone()),
                "PermissionDenied",
            ),
            (
                "a platform service without the peer permission",
                PeerProbeCredential::ApiKey(self.unpermitted.clone()),
                "PermissionDenied",
            ),
            (
                "a data-tenant service holding the peer permission",
                PeerProbeCredential::ApiKey(self.tenant_scoped.clone()),
                "PermissionDenied",
            ),
        ]
    }
}

/// Every private adapter this target serves.
const ADAPTERS: [PeerProbeService; 2] = [
    PeerProbeService::OraclePeer,
    PeerProbeService::AnalyticalWorker,
];

/// Reads the destination's body-poll counter through its own control channel.
///
/// # Errors
///
/// Returns the control-protocol failure unchanged.
fn body_polls(cluster: &mut BifrostProcessCluster) -> Result<u64, PeerJourneyError> {
    Ok(cluster.nodes_mut()[1].peer_body_polls()?)
}

/// Sends one probe from the first pod and reports the destination's outcome.
///
/// # Errors
///
/// Returns the control-protocol failure unchanged.
fn probe(
    cluster: &mut BifrostProcessCluster,
    plan: &PeerProbePlan,
) -> Result<String, PeerJourneyError> {
    Ok(cluster.nodes_mut()[0].peer_probe(plan)?)
}

/// No unauthorized identity causes the destination to poll a request body.
///
/// This is the ordering claim itself: authentication runs to a verdict on
/// metadata alone, so a refused caller never reaches transport admission, the
/// frame parser, or a protobuf decode on either adapter.
///
/// # Errors
///
/// Returns a message naming the adapter and identity that broke the claim.
fn refused_identities_never_reach_a_body(
    cluster: &mut BifrostProcessCluster,
    destination: &str,
    wrong: &WrongPrincipals,
) -> Result<(), PeerJourneyError> {
    for adapter in ADAPTERS {
        for (description, credential, expected) in wrong.cases() {
            let before = body_polls(cluster)?;
            let plan = PeerProbePlan::own(destination)
                .against(adapter)
                .presenting(credential);
            let outcome = probe(cluster, &plan)?;
            if outcome != expected {
                return Err(format!(
                    "{adapter:?} answered {description} with {outcome}, expected {expected}"
                )
                .into());
            }
            let after = body_polls(cluster)?;
            if after != before {
                return Err(format!(
                    "{adapter:?} polled {} request bodies for {description}",
                    after - before
                )
                .into());
            }
        }
    }
    Ok(())
}

/// The one configured peer Service principal is admitted on both adapters.
///
/// Admission is observed as the absence of an authentication verdict plus a
/// body poll the refused cases never produce: the request reached the plane
/// that decodes it.
///
/// # Errors
///
/// Returns a message naming the adapter that refused the configured identity.
fn the_configured_principal_is_admitted_on_both_adapters(
    cluster: &mut BifrostProcessCluster,
    destination: &str,
) -> Result<(), PeerJourneyError> {
    for adapter in ADAPTERS {
        let before = body_polls(cluster)?;
        let outcome = probe(cluster, &PeerProbePlan::own(destination).against(adapter))?;
        if outcome == "Unauthenticated" || outcome == "PermissionDenied" {
            return Err(format!(
                "{adapter:?} refused the configured peer principal with {outcome}"
            )
            .into());
        }
        if body_polls(cluster)? == before {
            return Err(format!(
                "{adapter:?} admitted the configured peer principal without reaching a body"
            )
            .into());
        }
    }
    Ok(())
}

/// A split or coalesced first frame is admitted exactly as a whole one is.
///
/// HTTP/2 never promises that a gRPC header arrives alone in its own DATA
/// frame, so a private listener that authenticates before decoding must not
/// acquire a different verdict from the same identity because of layout.
///
/// # Errors
///
/// Returns a message naming the layout whose verdict diverged.
fn first_frame_layout_does_not_change_admission(
    cluster: &mut BifrostProcessCluster,
    destination: &str,
) -> Result<(), PeerJourneyError> {
    let baseline = probe(cluster, &PeerProbePlan::own(destination))?;
    for framing in [PeerProbeFraming::SplitHeader, PeerProbeFraming::Coalesced] {
        let outcome = probe(cluster, &PeerProbePlan::own(destination).framed(framing))?;
        if outcome != baseline {
            return Err(format!(
                "a {framing:?} first frame answered {outcome}, but a whole frame answered {baseline}"
            )
            .into());
        }
    }
    Ok(())
}
