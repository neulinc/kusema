//! Kusema: API keys live in the OS keychain (Windows Credential Manager,
//! macOS Keychain, Linux Secret Service) instead of the settings file.
//!
//! In memory, `AppSettings::post_process_api_keys` still holds the real keys,
//! so every consumer and the settings UI work as before. Only the copy written
//! to disk changes: `for_disk` moves each key into the keychain and blanks it
//! in the file, and `load_keys` fills the blanks back in when settings are read.
//! A plaintext key found in the file (an older install) is migrated the same
//! way. If the keychain can't be used, keys stay in the file as before.

use crate::settings::AppSettings;
use log::{debug, warn};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

const SERVICE: &str = "com.neulinc.kusema";

/// A place to keep secrets, one per provider id.
trait KeyStore: Send + Sync {
    fn get(&self, id: &str) -> Result<Option<String>, String>;
    fn set(&self, id: &str, secret: &str) -> Result<(), String>;
    fn delete(&self, id: &str) -> Result<(), String>;
}

struct OsKeychain;

impl OsKeychain {
    /// Keychain backends may block (Secret Service talks D-Bus), so calls run
    /// on a plain OS thread, outside any async runtime that called us.
    fn run<T: Send>(f: impl FnOnce() -> keyring::Result<T> + Send) -> Result<T, String> {
        std::thread::scope(|s| s.spawn(f).join())
            .map_err(|_| "keychain thread panicked".to_string())?
            .map_err(|e| e.to_string())
    }
}

impl KeyStore for OsKeychain {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        Self::run(|| match keyring::Entry::new(SERVICE, id)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e),
        })
    }

    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
        Self::run(|| keyring::Entry::new(SERVICE, id)?.set_password(secret))
    }

    fn delete(&self, id: &str) -> Result<(), String> {
        Self::run(
            || match keyring::Entry::new(SERVICE, id)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(e),
            },
        )
    }
}

struct Secrets {
    store: Box<dyn KeyStore>,
    /// What the keychain holds per provider id, as far as we know; "" means
    /// no entry. Ids not in the map haven't been looked up yet.
    known: Mutex<HashMap<String, String>>,
    warned: AtomicBool,
}

impl Secrets {
    fn new(store: Box<dyn KeyStore>) -> Self {
        Self {
            store,
            known: Mutex::new(HashMap::new()),
            warned: AtomicBool::new(false),
        }
    }

    fn known(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.known.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn warn_unavailable(&self, error: &str) {
        if !self.warned.swap(true, Ordering::SeqCst) {
            warn!("OS keychain unavailable ({error}); API keys stay in the settings file");
        }
    }

    fn lookup(&self, id: &str) -> Option<String> {
        if let Some(secret) = self.known().get(id) {
            return Some(secret.clone()).filter(|s| !s.is_empty());
        }
        match self.store.get(id) {
            Ok(secret) => {
                let secret = secret.unwrap_or_default();
                self.known().insert(id.to_string(), secret.clone());
                Some(secret).filter(|s| !s.is_empty())
            }
            Err(e) => {
                self.warn_unavailable(&e);
                None
            }
        }
    }

    /// Make sure the keychain holds `secret` for `id`.
    fn store(&self, id: &str, secret: &str) -> bool {
        if self.known().get(id).is_some_and(|known| known == secret) {
            return true;
        }
        match self.store.set(id, secret) {
            Ok(()) => {
                self.known().insert(id.to_string(), secret.to_string());
                true
            }
            Err(e) => {
                self.warn_unavailable(&e);
                false
            }
        }
    }

    /// Fill in keys from the keychain. Plaintext keys from the file are moved
    /// into the keychain; returns true when the file should be rewritten
    /// without them.
    fn load_keys(&self, settings: &mut AppSettings) -> bool {
        let mut migrated = false;
        for (id, secret) in settings.post_process_api_keys.iter_mut() {
            if !secret.is_empty() {
                if self.store(id, secret) {
                    debug!("Moved the API key for '{id}' into the OS keychain");
                    migrated = true;
                }
            } else if let Some(stored) = self.lookup(id) {
                *secret = stored;
            }
        }
        migrated
    }

    /// The settings as they should be written to disk: keys the keychain
    /// holds are blanked, and keys the user cleared are removed from it.
    fn for_disk(&self, settings: &AppSettings) -> serde_json::Value {
        let mut disk = settings.clone();
        for (id, secret) in disk.post_process_api_keys.iter_mut() {
            if !secret.is_empty() {
                if self.store(id, secret) {
                    secret.clear();
                }
                continue;
            }
            let had_key = self.known().get(id).is_some_and(|s| !s.is_empty());
            if had_key {
                match self.store.delete(id) {
                    Ok(()) => {
                        self.known().insert(id.clone(), String::new());
                    }
                    Err(e) => {
                        warn!("Could not remove the API key for '{id}' from the keychain: {e}")
                    }
                }
            }
        }
        serde_json::to_value(&disk).expect("settings serialize to JSON")
    }
}

fn secrets() -> &'static Secrets {
    static SECRETS: OnceLock<Secrets> = OnceLock::new();
    SECRETS.get_or_init(|| {
        // Tests must never touch the real keychain.
        #[cfg(test)]
        let store: Box<dyn KeyStore> = Box::new(tests::MemoryStore::default());
        #[cfg(not(test))]
        let store: Box<dyn KeyStore> = Box::new(OsKeychain);
        Secrets::new(store)
    })
}

/// Fill `settings` with the API keys kept in the keychain, migrating any
/// plaintext keys into it. Returns true when the stored file needs rewriting.
pub fn load_keys(settings: &mut AppSettings) -> bool {
    secrets().load_keys(settings)
}

/// `settings` as JSON for the settings file, without the API keys the
/// keychain holds.
pub fn for_disk(settings: &AppSettings) -> serde_json::Value {
    secrets().for_disk(settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;
    use std::sync::Arc;

    #[derive(Default)]
    pub(super) struct MemoryStore {
        pub entries: Mutex<HashMap<String, String>>,
    }

    impl KeyStore for MemoryStore {
        fn get(&self, id: &str) -> Result<Option<String>, String> {
            Ok(self.entries.lock().unwrap().get(id).cloned())
        }
        fn set(&self, id: &str, secret: &str) -> Result<(), String> {
            self.entries
                .lock()
                .unwrap()
                .insert(id.to_string(), secret.to_string());
            Ok(())
        }
        fn delete(&self, id: &str) -> Result<(), String> {
            self.entries.lock().unwrap().remove(id);
            Ok(())
        }
    }

    /// A keychain that is never available (e.g. Linux without Secret Service).
    struct BrokenStore;

    impl KeyStore for BrokenStore {
        fn get(&self, _: &str) -> Result<Option<String>, String> {
            Err("no keychain".into())
        }
        fn set(&self, _: &str, _: &str) -> Result<(), String> {
            Err("no keychain".into())
        }
        fn delete(&self, _: &str) -> Result<(), String> {
            Err("no keychain".into())
        }
    }

    /// Lets a test keep a handle on the store it gives to `Secrets`.
    impl<T: KeyStore> KeyStore for Arc<T> {
        fn get(&self, id: &str) -> Result<Option<String>, String> {
            (**self).get(id)
        }
        fn set(&self, id: &str, secret: &str) -> Result<(), String> {
            (**self).set(id, secret)
        }
        fn delete(&self, id: &str) -> Result<(), String> {
            (**self).delete(id)
        }
    }

    fn memory() -> (Secrets, Arc<MemoryStore>) {
        let store = Arc::new(MemoryStore::default());
        (Secrets::new(Box::new(store.clone())), store)
    }

    fn with_key(id: &str, key: &str) -> AppSettings {
        let mut settings = get_default_settings();
        settings
            .post_process_api_keys
            .insert(id.to_string(), key.to_string());
        settings
    }

    fn disk_key(disk: &serde_json::Value, id: &str) -> String {
        disk["post_process_api_keys"][id]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn plaintext_keys_are_migrated_and_blanked_on_disk() {
        let (secrets, store) = memory();
        let mut settings = with_key("anthropic", "sk-ant-secret");

        assert!(secrets.load_keys(&mut settings));
        assert_eq!(settings.post_process_api_keys["anthropic"], "sk-ant-secret");
        assert_eq!(
            store
                .entries
                .lock()
                .unwrap()
                .get("anthropic")
                .map(String::as_str),
            Some("sk-ant-secret")
        );

        let disk = secrets.for_disk(&settings);
        assert_eq!(disk_key(&disk, "anthropic"), "");
        assert!(!disk.to_string().contains("sk-ant-secret"));
    }

    #[test]
    fn blank_keys_are_filled_from_the_keychain() {
        let (secrets, store) = memory();
        store.set("openai", "sk-openai").unwrap();
        let mut settings = with_key("openai", "");

        assert!(!secrets.load_keys(&mut settings));
        assert_eq!(settings.post_process_api_keys["openai"], "sk-openai");
    }

    #[test]
    fn changing_and_clearing_a_key_updates_the_keychain() {
        let (secrets, store) = memory();
        let mut settings = with_key("groq", "old");
        secrets.load_keys(&mut settings);

        settings
            .post_process_api_keys
            .insert("groq".into(), "new".into());
        assert_eq!(disk_key(&secrets.for_disk(&settings), "groq"), "");
        assert_eq!(store.get("groq").unwrap().as_deref(), Some("new"));

        settings
            .post_process_api_keys
            .insert("groq".into(), String::new());
        secrets.for_disk(&settings);
        assert_eq!(store.get("groq").unwrap(), None);
    }

    #[test]
    fn unknown_blank_keys_are_never_deleted() {
        // A provider whose keychain entry hasn't been looked up yet (e.g. one
        // just added by ensure_post_process_defaults) must not lose its key.
        let (secrets, store) = memory();
        store.set("cerebras", "kept").unwrap();
        secrets.for_disk(&with_key("cerebras", ""));
        assert_eq!(store.get("cerebras").unwrap().as_deref(), Some("kept"));
    }

    #[test]
    fn unavailable_keychain_keeps_keys_in_the_file() {
        let secrets = Secrets::new(Box::new(BrokenStore));
        let mut settings = with_key("anthropic", "sk-ant-secret");

        assert!(!secrets.load_keys(&mut settings));
        assert_eq!(settings.post_process_api_keys["anthropic"], "sk-ant-secret");
        let disk = secrets.for_disk(&settings);
        assert_eq!(disk_key(&disk, "anthropic"), "sk-ant-secret");
    }

    #[test]
    fn keychain_is_only_queried_once_per_provider() {
        struct Counting(std::sync::atomic::AtomicUsize);
        impl KeyStore for Counting {
            fn get(&self, _: &str) -> Result<Option<String>, String> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(Some("k".into()))
            }
            fn set(&self, _: &str, _: &str) -> Result<(), String> {
                Ok(())
            }
            fn delete(&self, _: &str) -> Result<(), String> {
                Ok(())
            }
        }
        let counting = Arc::new(Counting(Default::default()));
        let secrets = Secrets::new(Box::new(counting.clone()));
        let mut settings = with_key("openai", "");
        secrets.load_keys(&mut settings);
        let first = counting.0.load(Ordering::SeqCst);
        assert_eq!(first, settings.post_process_api_keys.len());
        for _ in 0..3 {
            secrets.load_keys(&mut with_key("openai", ""));
        }
        assert_eq!(counting.0.load(Ordering::SeqCst), first);
    }

    /// Uses the real OS keychain: `cargo test os_keychain -- --ignored`
    #[test]
    #[ignore]
    fn os_keychain_round_trip() {
        let id = "kusema-test-entry";
        let keychain = OsKeychain;
        keychain.set(id, "secret-value").unwrap();
        assert_eq!(keychain.get(id).unwrap().as_deref(), Some("secret-value"));
        keychain.delete(id).unwrap();
        assert_eq!(keychain.get(id).unwrap(), None);
        keychain.delete(id).unwrap(); // deleting a missing entry is fine
    }
}
