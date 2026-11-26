//! Kusema link reading: only the public internet. A link the user asks Kusema
//! to read must not reach this computer, the office network or a cloud
//! metadata address, whatever the address says, whatever its name resolves
//! to and wherever it redirects. Checked at three points:
//! - the address itself (scheme, a literal IP) before anything is sent;
//! - every redirect, the same way;
//! - every name, when it's resolved for the connection (`PublicResolver`), so
//!   a name can't pass a check and then resolve somewhere else.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};

/// Tests only: names under this domain stand in for public sites and resolve
/// to the test's own server on loopback. Not compiled into the app.
#[cfg(test)]
pub const TEST_DOMAIN: &str = "kusema-test.invalid";

/// Why a link can't be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotPublic {
    /// Not http or https (file:, ftp:, javascript: and the rest).
    Scheme(String),
    /// User name or password in the address.
    Credentials,
    /// Resolves to, or is, an address that isn't on the public internet.
    PrivateAddress(String),
    /// Not an address at all.
    Invalid,
}

impl std::fmt::Display for NotPublic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotPublic::Scheme(scheme) => {
                write!(f, "Kusema only reads web pages (http or https), not {scheme}: links.")
            }
            NotPublic::Credentials => {
                write!(f, "Kusema doesn't open links with a user name or password in them.")
            }
            NotPublic::PrivateAddress(host) => write!(
                f,
                "{host} is on this computer or a private network. Kusema only reads public web pages."
            ),
            NotPublic::Invalid => write!(f, "That isn't a web address Kusema can read."),
        }
    }
}

impl std::error::Error for NotPublic {}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        // Shared address space (carrier-grade NAT), 100.64.0.0/10.
        || (a == 100 && (b & 0xc0) == 64)
        // IETF protocol assignments, 192.0.0.0/24.
        || (a == 192 && b == 0 && c == 0)
        // Benchmarking, 198.18.0.0/15.
        || (a == 198 && (b & 0xfe) == 18)
        // Reserved, 240.0.0.0/4.
        || a >= 240)
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    let first = ip.segments()[0];
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        // Unique local, fc00::/7.
        || (first & 0xfe00) == 0xfc00
        // Link-local, fe80::/10.
        || (first & 0xffc0) == 0xfe80
        // Site-local (deprecated), fec0::/10.
        || (first & 0xffc0) == 0xfec0
        // Documentation, 2001:db8::/32.
        || (first == 0x2001 && ip.segments()[1] == 0x0db8)
        // IPv4-compatible (deprecated) ::a.b.c.d and NAT64 64:ff9b::/96 carry
        // an IPv4 address that could be private.
        || ip.segments()[..6].iter().all(|&s| s == 0)
        || (first == 0x0064 && ip.segments()[1] == 0xff9b))
}

/// Whether `ip` is on the public internet.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

/// Checks an address before it's requested (and each redirect): http or
/// https, no credentials, and a literal IP must be public. Names are checked
/// when they're resolved.
pub fn check_url(url: &reqwest::Url) -> Result<(), NotPublic> {
    match url.scheme() {
        "http" | "https" => {}
        other => return Err(NotPublic::Scheme(other.to_string())),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(NotPublic::Credentials);
    }
    let Some(host) = url.host_str().filter(|h| !h.is_empty()) else {
        return Err(NotPublic::Invalid);
    };
    // The URL parser has already turned "2130706433" and "0x7f.1" into
    // dotted IPv4, and keeps IPv6 in brackets.
    let literal = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = literal.parse::<IpAddr>() {
        return if is_public(ip) {
            Ok(())
        } else {
            Err(NotPublic::PrivateAddress(ip.to_string()))
        };
    }
    let name = host.trim_end_matches('.').to_ascii_lowercase();
    #[cfg(test)]
    if name.ends_with(TEST_DOMAIN) {
        return Ok(());
    }
    if name == "localhost" || name.ends_with(".localhost") || name.ends_with(".local") {
        Err(NotPublic::PrivateAddress(name))
    } else {
        Ok(())
    }
}

/// The addresses `host` resolves to, refused if any isn't public (a name
/// with one private address among public ones is refused too: the connection
/// could use either).
pub fn resolve_public(host: &str) -> Result<Vec<SocketAddr>, NotPublic> {
    #[cfg(test)]
    if host.ends_with(TEST_DOMAIN) {
        return Ok(vec![SocketAddr::from(([127, 0, 0, 1], 0))]);
    }
    let addrs: Vec<SocketAddr> = (host, 0)
        .to_socket_addrs()
        .map_err(|_| NotPublic::Invalid)?
        .collect();
    if addrs.is_empty() {
        return Err(NotPublic::Invalid);
    }
    if addrs.iter().any(|addr| !is_public(addr.ip())) {
        return Err(NotPublic::PrivateAddress(host.to_string()));
    }
    Ok(addrs)
}

/// Resolves names for link reading, refusing any that lead off the public
/// internet. Runs at connection time, after redirects, for every name.
pub struct PublicResolver;

impl reqwest::dns::Resolve for PublicResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let found = tokio::task::spawn_blocking(move || resolve_public(&host))
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
            match found {
                Ok(addrs) => Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs),
                Err(e) => Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
            }
        })
    }
}

/// The `NotPublic` inside a reqwest error, if that's why it failed.
pub fn not_public_in(error: &(dyn std::error::Error + 'static)) -> Option<NotPublic> {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(current) = source {
        if let Some(found) = current.downcast_ref::<NotPublic>() {
            return Some(found.clone());
        }
        source = current.source();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> reqwest::Url {
        reqwest::Url::parse(s).unwrap()
    }

    #[test]
    fn private_and_special_addresses_are_not_public() {
        for ip in [
            "127.0.0.1",
            "127.9.9.9",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.254",
            "192.168.0.10",
            "169.254.169.254",
            "100.64.0.1",
            "100.127.255.255",
            "0.0.0.0",
            "0.1.2.3",
            "255.255.255.255",
            "224.0.0.1",
            "192.0.0.8",
            "192.0.2.1",
            "198.18.0.1",
            "240.0.0.1",
            "::",
            "::1",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "fec0::1",
            "ff02::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
            "::127.0.0.1",
            "64:ff9b::a00:1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn public_addresses_are_public() {
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "93.184.215.14",
            "172.32.0.1",
            "100.128.0.1",
            "2606:4700::1111",
            "2a00:1450:4001::200e",
            "::ffff:8.8.8.8",
        ] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn only_http_and_https_links_without_credentials() {
        assert!(check_url(&url("https://example.com/page")).is_ok());
        assert!(check_url(&url("http://example.com:8080/x")).is_ok());
        for bad in [
            "file:///C:/Windows/win.ini",
            "ftp://example.com/file",
            "javascript:alert(1)",
            "data:text/html,hi",
            "gopher://example.com",
        ] {
            assert!(
                matches!(check_url(&url(bad)), Err(NotPublic::Scheme(_))),
                "{bad}"
            );
        }
        assert_eq!(
            check_url(&url("https://user:pass@example.com/")),
            Err(NotPublic::Credentials)
        );
    }

    #[test]
    fn local_and_private_hosts_are_refused_before_anything_is_sent() {
        for bad in [
            "http://localhost/",
            "http://LOCALHOST:11434/api",
            "http://app.localhost/",
            "http://printer.local/",
            "http://127.0.0.1/",
            "http://[::1]:8080/",
            "http://10.0.0.5/admin",
            "http://192.168.1.1/",
            "http://169.254.169.254/latest/meta-data/",
            "http://[fe80::1]/",
            "http://[::ffff:192.168.1.1]/",
            "http://0.0.0.0:8000/",
            // Other spellings of 127.0.0.1 are normalized by the URL parser.
            "http://2130706433/",
            "http://0x7f.0.0.1/",
            "http://127.1/",
        ] {
            assert!(
                matches!(check_url(&url(bad)), Err(NotPublic::PrivateAddress(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn names_that_resolve_locally_are_refused() {
        // "localhost" resolves on every machine, without a network.
        assert!(matches!(
            resolve_public("localhost"),
            Err(NotPublic::PrivateAddress(_))
        ));
        assert!(matches!(
            resolve_public("127.0.0.1"),
            Err(NotPublic::PrivateAddress(_))
        ));
    }
}
