//! Multi-process journeys over the private Bifrost peer plane.
//!
//! Every test here boots real `wyrd-server` child processes through
//! [`wyrd_testing::bifrost::process_cluster::BifrostProcessCluster`] rather
//! than composing a server in-process, because the claims under test are
//! exactly the ones an in-process fixture cannot make: two listeners in two
//! trust zones under one lifecycle, mutual TLS on the wire, one peer identity
//! per pod, and membership addresses another pod can actually dial.

mod analytical;
mod listener;
mod security;
mod support;
mod transport;
