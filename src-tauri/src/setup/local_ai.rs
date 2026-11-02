//! Looks for Ollama on this computer, for setup's AI assistant screen. Goes
//! through `llm_client::fetch_models`, so the privacy choke point sees it.

use crate::llm_client;
use crate::settings::{self, PostProcessProvider};
use serde::Serialize;
use specta::Type;
use std::time::Duration;
use tauri::AppHandle;

/// Where Ollama listens by default: its OpenAI-compatible API.
pub const OLLAMA_BASE_URL: &str = "http://localhost:11434/v1";
/// Something else on that port must not hold up the screen.
const DETECT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct LocalAiStatus {
    pub found: bool,
    /// Installed models. Empty when it's running but none are installed yet.
    pub models: Vec<String>,
    pub base_url: String,
}

async fn detect_at(provider: PostProcessProvider, base_url: &str) -> LocalAiStatus {
    let provider = PostProcessProvider {
        base_url: base_url.to_string(),
        ..provider
    };
    let listed = tokio::time::timeout(
        DETECT_TIMEOUT,
        llm_client::fetch_models(&provider, String::new()),
    )
    .await;
    let (found, models) = match listed {
        Ok(Ok(models)) => (true, models),
        Ok(Err(e)) => {
            log::debug!("No local AI found: {e}");
            (false, Vec::new())
        }
        Err(_) => {
            log::debug!("No local AI found: no answer in time");
            (false, Vec::new())
        }
    };
    LocalAiStatus {
        found,
        models,
        base_url: base_url.to_string(),
    }
}

fn provider(app: &AppHandle, id: &str) -> Result<PostProcessProvider, String> {
    settings::get_settings(app)
        .post_process_providers
        .into_iter()
        .find(|p| p.id == id)
        .ok_or(format!("The {id} provider is missing from the settings."))
}

/// Is Ollama running on this computer, and with which models?
#[tauri::command]
#[specta::specta]
pub async fn detect_local_ai(app: AppHandle) -> Result<LocalAiStatus, String> {
    let ollama = provider(&app, crate::local_providers::OLLAMA_ID)?;
    Ok(detect_at(ollama, OLLAMA_BASE_URL).await)
}

/// Is this local provider's server running, and with which models? "Not
/// running" is an answer here, not an error.
#[tauri::command]
#[specta::specta]
pub async fn detect_local_provider(
    app: AppHandle,
    provider_id: String,
) -> Result<LocalAiStatus, String> {
    let local = provider(&app, &provider_id)?;
    let base_url = local.base_url.clone();
    Ok(detect_at(local, &base_url).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::{
        self, set_test_gate, Gate, GateConfig, LedgerEntry, LedgerFilter, Purpose, RequestResult,
        Rules,
    };
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn provider() -> PostProcessProvider {
        settings::get_default_settings()
            .post_process_providers
            .into_iter()
            .find(|p| p.id == crate::local_providers::OLLAMA_ID)
            .unwrap()
    }

    async fn serve(status: &str, body: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request).await.unwrap();
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        format!("http://{address}/v1")
    }

    /// Runs the detection under `rules` and returns what the ledger recorded.
    fn detect(
        rules: Rules,
        base_url: impl FnOnce() -> String,
    ) -> (LocalAiStatus, Vec<LedgerEntry>) {
        let dir = tempfile::TempDir::new().unwrap();
        let ledger = network::test_ledger(&dir.path().join("ledger.db"));
        set_test_gate(Arc::new(Gate::new(
            move || GateConfig {
                rules,
                retention_days: 30,
            },
            Some(ledger.clone()),
            || {},
        )));
        let url = base_url();
        let status = tauri::async_runtime::block_on(detect_at(provider(), &url));
        let entries = ledger.list(&LedgerFilter::default(), 10).unwrap();
        (status, entries)
    }

    fn privileged() -> Rules {
        Rules {
            privileged: true,
            ..Default::default()
        }
    }

    fn running(body: &'static str) -> impl FnOnce() -> String {
        move || tauri::async_runtime::block_on(serve("200 OK", body))
    }

    #[test]
    fn lists_the_installed_models_and_the_ledger_records_the_request() {
        let (status, entries) = detect(
            Rules::default(),
            running(r#"{"data":[{"id":"llama3.2"},{"id":"gemma3"}]}"#),
        );
        assert!(status.found);
        assert_eq!(status.models, ["llama3.2", "gemma3"]);
        assert_eq!(entries.len(), 1, "one request, through the choke point");
        assert_eq!(entries[0].purpose, Purpose::ProviderModels);
        assert_eq!(entries[0].result, RequestResult::Sent);
    }

    #[test]
    fn privileged_mode_still_allows_looking_on_this_computer() {
        let (status, entries) = detect(privileged(), running(r#"{"data":[{"id":"llama3.2"}]}"#));
        assert!(status.found);
        assert_eq!(entries[0].result, RequestResult::Sent);
    }

    #[test]
    fn running_with_no_models_is_found_and_empty() {
        let (status, _) = detect(Rules::default(), running(r#"{"data":[]}"#));
        assert!(status.found);
        assert!(status.models.is_empty());
    }

    #[test]
    fn nothing_listening_is_not_found() {
        let closed = || {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}/v1", listener.local_addr().unwrap())
        };
        let (status, entries) = detect(Rules::default(), closed);
        assert!(!status.found);
        assert!(status.models.is_empty());
        assert_eq!(entries.len(), 1, "failed attempts are recorded too");
        assert_eq!(entries[0].result, RequestResult::Failed);
    }

    #[test]
    fn something_else_on_the_port_is_not_found() {
        let (status, _) = detect(Rules::default(), || {
            tauri::async_runtime::block_on(serve("404 Not Found", "nope"))
        });
        assert!(!status.found);
    }

    #[test]
    fn a_cloud_address_would_be_refused_at_the_choke_point() {
        let (status, entries) = detect(privileged(), || "https://api.openai.com/v1".to_string());
        assert!(!status.found);
        assert_eq!(entries[0].result, RequestResult::Blocked);
    }
}
