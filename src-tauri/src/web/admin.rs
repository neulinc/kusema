//! Kusema: settings an administrator sets for every user of a computer, as
//! Windows policy values under
//! `HKEY_LOCAL_MACHINE\SOFTWARE\Policies\Neulinc\Kusema` (Group Policy or
//! Intune can deploy them; users without admin rights can't change them):
//!
//! | Value              | Type      | Effect                                              |
//! |--------------------|-----------|-----------------------------------------------------|
//! | `DisableWebAccess` | REG_DWORD | 1: no web search and no link reading, in any mode   |
//! | `HiddenProviders`  | REG_SZ    | AI providers to hide, by id, comma-separated        |
//! | `SearxngUrl`       | REG_SZ    | The firm's SearXNG server, fixed for every user     |
//!
//! Read once at startup: a change applies the next time Kusema starts.
//! See docs/it-admin.md.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::OnceLock;

/// The registry key, under HKEY_LOCAL_MACHINE.
pub const POLICY_KEY: &str = r"SOFTWARE\Policies\Neulinc\Kusema";

/// What the administrator decided. Everything off when nothing is set.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct AdminPolicy {
    pub disable_web_access: bool,
    /// Provider ids, lowercase ("deepseek", "gemini", "openai"...).
    pub hidden_providers: Vec<String>,
    /// The SearXNG server set for everyone; the user can't change it.
    pub searxng_url: Option<String>,
}

impl AdminPolicy {
    pub fn hides(&self, provider_id: &str) -> bool {
        self.hidden_providers
            .iter()
            .any(|id| id.eq_ignore_ascii_case(provider_id))
    }
}

/// `HiddenProviders`: "deepseek, Gemini ,,openai" → ["deepseek", "gemini", "openai"].
pub fn parse_provider_list(value: &str) -> Vec<String> {
    value
        .split([',', ';'])
        .map(|id| id.trim().to_ascii_lowercase())
        .filter(|id| !id.is_empty())
        .collect()
}

/// A usable server address, or nothing.
fn parse_url(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches('/');
    (value.starts_with("http://") || value.starts_with("https://")).then(|| value.to_string())
}

#[cfg(windows)]
fn read_registry() -> AdminPolicy {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;
    let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(POLICY_KEY, KEY_READ)
    else {
        return AdminPolicy::default();
    };
    let disable_web_access = key
        .get_value::<u32, _>("DisableWebAccess")
        .is_ok_and(|v| v != 0);
    let hidden_providers = key
        .get_value::<String, _>("HiddenProviders")
        .map(|v| parse_provider_list(&v))
        .unwrap_or_default();
    let searxng_url = key
        .get_value::<String, _>("SearxngUrl")
        .ok()
        .and_then(|v| parse_url(&v));
    AdminPolicy {
        disable_web_access,
        hidden_providers,
        searxng_url,
    }
}

#[cfg(not(windows))]
fn read_registry() -> AdminPolicy {
    AdminPolicy::default()
}

fn read() -> AdminPolicy {
    // Debug builds: `KUSEMA_DEBUG_ADMIN_POLICY` (JSON) stands in for the
    // registry, so isolated test runs never need to touch the machine's.
    #[cfg(debug_assertions)]
    if let Ok(json) = std::env::var("KUSEMA_DEBUG_ADMIN_POLICY") {
        match serde_json::from_str::<AdminPolicy>(&json) {
            Ok(mut policy) => {
                policy.hidden_providers = policy
                    .hidden_providers
                    .iter()
                    .flat_map(|id| parse_provider_list(id))
                    .collect();
                policy.searxng_url = policy.searxng_url.as_deref().and_then(parse_url);
                log::info!("Admin policy from KUSEMA_DEBUG_ADMIN_POLICY: {policy:?}");
                return policy;
            }
            Err(e) => log::warn!("KUSEMA_DEBUG_ADMIN_POLICY isn't valid: {e}"),
        }
    }
    let policy = read_registry();
    if policy != AdminPolicy::default() {
        log::info!("Admin policy: {policy:?}");
    }
    policy
}

static POLICY: OnceLock<AdminPolicy> = OnceLock::new();

#[cfg(test)]
thread_local! {
    static TEST_POLICY: std::cell::RefCell<Option<AdminPolicy>> = const { std::cell::RefCell::new(None) };
}

/// Uses `policy` for the rest of this test thread.
#[cfg(test)]
pub fn set_test_policy(policy: AdminPolicy) {
    TEST_POLICY.with(|slot| *slot.borrow_mut() = Some(policy));
}

/// The policy in force.
pub fn policy() -> AdminPolicy {
    #[cfg(test)]
    {
        TEST_POLICY.with(|slot| slot.borrow().clone().unwrap_or_default())
    }
    #[cfg(not(test))]
    POLICY.get_or_init(read).clone()
}

#[tauri::command]
#[specta::specta]
pub fn get_admin_policy() -> AdminPolicy {
    policy()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_providers_are_a_forgiving_list() {
        assert_eq!(
            parse_provider_list("deepseek, Gemini ,,OpenAI;zai"),
            ["deepseek", "gemini", "openai", "zai"]
        );
        assert!(parse_provider_list("  ").is_empty());
        let policy = AdminPolicy {
            hidden_providers: parse_provider_list("DeepSeek"),
            ..Default::default()
        };
        assert!(policy.hides("deepseek"));
        assert!(policy.hides("DEEPSEEK"));
        assert!(!policy.hides("openai"));
    }

    #[test]
    fn only_a_web_address_is_taken_as_the_server() {
        assert_eq!(
            parse_url(" https://search.firm.example/ "),
            Some("https://search.firm.example".into())
        );
        assert_eq!(parse_url("search.firm.example"), None);
        assert_eq!(parse_url("file:///c:/x"), None);
    }

    #[test]
    fn nothing_set_means_nothing_restricted() {
        let policy = AdminPolicy::default();
        assert!(!policy.disable_web_access);
        assert!(policy.hidden_providers.is_empty());
        assert!(policy.searxng_url.is_none());
    }
}
