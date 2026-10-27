use crate::llm_client::{ChatRole, ChatTurn};

/// Default instructions for Assistant mode. Replies are meant to be read
/// aloud, so the model is steered away from formatting that doesn't speak.
pub const DEFAULT_SYSTEM_PROMPT: &str = "You are Kusema, a helpful voice assistant. \
Answers will be spoken aloud, so reply in short, natural spoken sentences. \
No markdown, lists, code blocks or emoji unless asked.";

/// Most messages kept. Past this the oldest question/answer pair is dropped,
/// so a long session can't grow every request without bound. Even, so the
/// history always starts with a user message.
const MAX_MESSAGES: usize = 40;

/// An ordered user/assistant exchange plus the system prompt it runs under.
#[derive(Debug, Clone, PartialEq)]
pub struct Conversation {
    system_prompt: String,
    messages: Vec<ChatTurn>,
}

impl Default for Conversation {
    fn default() -> Self {
        Self::new(DEFAULT_SYSTEM_PROMPT)
    }
}

impl Conversation {
    pub fn new(system_prompt: impl Into<String>) -> Self {
        Self {
            system_prompt: system_prompt.into(),
            messages: Vec::new(),
        }
    }

    pub fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    /// Messages, oldest first.
    pub fn messages(&self) -> &[ChatTurn] {
        &self.messages
    }

    /// The history plus a new question, as it should be sent to the model.
    pub fn with_question(&self, question: &str) -> Vec<ChatTurn> {
        let mut turns = self.messages.clone();
        turns.push(ChatTurn {
            role: ChatRole::User,
            content: question.to_string(),
        });
        turns
    }

    /// Record a finished exchange. Both halves are added together, so a failed
    /// or cancelled request never leaves an unanswered question behind.
    pub fn push_exchange(&mut self, question: String, answer: String) {
        self.messages.push(ChatTurn {
            role: ChatRole::User,
            content: question,
        });
        self.messages.push(ChatTurn {
            role: ChatRole::Assistant,
            content: answer,
        });
        let excess = self.messages.len().saturating_sub(MAX_MESSAGES);
        // Drop whole pairs so the history keeps starting with a question.
        self.messages.drain(..excess.next_multiple_of(2));
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_uses_spoken_system_prompt() {
        let conversation = Conversation::default();
        assert_eq!(conversation.system_prompt(), DEFAULT_SYSTEM_PROMPT);
        assert!(conversation.messages().is_empty());
    }

    #[test]
    fn question_is_appended_without_changing_history() {
        let mut conversation = Conversation::default();
        conversation.push_exchange("Capital of Ghana?".into(), "Accra.".into());

        let turns = conversation.with_question("Its population?");
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[2].role, ChatRole::User);
        assert_eq!(turns[2].content, "Its population?");
        assert_eq!(conversation.messages().len(), 2);
    }

    #[test]
    fn exchange_keeps_order() {
        let mut conversation = Conversation::default();
        conversation.push_exchange("Q1".into(), "A1".into());
        conversation.push_exchange("Q2".into(), "A2".into());
        let roles: Vec<_> = conversation.messages().iter().map(|m| m.role).collect();
        assert_eq!(
            roles,
            [
                ChatRole::User,
                ChatRole::Assistant,
                ChatRole::User,
                ChatRole::Assistant
            ]
        );
        assert_eq!(conversation.messages()[3].content, "A2");
    }

    #[test]
    fn history_is_capped_by_dropping_oldest_pairs() {
        let mut conversation = Conversation::default();
        for i in 0..(MAX_MESSAGES / 2 + 3) {
            conversation.push_exchange(format!("Q{i}"), format!("A{i}"));
        }
        let messages = conversation.messages();
        assert_eq!(messages.len(), MAX_MESSAGES);
        assert_eq!(messages[0].role, ChatRole::User);
        assert_eq!(messages[0].content, "Q3");
    }

    #[test]
    fn clear_empties_history_but_keeps_prompt() {
        let mut conversation = Conversation::new("Custom");
        conversation.push_exchange("Q".into(), "A".into());
        conversation.clear();
        assert!(conversation.messages().is_empty());
        assert_eq!(conversation.system_prompt(), "Custom");
    }
}
