//! Semantic Kubernetes contract checks for Bifrost role routing and rollback.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Minimal metadata needed to correlate Kubernetes resources.
#[derive(Debug, Deserialize)]
struct Metadata {
    /// Resource name used by services and routes.
    name: String,
}

/// One Kubernetes document projected onto the Bifrost contract fields.
#[derive(Debug, Deserialize)]
struct Resource {
    /// Kubernetes resource kind.
    kind: String,
    /// Resource identity.
    metadata: Metadata,
    /// Kind-specific specification retained as semantic YAML.
    spec: serde_yaml::Value,
}

/// Deployment facts that determine role routing and readiness.
#[derive(Debug)]
struct DeploymentFacts {
    /// Labels placed on each pod.
    labels: BTreeMap<String, String>,
    /// Enabled Wyrd roles from `WYRD_ROLES`.
    roles: BTreeSet<String>,
    /// Container image, used to distinguish rollback from the candidate image.
    image: String,
    /// HTTP readiness path.
    readiness_path: String,
    /// Environment variables projected by stable name.
    environment: BTreeMap<String, String>,
    /// Container mount paths projected by volume name.
    volume_mounts: BTreeMap<String, String>,
    /// Pod volumes backed by an `emptyDir` object.
    empty_dir_volumes: BTreeSet<String>,
}

/// Complete semantic projection of one multi-document manifest.
#[derive(Debug)]
struct ManifestFacts {
    /// Deployments indexed by resource name.
    deployments: BTreeMap<String, DeploymentFacts>,
    /// Service selectors indexed by service name.
    services: BTreeMap<String, BTreeMap<String, String>>,
    /// HTTP route path and backend pairs.
    http_routes: Vec<(String, String)>,
    /// gRPC route service and backend pairs.
    grpc_routes: Vec<(String, String)>,
}

/// Parse one Kubernetes multi-document manifest without relying on layout or text formatting.
///
/// # Errors
///
/// Returns an error when YAML is malformed or a required projected field has
/// the wrong semantic type.
fn parse_manifest(source: &str) -> Result<ManifestFacts, String> {
    let mut facts = ManifestFacts {
        deployments: BTreeMap::new(),
        services: BTreeMap::new(),
        http_routes: Vec::new(),
        grpc_routes: Vec::new(),
    };
    for document in serde_yaml::Deserializer::from_str(source) {
        let resource = Resource::deserialize(document).map_err(|error| error.to_string())?;
        match resource.kind.as_str() {
            "Deployment" => {
                let selector = string_map(at(&resource.spec, &["selector", "matchLabels"])?)?;
                let labels = string_map(at(&resource.spec, &["template", "metadata", "labels"])?)?;
                if !selector
                    .iter()
                    .all(|(key, value)| labels.get(key) == Some(value))
                {
                    return Err(format!(
                        "deployment {} selector does not match its pod labels",
                        resource.metadata.name
                    ));
                }
                let container = sequence(at(&resource.spec, &["template", "spec", "containers"])?)?
                    .first()
                    .ok_or_else(|| {
                        format!("deployment {} has no container", resource.metadata.name)
                    })?;
                let image = scalar(at(container, &["image"])?)?.to_owned();
                let readiness_path =
                    scalar(at(container, &["readinessProbe", "httpGet", "path"])?)?.to_owned();
                let environment = sequence(at(container, &["env"])?)?
                    .iter()
                    .map(|entry| {
                        Ok((
                            scalar(at(entry, &["name"])?)?.to_owned(),
                            scalar(at(entry, &["value"])?)?.to_owned(),
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>, String>>()?;
                let roles = environment
                    .get("WYRD_ROLES")
                    .ok_or_else(|| {
                        format!("deployment {} omits WYRD_ROLES", resource.metadata.name)
                    })?
                    .split(',')
                    .map(str::to_owned)
                    .collect();
                let volume_mounts = at(container, &["volumeMounts"])
                    .ok()
                    .map(sequence)
                    .transpose()?
                    .into_iter()
                    .flatten()
                    .map(|entry| {
                        Ok((
                            scalar(at(entry, &["name"])?)?.to_owned(),
                            scalar(at(entry, &["mountPath"])?)?.to_owned(),
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>, String>>()?;
                let empty_dir_volumes = at(&resource.spec, &["template", "spec", "volumes"])
                    .ok()
                    .map(sequence)
                    .transpose()?
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| {
                        at(entry, &["emptyDir"])
                            .ok()
                            .and_then(|_| at(entry, &["name"]).ok())
                            .and_then(|name| scalar(name).ok())
                            .map(str::to_owned)
                    })
                    .collect();
                facts.deployments.insert(
                    resource.metadata.name,
                    DeploymentFacts {
                        labels,
                        roles,
                        image,
                        readiness_path,
                        environment,
                        volume_mounts,
                        empty_dir_volumes,
                    },
                );
            }
            "Service" => {
                facts.services.insert(
                    resource.metadata.name,
                    string_map(at(&resource.spec, &["selector"])?)?,
                );
            }
            "HTTPRoute" => {
                let rule = sequence(at(&resource.spec, &["rules"])?)?
                    .first()
                    .ok_or_else(|| "HTTPRoute has no rule".to_owned())?;
                let path = scalar(at(
                    sequence(at(rule, &["matches"])?)?
                        .first()
                        .ok_or_else(|| "HTTPRoute has no match".to_owned())?,
                    &["path", "value"],
                )?)?;
                let backend = scalar(at(
                    sequence(at(rule, &["backendRefs"])?)?
                        .first()
                        .ok_or_else(|| "HTTPRoute has no backend".to_owned())?,
                    &["name"],
                )?)?;
                facts
                    .http_routes
                    .push((path.to_owned(), backend.to_owned()));
            }
            "GRPCRoute" => {
                let rule = sequence(at(&resource.spec, &["rules"])?)?
                    .first()
                    .ok_or_else(|| "GRPCRoute has no rule".to_owned())?;
                let service = scalar(at(
                    sequence(at(rule, &["matches"])?)?
                        .first()
                        .ok_or_else(|| "GRPCRoute has no match".to_owned())?,
                    &["method", "service"],
                )?)?;
                let backend = scalar(at(
                    sequence(at(rule, &["backendRefs"])?)?
                        .first()
                        .ok_or_else(|| "GRPCRoute has no backend".to_owned())?,
                    &["name"],
                )?)?;
                facts
                    .grpc_routes
                    .push((service.to_owned(), backend.to_owned()));
            }
            _ => {}
        }
    }
    Ok(facts)
}

/// Resolve a nested mapping path from semantic YAML.
///
/// # Errors
///
/// Returns an error when a path component is absent or not a mapping.
fn at<'a>(value: &'a serde_yaml::Value, path: &[&str]) -> Result<&'a serde_yaml::Value, String> {
    path.iter().try_fold(value, |current, component| {
        current
            .as_mapping()
            .and_then(|mapping| mapping.get(serde_yaml::Value::String((*component).to_owned())))
            .ok_or_else(|| format!("missing semantic YAML path {}", path.join(".")))
    })
}

/// Project a YAML mapping whose keys and values must both be strings.
///
/// # Errors
///
/// Returns an error for non-mapping input or non-string entries.
fn string_map(value: &serde_yaml::Value) -> Result<BTreeMap<String, String>, String> {
    value
        .as_mapping()
        .ok_or_else(|| "expected YAML mapping".to_owned())?
        .iter()
        .map(|(key, value)| Ok((scalar(key)?.to_owned(), scalar(value)?.to_owned())))
        .collect()
}

/// Require a YAML sequence.
///
/// # Errors
///
/// Returns an error when the value is not a sequence.
fn sequence(value: &serde_yaml::Value) -> Result<&Vec<serde_yaml::Value>, String> {
    value
        .as_sequence()
        .ok_or_else(|| "expected YAML sequence".to_owned())
}

/// Require a YAML string scalar.
///
/// # Errors
///
/// Returns an error when the value is not a string.
fn scalar(value: &serde_yaml::Value) -> Result<&str, String> {
    value
        .as_str()
        .ok_or_else(|| "expected YAML string".to_owned())
}

/// Validate a selector by proving it targets at least one deployment with the required role.
///
/// # Errors
///
/// Returns an error when the service is absent, selects no deployment, or can
/// select a deployment without the required role.
fn validate_service_role(
    facts: &ManifestFacts,
    service: &str,
    required_role: &str,
) -> Result<(), String> {
    let selector = facts
        .services
        .get(service)
        .ok_or_else(|| format!("missing service {service}"))?;
    let targets = facts
        .deployments
        .values()
        .filter(|deployment| {
            selector
                .iter()
                .all(|(key, value)| deployment.labels.get(key) == Some(value))
        })
        .collect::<Vec<_>>();
    if targets.is_empty()
        || targets
            .iter()
            .any(|deployment| !deployment.roles.contains(required_role))
    {
        return Err(format!(
            "service {service} does not exclusively select {required_role} deployments"
        ));
    }
    Ok(())
}

/// Validate the candidate mixed/role-separated routing and readiness contract.
///
/// # Errors
///
/// Returns an error for selector, route, readiness, or role mismatches.
fn validate_candidate(facts: &ManifestFacts, require_routes: bool) -> Result<(), String> {
    if facts.deployments.is_empty()
        || facts
            .deployments
            .values()
            .any(|deployment| deployment.readiness_path != "/readyz")
    {
        return Err("every deployment must use /readyz".to_owned());
    }
    if require_routes {
        validate_service_role(facts, "wyrd-ingest", "scribe")?;
        validate_service_role(facts, "wyrd-query", "oracle")?;
        validate_service_role(facts, "wyrd-bifrost-private", "oracle")?;
        if !facts
            .http_routes
            .contains(&("/v1/query".to_owned(), "wyrd-query".to_owned()))
        {
            return Err("query HTTPRoute must target wyrd-query at /v1/query".to_owned());
        }
        if !facts.grpc_routes.contains(&(
            "wyrd.v1.BifrostIngestService".to_owned(),
            "wyrd-ingest".to_owned(),
        )) {
            return Err("ingest GRPCRoute must target wyrd-ingest".to_owned());
        }
    } else {
        let has_scribe = facts
            .deployments
            .values()
            .any(|deployment| deployment.roles.contains("scribe"));
        let has_oracle = facts
            .deployments
            .values()
            .any(|deployment| deployment.roles.contains("oracle"));
        if !has_scribe || !has_oracle {
            return Err(
                "role-separated manifest must contain Scribe and Oracle deployments".to_owned(),
            );
        }
        for deployment in facts
            .deployments
            .values()
            .filter(|deployment| deployment.roles.contains("oracle"))
        {
            let expected = "/var/lib/wyrd/bifrost";
            if deployment
                .environment
                .get("WYRD_SCRIBE_WAL_DIR")
                .map(String::as_str)
                != Some(expected)
                || deployment
                    .volume_mounts
                    .get("bifrost-data")
                    .map(String::as_str)
                    != Some(expected)
                || !deployment.empty_dir_volumes.contains("bifrost-data")
            {
                return Err(
                    "role-separated Oracle requires bifrost-data emptyDir mounted at WYRD_SCRIBE_WAL_DIR"
                        .to_owned(),
                );
            }
        }
    }
    Ok(())
}

/// Validate that rollback restores one stable mixed-role deployment.
///
/// # Errors
///
/// Returns an error when rollback does not restore Scribe, Forge, and Oracle on
/// a stable image with the production readiness path.
fn validate_rollback(facts: &ManifestFacts) -> Result<(), String> {
    if facts.deployments.len() != 1 {
        return Err("rollback must contain exactly one deployment".to_owned());
    }
    let deployment = facts
        .deployments
        .values()
        .next()
        .ok_or_else(|| "rollback deployment missing".to_owned())?;
    let required = BTreeSet::from(["scribe".to_owned(), "forge".to_owned(), "oracle".to_owned()]);
    if deployment.roles != required
        || deployment.image != "wyrd/server:stable"
        || deployment.readiness_path != "/readyz"
    {
        return Err(format!(
            "rollback must restore the stable mixed-role deployment: {deployment:?}"
        ));
    }
    Ok(())
}

/// Resolve the checked-in Bifrost deployment directory.
fn manifest_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("deploy/kubernetes/bifrost")
}

/// Read one checked-in deployment manifest.
///
/// # Errors
///
/// Returns an error when the manifest cannot be read.
fn read_manifest(name: &str) -> Result<String, String> {
    std::fs::read_to_string(manifest_directory().join(name)).map_err(|error| error.to_string())
}

/// Checked-in manifests satisfy routing, selector, readiness, and rollback contracts.
#[test]
fn bifrost_deployment_contract_accepts_checked_in_manifests() -> Result<(), String> {
    validate_candidate(
        &parse_manifest(&read_manifest("deployment-mixed.yaml")?)?,
        true,
    )?;
    validate_candidate(
        &parse_manifest(&read_manifest("deployment-role-separated.yaml")?)?,
        false,
    )?;
    validate_rollback(&parse_manifest(&read_manifest("rollback-mixed.yaml")?)?)
}

/// Semantic mutations prove selector, route, readiness, and rollback failures are detected.
#[test]
fn bifrost_deployment_contract_rejects_broken_fixtures() -> Result<(), String> {
    let mixed = read_manifest("deployment-mixed.yaml")?;
    let separated = read_manifest("deployment-role-separated.yaml")?;
    let rollback = read_manifest("rollback-mixed.yaml")?;

    let bad_selector = mixed.replacen(
        "matchLabels:\n      app: wyrd-bifrost",
        "matchLabels:\n      app: wrong-bifrost",
        1,
    );
    assert!(parse_manifest(&bad_selector).is_err());

    let bad_route = mixed.replacen("value: /v1/query", "value: /wrong/query", 1);
    assert!(validate_candidate(&parse_manifest(&bad_route)?, true).is_err());

    let bad_readiness = separated.replacen("path: /readyz", "path: /wrong-ready", 1);
    assert!(validate_candidate(&parse_manifest(&bad_readiness)?, false).is_err());

    let missing_oracle_mount = separated.replacen(
        "          volumeMounts:\n            - {name: bifrost-data, mountPath: /var/lib/wyrd/bifrost}\n",
        "",
        1,
    );
    assert!(validate_candidate(&parse_manifest(&missing_oracle_mount)?, false).is_err());

    let mismatched_oracle_path = separated.replacen(
        "{name: WYRD_SCRIBE_WAL_DIR, value: /var/lib/wyrd/bifrost}",
        "{name: WYRD_SCRIBE_WAL_DIR, value: /var/lib/wyrd/wrong}",
        1,
    );
    assert!(validate_candidate(&parse_manifest(&mismatched_oracle_path)?, false).is_err());

    let bad_rollback = rollback.replacen("wyrd/server:stable", "wyrd/server:latest", 1);
    assert!(validate_rollback(&parse_manifest(&bad_rollback)?).is_err());
    Ok(())
}
