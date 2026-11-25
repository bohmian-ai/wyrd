//! Keys of the shared per-source admission windows.
//!
//! Every per-source window (password sign-in, signup, mail requests) keys a
//! request's trusted source the same way: an IPv4 address as itself, an IPv6
//! address as its `/64` prefix, because one subscriber is routinely handed a
//! whole `/64` to rotate through.

use std::net::{IpAddr, Ipv6Addr};

/// The admission-window key of the trusted `source`.
#[must_use]
pub fn source_key(source: IpAddr) -> String {
    match source.to_canonical() {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => {
            let prefix = u128::from(v6) & (u128::MAX << 64);
            format!("{}/64", Ipv6Addr::from(prefix))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::source_key;

    /// IPv4 keys as itself, mapped IPv4 as its IPv4, IPv6 as its `/64`.
    #[test]
    fn ipv6_sources_share_their_64() {
        let key = |value: &str| source_key(value.parse().expect("address"));
        assert_eq!(key("198.51.100.1"), "198.51.100.1");
        assert_eq!(key("::ffff:198.51.100.1"), "198.51.100.1");
        assert_eq!(key("2001:db8:1:2::1"), "2001:db8:1:2::/64");
        assert_eq!(key("2001:db8:1:2:ffff::9"), "2001:db8:1:2::/64");
        assert_ne!(key("2001:db8:1:3::1"), key("2001:db8:1:2::1"));
    }
}
