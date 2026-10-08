//! Where the AI endpoint listens: this computer only (the default), the Docker bridge
//! so that containers can reach it, or an address the user types. The token is needed
//! in every case.

use std::net::{IpAddr, Ipv4Addr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Listen {
    /// 127.0.0.1: only programs on this computer.
    #[default]
    Local,
    /// The Docker bridge, for AI clients running in containers.
    Docker,
    /// An address of the user's choice.
    Custom(String),
}

/// How a [`Listen`] choice turns into a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The address the server binds.
    pub bind: IpAddr,
    /// Host header values the server accepts (any port); every other one is refused,
    /// against DNS rebinding.
    pub hosts: Vec<String>,
    /// The host clients put into the URL.
    pub client_host: String,
}

/// What containers call the computer they run on.
pub const DOCKER_HOST: &str = "host.docker.internal";

const LOOPBACK_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "::1"];

impl Endpoint {
    pub fn local() -> Self {
        Endpoint {
            bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
            hosts: LOOPBACK_HOSTS.iter().map(|h| (*h).to_owned()).collect(),
            client_host: "127.0.0.1".into(),
        }
    }
}

impl Listen {
    /// What to bind and accept, or why that is not possible here.
    pub fn endpoint(&self) -> Result<Endpoint, String> {
        match self {
            Listen::Local => Ok(Endpoint::local()),
            Listen::Docker => docker(docker_bridge()),
            Listen::Custom(text) => {
                let ip: IpAddr = text
                    .trim()
                    .parse()
                    .map_err(|_| format!("\"{}\" is not an IP address.", text.trim()))?;
                let mut hosts = vec![ip.to_string(), DOCKER_HOST.to_owned()];
                if ip.is_unspecified() {
                    // Every address of this computer; the Host check still decides.
                    hosts.extend(LOOPBACK_HOSTS.iter().map(|h| (*h).to_owned()));
                }
                Ok(Endpoint {
                    bind: ip,
                    hosts,
                    client_host: ip.to_string(),
                })
            }
        }
    }
}

/// On Linux containers reach the computer through the bridge (`docker0`); Docker
/// Desktop on macOS and Windows forwards `host.docker.internal` to 127.0.0.1.
fn docker(bridge: Option<Ipv4Addr>) -> Result<Endpoint, String> {
    let ip = if cfg!(target_os = "linux") {
        bridge.ok_or("No Docker bridge (docker0) found. Is Docker running?")?
    } else {
        Ipv4Addr::LOCALHOST
    };
    Ok(Endpoint {
        bind: IpAddr::V4(ip),
        hosts: vec![ip.to_string(), DOCKER_HOST.to_owned()],
        client_host: DOCKER_HOST.into(),
    })
}

/// This computer's address on the Docker bridge, read from `/proc` (Linux only).
fn docker_bridge() -> Option<Ipv4Addr> {
    let route = std::fs::read_to_string("/proc/net/route").ok()?;
    let trie = std::fs::read_to_string("/proc/net/fib_trie").ok()?;
    bridge_address(&route, &trie, "docker0")
}

/// The local address that lies in the network routed through `iface`. `route` is
/// `/proc/net/route` (little-endian hex), `trie` is `/proc/net/fib_trie`.
pub fn bridge_address(route: &str, trie: &str, iface: &str) -> Option<Ipv4Addr> {
    let hex = |s: &str| u32::from_str_radix(s, 16).ok().map(u32::from_be);
    // The interface's own network, not a default route through it (mask 0).
    let (net, mask) = route.lines().skip(1).find_map(|line| {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.first() != Some(&iface) {
            return None;
        }
        let (net, mask) = (hex(f.get(1)?)?, hex(f.get(7)?)?);
        (mask != 0).then_some((net, mask))
    })?;
    let mut last: Option<Ipv4Addr> = None;
    for line in trie.lines() {
        let t = line.trim_start();
        if let Some(addr) = t.strip_prefix("|-- ") {
            last = addr.trim().parse().ok();
        } else if t.contains("host LOCAL")
            && let Some(ip) = last
            && u32::from(ip) & mask == net
        {
            return Some(ip);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUTE: &str = "\
Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT
wlan0\t00000000\t0102A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0
docker0\t000011AC\t00000000\t0001\t0\t0\t0\t0000FFFF\t0\t0\t0
wlan0\t0002A8C0\t00000000\t0001\t0\t0\t600\t00FFFFFF\t0\t0\t0
";

    const TRIE: &str = "\
Main:
  +-- 0.0.0.0/0 3 0 5
     |-- 0.0.0.0
        /0 universe UNICAST
     +-- 172.17.0.0/16 2 0 2
        +-- 172.17.0.0/30 2 0 2
           |-- 172.17.0.0
              /16 link UNICAST
           |-- 172.17.0.1
              /32 host LOCAL
        |-- 172.17.255.255
           /32 link BROADCAST
     +-- 192.168.2.0/24 2 0 2
        |-- 192.168.2.40
           /32 host LOCAL
";

    #[test]
    fn the_bridge_address_comes_from_route_and_trie() {
        assert_eq!(
            bridge_address(ROUTE, TRIE, "docker0"),
            Some(Ipv4Addr::new(172, 17, 0, 1))
        );
        assert_eq!(
            bridge_address(ROUTE, TRIE, "wlan0"),
            Some(Ipv4Addr::new(192, 168, 2, 40))
        );
        assert_eq!(bridge_address(ROUTE, TRIE, "br-missing"), None);
    }

    #[test]
    fn each_choice_binds_and_accepts_its_own_hosts() {
        let local = Listen::Local.endpoint().unwrap();
        assert_eq!(local.bind, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert!(!local.hosts.iter().any(|h| h == DOCKER_HOST));

        let bridge = docker(Some(Ipv4Addr::new(172, 17, 0, 1))).unwrap();
        assert_eq!(bridge.client_host, DOCKER_HOST);
        assert!(bridge.hosts.iter().any(|h| h == DOCKER_HOST));
        if cfg!(target_os = "linux") {
            assert_eq!(bridge.bind, IpAddr::V4(Ipv4Addr::new(172, 17, 0, 1)));
            assert!(docker(None).unwrap_err().contains("docker0"));
        }

        let custom = Listen::Custom(" 10.0.0.5 ".into()).endpoint().unwrap();
        assert_eq!(custom.bind.to_string(), "10.0.0.5");
        assert_eq!(custom.client_host, "10.0.0.5");
        assert!(
            Listen::Custom("my-laptop".into())
                .endpoint()
                .unwrap_err()
                .contains("not an IP address")
        );
    }
}
