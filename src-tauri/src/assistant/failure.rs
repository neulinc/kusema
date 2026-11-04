//! Every way a request to the AI can fail before or while it's answered,
//! worded for the person at the computer. Each has a translation key for
//! the pill (`overlay.ai.*`), the English that's spoken and logged, and the
//! settings page that fixes it, for the pill's "Set up" button.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::settings::PostProcessProvider;

/// Where "Set up" goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SetupPage {
    /// Assistant → Settings: the provider, its key and model.
    Assistant,
    /// Privacy: Privileged mode.
    Privacy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// Nothing is set up: no provider, or one with neither key nor model.
    NoProvider,
    NoKey(String),
    NoModel(String),
    /// Apple Intelligence can't answer questions yet.
    ProviderUnsupported(String),
    /// The provider chosen for screen questions was removed.
    ScreenProviderGone,
    KeyRejected(String),
    /// Ollama or LM Studio on this computer isn't running.
    LocalNotRunning(String),
    Unreachable(String),
    Privileged(String),
    RateLimited(String),
    NotFound(String),
    NoVision,
    EmptyReply(String),
    /// Nothing was heard after switching to the Assistant.
    NothingHeard,
    /// Anything else, with the provider's own words.
    Other(String, String),
}

impl Failure {
    /// The key under `overlay.ai`.
    pub fn key(&self) -> &'static str {
        match self {
            Failure::NoProvider => "noProvider",
            Failure::NoKey(_) => "noKey",
            Failure::NoModel(_) => "noModel",
            Failure::ProviderUnsupported(_) => "providerUnsupported",
            Failure::ScreenProviderGone => "screenProviderGone",
            Failure::KeyRejected(_) => "keyRejected",
            Failure::LocalNotRunning(_) => "localNotRunning",
            Failure::Unreachable(_) => "unreachable",
            Failure::Privileged(_) => "privileged",
            Failure::RateLimited(_) => "rateLimited",
            Failure::NotFound(_) => "notFound",
            Failure::NoVision => "noVision",
            Failure::EmptyReply(_) => "emptyReply",
            Failure::NothingHeard => "nothingHeard",
            Failure::Other(_, _) => "other",
        }
    }

    /// The provider's name, for the key's `{{name}}` (for `Other`, with what
    /// it said).
    pub fn name(&self) -> Option<String> {
        match self {
            Failure::NoKey(n)
            | Failure::NoModel(n)
            | Failure::ProviderUnsupported(n)
            | Failure::KeyRejected(n)
            | Failure::LocalNotRunning(n)
            | Failure::Unreachable(n)
            | Failure::Privileged(n)
            | Failure::RateLimited(n)
            | Failure::NotFound(n)
            | Failure::EmptyReply(n) => Some(n.clone()),
            Failure::Other(n, error) => Some(format!("{n}: {error}")),
            _ => None,
        }
    }

    /// The words in English: spoken, logged, and the pill's text when its
    /// translation is missing. Kept the same as `overlay.ai.*` in en.
    pub fn english(&self) -> String {
        let n = self.name().unwrap_or_default();
        match self {
            Failure::NoProvider => {
                "Kusema needs an AI to answer. Set one up in Assistant \u{2192} Settings.".to_string()
            }
            Failure::NoKey(_) => format!("No API key for {n}. Add one in Assistant \u{2192} Settings."),
            Failure::NoModel(_) => {
                format!("No model chosen for {n}. Choose one in Assistant \u{2192} Settings.")
            }
            Failure::ProviderUnsupported(_) => format!(
                "{n} can't answer questions yet. Choose another AI in Assistant \u{2192} Settings."
            ),
            Failure::ScreenProviderGone => "The AI chosen for screen questions is gone. Choose another in Assistant \u{2192} Settings.".to_string(),
            Failure::KeyRejected(_) => {
                format!("{n} didn't accept the API key. Check it in Assistant \u{2192} Settings.")
            }
            Failure::LocalNotRunning(_) => {
                format!("{n} isn't running on this computer. Start it, then ask again.")
            }
            Failure::Unreachable(_) => format!(
                "Couldn't reach {n}. Check your internet connection and the AI's address in Assistant \u{2192} Settings."
            ),
            Failure::Privileged(_) => "Privileged mode is on, so cloud AI is blocked. Use a local AI, or turn Privileged mode off.".to_string(),
            Failure::RateLimited(_) => format!("{n} is busy. Wait a moment, then ask again."),
            Failure::NotFound(_) => {
                format!("{n} couldn't find that model. Check it in Assistant \u{2192} Settings.")
            }
            Failure::NoVision => super::NO_VISION_MESSAGE.to_string(),
            Failure::EmptyReply(_) => format!("{n} didn't send an answer. Try asking again."),
            Failure::NothingHeard => {
                "Kusema didn't hear a question. Hold the key, tap Space, then speak.".to_string()
            }
            Failure::Other(_, _) => format!("Kusema couldn't get an answer: {n}"),
        }
    }

    /// Where "Set up" goes, when settings can fix it.
    pub fn setup(&self) -> Option<SetupPage> {
        match self {
            Failure::NoProvider
            | Failure::NoKey(_)
            | Failure::NoModel(_)
            | Failure::ProviderUnsupported(_)
            | Failure::ScreenProviderGone
            | Failure::KeyRejected(_)
            | Failure::NotFound(_)
            | Failure::NoVision
            | Failure::Unreachable(_) => Some(SetupPage::Assistant),
            Failure::Privileged(_) => Some(SetupPage::Privacy),
            Failure::LocalNotRunning(_)
            | Failure::RateLimited(_)
            | Failure::EmptyReply(_)
            | Failure::NothingHeard
            | Failure::Other(_, _) => None,
        }
    }
}

/// Whether an error says the model doesn't take images. Providers word this
/// differently ("image_url is only supported by certain models", "unknown
/// variant `image_url`, expected `text`", "does not support image input"...).
fn is_image_rejection(error: &str) -> bool {
    let error = error.to_lowercase();
    [
        "image",
        "vision",
        "multimodal",
        "multi-modal",
        "unknown variant",
        "expected `text`",
        "must be a string",
        "content type",
    ]
    .iter()
    .any(|hint| error.contains(hint))
}

/// An llm_client error, as a failure. `with_image`: the request carried a
/// screenshot.
pub fn classify(provider: &PostProcessProvider, error: &str, with_image: bool) -> Failure {
    let label = provider.label.clone();
    let connect = error.contains("kind: connect") || error.contains("kind: timeout");
    let on_this_computer = crate::network::is_on_this_computer(&provider.base_url);
    if crate::network::is_blocked(error) {
        Failure::Privileged(label)
    } else if error.contains("status 401") || error.contains("status 403") {
        Failure::KeyRejected(label)
    } else if with_image && !error.contains("status 429") && is_image_rejection(error) {
        Failure::NoVision
    } else if error.contains("status 404") {
        Failure::NotFound(label)
    } else if error.contains("status 429") {
        Failure::RateLimited(label)
    } else if connect && on_this_computer {
        Failure::LocalNotRunning(label)
    } else if connect {
        Failure::Unreachable(label)
    } else {
        Failure::Other(label, error.to_string())
    }
}

/// Why the provider can't be used, before anything is sent. `refused`:
/// Privileged mode would refuse it anyway, which says more than a missing
/// key or model would.
pub fn missing_setup(
    provider: &PostProcessProvider,
    model: &str,
    api_key: &str,
    refused: bool,
) -> Option<Failure> {
    let needs_key = !crate::local_providers::key_optional(&provider.id);
    if refused {
        return None;
    }
    match (model.is_empty(), api_key.is_empty() && needs_key) {
        // Never set up at all.
        (true, true) => Some(Failure::NoProvider),
        (true, false) => Some(Failure::NoModel(provider.label.clone())),
        (false, true) => Some(Failure::NoKey(provider.label.clone())),
        (false, false) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;

    fn provider(id: &str) -> PostProcessProvider {
        get_default_settings()
            .post_process_providers
            .into_iter()
            .find(|p| p.id == id)
            .unwrap()
    }

    #[test]
    fn each_missing_piece_is_named_with_the_way_to_fix_it() {
        let anthropic = provider("anthropic");
        // Nothing chosen yet: the general message.
        let none = missing_setup(&anthropic, "", "", false).unwrap();
        assert_eq!(none, Failure::NoProvider);
        assert_eq!(
            none.english(),
            "Kusema needs an AI to answer. Set one up in Assistant \u{2192} Settings."
        );
        assert_eq!(none.setup(), Some(SetupPage::Assistant));
        assert_eq!(
            missing_setup(&anthropic, "claude", "", false),
            Some(Failure::NoKey("Anthropic".into()))
        );
        assert_eq!(
            missing_setup(&anthropic, "", "sk", false),
            Some(Failure::NoModel("Anthropic".into()))
        );
        assert_eq!(missing_setup(&anthropic, "claude", "sk", false), None);
        // A local provider needs no key, only a model.
        let ollama = provider("ollama");
        assert_eq!(missing_setup(&ollama, "llama3", "", false), None);
        assert_eq!(
            missing_setup(&ollama, "", "", false),
            Some(Failure::NoModel("Ollama".into()))
        );
        // Privileged mode would refuse it: that's what to say instead.
        assert_eq!(missing_setup(&anthropic, "", "", true), None);
    }

    #[test]
    fn request_errors_are_told_apart() {
        let anthropic = provider("anthropic");
        let ollama = provider("ollama");
        let blocked = crate::network::Blocked {
            host: "api.anthropic.com".to_string(),
            verdict: crate::network::BlockReason::CloudAi,
        }
        .to_string();
        assert_eq!(
            classify(&anthropic, &blocked, false),
            Failure::Privileged("Anthropic".into())
        );
        assert_eq!(
            classify(&anthropic, &blocked, false).setup(),
            Some(SetupPage::Privacy)
        );
        assert_eq!(
            classify(
                &anthropic,
                "API request failed with status 401 Unauthorized",
                false
            ),
            Failure::KeyRejected("Anthropic".into())
        );
        let refused = "error sending request (kind: connect, Connection refused)";
        // A local server that isn't running is said as such...
        let local = classify(&ollama, refused, false);
        assert_eq!(local, Failure::LocalNotRunning("Ollama".into()));
        assert_eq!(
            local.english(),
            "Ollama isn't running on this computer. Start it, then ask again."
        );
        // ...a cloud one as unreachable.
        assert_eq!(
            classify(&anthropic, refused, false),
            Failure::Unreachable("Anthropic".into())
        );
        assert_eq!(
            classify(&anthropic, "status 400: image_url is only supported", true),
            Failure::NoVision
        );
        assert_eq!(
            classify(&anthropic, "status 429 Too Many Requests", true),
            Failure::RateLimited("Anthropic".into())
        );
    }

    #[test]
    fn every_failure_has_a_key_and_words() {
        static EN: std::sync::LazyLock<serde_json::Value> = std::sync::LazyLock::new(|| {
            serde_json::from_str(include_str!(
                "../../../src/i18n/locales/en/translation.json"
            ))
            .expect("en parses")
        });
        for failure in [
            Failure::NoProvider,
            Failure::NoKey("A".into()),
            Failure::NoModel("A".into()),
            Failure::ProviderUnsupported("A".into()),
            Failure::ScreenProviderGone,
            Failure::KeyRejected("A".into()),
            Failure::LocalNotRunning("A".into()),
            Failure::Unreachable("A".into()),
            Failure::Privileged("A".into()),
            Failure::RateLimited("A".into()),
            Failure::NotFound("A".into()),
            Failure::NoVision,
            Failure::EmptyReply("A".into()),
            Failure::NothingHeard,
            Failure::Other("A".into(), "boom".into()),
        ] {
            // The pill's translation and the spoken English say the same.
            let en = &EN["overlay"]["ai"][failure.key()];
            let text = en
                .as_str()
                .unwrap_or_else(|| panic!("no en {}", failure.key()));
            let name = failure.name().unwrap_or_default();
            assert_eq!(text.replace("{{name}}", &name), failure.english());
        }
    }
}
