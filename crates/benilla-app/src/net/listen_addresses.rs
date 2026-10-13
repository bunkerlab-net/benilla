//! Local interface addresses for a configured listener, not a reachability or NAT test.
use std::collections::BTreeSet;
use std::net::{IpAddr, SocketAddr, SocketAddrV6};

/// Read local interface metadata only. Call when building the login display, not every frame.
/// A wildcard expands to up interfaces of the same address family, including loopback and VPNs.
/// No sockets, router mappings, remote requests or connectivity checks are performed.
pub(crate) fn discover(listen: SocketAddr) -> Vec<SocketAddr> {
    if !listen.ip().is_unspecified() {
        return vec![listen];
    }
    let mut candidates = Vec::new();
    for interface in netdev::get_interfaces().into_iter().filter(|i| i.is_up()) {
        for address in interface.ipv4 {
            candidates.push(SocketAddr::new(IpAddr::V4(address.addr()), listen.port()));
        }
        for (index, address) in interface.ipv6.iter().enumerate() {
            let ip = address.addr();
            let scope = if ip.is_unicast_link_local() {
                interface
                    .ipv6_scope_ids
                    .get(index)
                    .copied()
                    .unwrap_or(interface.index)
            } else {
                0
            };
            candidates.push(SocketAddr::V6(SocketAddrV6::new(
                ip,
                listen.port(),
                0,
                scope,
            )));
        }
    }
    select(listen, candidates)
}

fn select(listen: SocketAddr, candidates: impl IntoIterator<Item = SocketAddr>) -> Vec<SocketAddr> {
    if !listen.ip().is_unspecified() {
        return vec![listen];
    }
    candidates
        .into_iter()
        .filter(|address| {
            address.is_ipv4() == listen.is_ipv4()
                && !address.ip().is_unspecified()
                && !address.ip().is_multicast()
                && !matches!(address.ip(), IpAddr::V4(ip) if ip.is_broadcast())
        })
        .map(|mut address| {
            address.set_port(listen.port());
            address
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wildcard_ipv4_includes_loopback_lan_and_vpn_with_configured_port() {
        let input = [
            "127.0.0.1:0",
            "192.168.1.4:1",
            "100.64.2.3:2",
            "192.168.1.4:3",
            "[::1]:1",
        ];
        let values = select(
            "0.0.0.0:7475".parse().unwrap(),
            input.map(|s| s.parse().unwrap()),
        );
        assert_eq!(
            values.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["100.64.2.3:7475", "127.0.0.1:7475", "192.168.1.4:7475"]
        );
    }
    #[test]
    fn explicit_bind_is_not_replaced_by_unrelated_interfaces() {
        let listen = "127.0.0.1:7474".parse().unwrap();
        assert_eq!(
            select(listen, ["192.168.1.4:7".parse().unwrap()]),
            vec![listen]
        );
    }
    #[test]
    fn invalid_candidates_are_not_advertised() {
        let input = ["0.0.0.0:1", "224.0.0.1:1", "255.255.255.255:1"];
        assert!(select(
            "0.0.0.0:7474".parse().unwrap(),
            input.map(|s| s.parse().unwrap())
        )
        .is_empty());
    }
    #[test]
    fn ipv6_keeps_scope_and_does_not_claim_ipv4_dual_stack() {
        let input = ["[fe80::1%3]:1", "[::1]:1", "127.0.0.1:1"];
        let values = select(
            "[::]:7474".parse().unwrap(),
            input.map(|s| s.parse().unwrap()),
        );
        assert!(values.contains(&"[fe80::1%3]:7474".parse().unwrap()));
        assert!(values.contains(&"[::1]:7474".parse().unwrap()));
        assert_eq!(values.len(), 2);
    }
}
