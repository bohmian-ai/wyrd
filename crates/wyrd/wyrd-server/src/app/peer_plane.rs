//! Retained status of this process's private Bifrost peer listener.
//!
//! The peer plane is the only way a Bifrost node is reachable east-west: an
//! Oracle resolves Scribe tail routes across it, a coordinator sends Analytical
//! stage operations across it, and lifecycle fanout uses it. A node that serves
//! the public listener while its private listener is absent looks healthy to a
//! load balancer and is silently useless to its peers.
//!
//! So readiness observes the peer plane the same way it observes the Scribe and
//! Oracle roles: a retained bit published by the owner that actually runs the
//! listener, read without touching a socket.

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether this process must run a peer listener, and whether one is running.
///
/// The two bits are separate on purpose. `required` follows the selected
/// target and is set during composition; `serving` follows the supervised task
/// and is set when the listener starts and cleared when it stops. Readiness
/// fails only when the target requires a peer plane and none is serving, so a
/// Forge worker — which correctly opens no peer socket — stays ready.
#[derive(Debug, Default)]
pub struct PeerPlaneStatus {
    /// Set when the selected target must open the private peer listener.
    required: AtomicBool,
    /// Set for as long as the supervised peer listener task is serving.
    serving: AtomicBool,
}

impl PeerPlaneStatus {
    /// Declares that this process's target must serve the peer plane.
    pub fn require(&self) {
        self.required.store(true, Ordering::Release);
    }

    /// Publishes that the supervised peer listener has begun serving.
    pub fn mark_serving(&self) {
        self.serving.store(true, Ordering::Release);
    }

    /// Publishes that the supervised peer listener has stopped serving.
    pub fn mark_stopped(&self) {
        self.serving.store(false, Ordering::Release);
    }

    /// Reports whether this target must serve the peer plane.
    #[must_use]
    pub fn is_required(&self) -> bool {
        self.required.load(Ordering::Acquire)
    }

    /// Reports whether a peer listener is currently serving.
    #[must_use]
    pub fn is_serving(&self) -> bool {
        self.serving.load(Ordering::Acquire)
    }

    /// Reports whether the peer plane satisfies this target's requirement.
    ///
    /// A target that requires no peer plane is always satisfied; one that does
    /// is satisfied only while a listener is actually serving.
    #[must_use]
    pub fn is_satisfied(&self) -> bool {
        !self.is_required() || self.is_serving()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A target that opens no peer socket is not held back by the peer plane.
    #[test]
    fn an_unrequired_peer_plane_is_satisfied_while_idle() {
        let status = PeerPlaneStatus::default();
        assert!(status.is_satisfied());
    }

    /// A peer-bearing target is unsatisfied until its listener actually serves.
    #[test]
    fn a_required_peer_plane_is_unsatisfied_until_it_serves() {
        let status = PeerPlaneStatus::default();
        status.require();
        assert!(!status.is_satisfied());
        status.mark_serving();
        assert!(status.is_satisfied());
    }

    /// A peer listener that stops takes the node out of readiness again.
    #[test]
    fn a_stopped_peer_listener_withdraws_readiness() {
        let status = PeerPlaneStatus::default();
        status.require();
        status.mark_serving();
        status.mark_stopped();
        assert!(!status.is_satisfied());
    }
}
