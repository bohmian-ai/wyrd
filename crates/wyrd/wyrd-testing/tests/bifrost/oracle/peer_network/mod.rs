//! Multi-process journeys over the private Bifrost peer plane.
//!
//! Every test here boots a [`crate::peer_cluster::PeerCluster`] of real
//! `wyrd-server` pods, each with two listeners in two trust zones under one
//! lifecycle, mutual TLS on the wire, one peer identity per pod, and
//! membership addresses another pod can actually dial.

mod analytical;
mod join;
mod listener;
mod security;
mod support;
mod transport;
