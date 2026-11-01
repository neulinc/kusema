//! Which destinations a request may reach. Pure, so every case can be tested.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Where a host is, as far as can be known without asking DNS (whose answer
/// could change between the check and the request).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// This computer.
    Loopback,
    /// A private address on the local network.
    Lan,
    /// Anywhere else, including any name that can't be shown to be local.
    Public,
}

fn is_loopback_v4(ip: Ipv4Addr) -> bool {
    ip.is_loopback()
}

fn is_lan_v4(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_link_local()
}

fn is_lan_v6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    // Unique local (fc00::/7) and link-local (fe80::/10).
    (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
}

/// Classifies a URL's host (`url.host_str()`; IPv6 may keep its brackets).
pub fn classify(host: &str) -> Reach {
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host == "localhost" {
        return Reach::Loopback;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) if is_loopback_v4(ip) => Reach::Loopback,
        Ok(IpAddr::V4(ip)) if is_lan_v4(ip) => Reach::Lan,
        Ok(IpAddr::V6(ip)) => match ip.to_ipv4_mapped() {
            Some(v4) if is_loopback_v4(v4) => Reach::Loopback,
            Some(v4) if is_lan_v4(v4) => Reach::Lan,
            Some(_) => Reach::Public,
            None if ip.is_loopback() => Reach::Loopback,
            None if is_lan_v6(ip) => Reach::Lan,
            None => Reach::Public,
        },
        Ok(IpAddr::V4(_)) => Reach::Public,
        // mDNS names only ever resolve on the local network. Any other name
        // could resolve anywhere ("localhost.example.com", "ollama"), so it
        // isn't trusted: Privileged mode needs an address it can check.
        Err(_) if host.ends_with(".local") => Reach::Lan,
        Err(_) => Reach::Public,
    }
}

/// The settings that decide what's allowed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rules {
    pub privileged: bool,
    pub allow_lan: bool,
    pub block_downloads: bool,
}

/// What a request carries, for the rules' purposes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// Goes to an AI provider, and may carry the user's words or screen.
    Ai,
    /// A model, voice or update: no user content.
    Download,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    BlockCloudAi,
    BlockLanAi,
    BlockDownload,
}

/// The rule itself. Outside Privileged mode, everything is allowed (and logged).
pub fn decide(rules: Rules, category: Category, reach: Reach) -> Verdict {
    if !rules.privileged {
        return Verdict::Allow;
    }
    match (category, reach) {
        (_, Reach::Loopback) => Verdict::Allow,
        (Category::Ai, Reach::Lan) if rules.allow_lan => Verdict::Allow,
        (Category::Ai, Reach::Lan) => Verdict::BlockLanAi,
        (Category::Ai, Reach::Public) => Verdict::BlockCloudAi,
        (Category::Download, _) if rules.block_downloads => Verdict::BlockDownload,
        (Category::Download, _) => Verdict::Allow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OFF: Rules = Rules {
        privileged: false,
        allow_lan: false,
        block_downloads: false,
    };
    const ON: Rules = Rules {
        privileged: true,
        allow_lan: false,
        block_downloads: false,
    };

    #[test]
    fn loopback_hosts() {
        for host in [
            "localhost",
            "LOCALHOST",
            "localhost.",
            "127.0.0.1",
            "127.1.2.3",
            "::1",
            "[::1]",
            "[::ffff:127.0.0.1]",
        ] {
            assert_eq!(classify(host), Reach::Loopback, "{host}");
        }
    }

    #[test]
    fn lan_hosts() {
        for host in [
            "10.0.0.5",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.20",
            "169.254.1.1",
            "[fd12::1]",
            "[fe80::1]",
            "[::ffff:192.168.1.20]",
            "ollama.local",
        ] {
            assert_eq!(classify(host), Reach::Lan, "{host}");
        }
    }

    #[test]
    fn public_hosts() {
        for host in [
            "api.openai.com",
            "api.anthropic.com",
            "8.8.8.8",
            "172.32.0.1",
            "[2606:4700::1111]",
            "[::ffff:8.8.8.8]",
            "0.0.0.0",
        ] {
            assert_eq!(classify(host), Reach::Public, "{host}");
        }
    }

    #[test]
    fn names_that_only_look_local_are_public() {
        for host in [
            "localhost.evil.example",
            "127.0.0.1.nip.io",
            "local",
            "ollama",
            "192.168.1.1.example.com",
            "",
        ] {
            assert_eq!(classify(host), Reach::Public, "{host}");
        }
    }

    #[test]
    fn outside_privileged_mode_everything_is_allowed() {
        for reach in [Reach::Loopback, Reach::Lan, Reach::Public] {
            for category in [Category::Ai, Category::Download] {
                assert_eq!(decide(OFF, category, reach), Verdict::Allow);
            }
        }
    }

    #[test]
    fn privileged_mode_blocks_cloud_ai_and_allows_loopback() {
        assert_eq!(
            decide(ON, Category::Ai, Reach::Public),
            Verdict::BlockCloudAi
        );
        assert_eq!(decide(ON, Category::Ai, Reach::Loopback), Verdict::Allow);
    }

    #[test]
    fn lan_ai_servers_need_their_own_setting() {
        assert_eq!(decide(ON, Category::Ai, Reach::Lan), Verdict::BlockLanAi);
        let allow_lan = Rules {
            allow_lan: true,
            ..ON
        };
        assert_eq!(decide(allow_lan, Category::Ai, Reach::Lan), Verdict::Allow);
        // It never opens the door to the internet.
        assert_eq!(
            decide(allow_lan, Category::Ai, Reach::Public),
            Verdict::BlockCloudAi
        );
    }

    #[test]
    fn downloads_stay_allowed_unless_also_blocked() {
        assert_eq!(
            decide(ON, Category::Download, Reach::Public),
            Verdict::Allow
        );
        let offline = Rules {
            block_downloads: true,
            ..ON
        };
        assert_eq!(
            decide(offline, Category::Download, Reach::Public),
            Verdict::BlockDownload
        );
        assert_eq!(
            decide(offline, Category::Download, Reach::Loopback),
            Verdict::Allow
        );
        // The option means nothing with Privileged mode off.
        let off_with_option = Rules {
            privileged: false,
            block_downloads: true,
            ..ON
        };
        assert_eq!(
            decide(off_with_option, Category::Download, Reach::Public),
            Verdict::Allow
        );
    }
}
