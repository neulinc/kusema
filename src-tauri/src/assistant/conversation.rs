use crate::llm_client::{ChatImage, ChatRole, ChatTurn};

/// Default instructions for Assistant mode. Replies are meant to be read
/// aloud, so the model is steered away from formatting that doesn't speak.
pub const DEFAULT_SYSTEM_PROMPT: &str = "You are Kusema, a helpful voice assistant. \
Answers will be spoken aloud, so reply in short, natural spoken sentences. \
No markdown, lists, code blocks or emoji unless asked.";

/// Added to the system prompt once the conversation includes a screenshot.
pub const SCREEN_PROMPT: &str = "The user may share a screenshot of the window they are looking at. Answer their question about it directly. Describe what you see only as far as the answer needs it, and keep spoken answers to a few sentences; the user will ask if they want more detail.";

/// Stands in for a screenshot that has since been dropped, so the model knows
/// it was there but can no longer see it.
const DROPPED_SCREENSHOT_NOTE: &str = "[An earlier screenshot, no longer attached]";

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

    /// Messages, oldest first.
    pub fn messages(&self) -> &[ChatTurn] {
        &self.messages
    }

    /// Whether any question so far was about a screenshot. Such a
    /// conversation stays with the screen-question provider, as the answers
    /// describe what was on screen.
    pub fn has_screenshot(&self) -> bool {
        self.messages.iter().any(|m| m.screenshot)
    }

    /// The system prompt for a request made of `turns`.
    pub fn prompt_for(&self, turns: &[ChatTurn]) -> String {
        if turns.iter().any(|t| t.screenshot) {
            format!("{} {SCREEN_PROMPT}", self.system_prompt)
        } else {
            self.system_prompt.clone()
        }
    }

    /// The history plus a new question, as it should be sent to the model.
    /// The latest screenshot stays attached, so follow-ups can refer to it.
    pub fn with_question(&self, question: &str) -> Vec<ChatTurn> {
        let mut turns = self.messages.clone();
        turns.push(ChatTurn::new(ChatRole::User, question));
        for_request(turns)
    }

    /// The history plus a new question about `image`, which replaces any
    /// earlier screenshot.
    pub fn with_screen_question(&self, question: &str, image: ChatImage) -> Vec<ChatTurn> {
        let mut turns = self.messages.clone();
        drop_images(&mut turns);
        turns.push(screen_turn(question.to_string(), image));
        for_request(turns)
    }

    /// Record a finished exchange. Both halves are added together, so a failed
    /// or cancelled request never leaves an unanswered question behind.
    pub fn push_exchange(&mut self, question: String, answer: String) {
        self.push(ChatTurn::new(ChatRole::User, question), answer);
    }

    /// Record a finished exchange about `image`. Older screenshots are dropped
    /// so every later request doesn't pay for them again.
    pub fn push_screen_exchange(&mut self, question: String, answer: String, image: ChatImage) {
        drop_images(&mut self.messages);
        self.push(screen_turn(question, image), answer);
    }

    fn push(&mut self, question: ChatTurn, answer: String) {
        self.messages.push(question);
        self.messages
            .push(ChatTurn::new(ChatRole::Assistant, answer));
        let excess = self.messages.len().saturating_sub(MAX_MESSAGES);
        // Drop whole pairs so the history keeps starting with a question.
        self.messages.drain(..excess.next_multiple_of(2));
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }
}

fn screen_turn(question: String, image: ChatImage) -> ChatTurn {
    ChatTurn {
        screenshot: true,
        image: Some(image),
        ..ChatTurn::new(ChatRole::User, question)
    }
}

fn drop_images(turns: &mut [ChatTurn]) {
    for turn in turns {
        turn.image = None;
    }
}

/// Detach screenshots `provider_id` wasn't approved for (the setting changed
/// since), so an image only ever goes where the user agreed to send it.
pub fn keep_images_for(mut turns: Vec<ChatTurn>, provider_id: &str) -> Vec<ChatTurn> {
    for turn in &mut turns {
        if turn
            .image
            .as_ref()
            .is_some_and(|i| i.approved_for != provider_id)
        {
            turn.image = None;
        }
    }
    for_request(turns)
}

/// Mark questions whose screenshot is no longer attached.
fn for_request(mut turns: Vec<ChatTurn>) -> Vec<ChatTurn> {
    for turn in &mut turns {
        if turn.screenshot
            && turn.image.is_none()
            && !turn.content.starts_with(DROPPED_SCREENSHOT_NOTE)
        {
            turn.content = format!("{DROPPED_SCREENSHOT_NOTE} {}", turn.content);
        }
    }
    turns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_uses_spoken_system_prompt() {
        let conversation = Conversation::default();
        assert_eq!(conversation.prompt_for(&[]), DEFAULT_SYSTEM_PROMPT);
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

    fn image(tag: u8) -> ChatImage {
        ChatImage {
            media_type: "image/jpeg",
            data: std::sync::Arc::from(&[tag][..]),
            approved_for: "anthropic".to_string(),
        }
    }

    fn attached(turns: &[ChatTurn]) -> Vec<Option<u8>> {
        turns
            .iter()
            .map(|t| t.image.as_ref().map(|i| i.data[0]))
            .collect()
    }

    #[test]
    fn newer_screenshot_drops_older_ones() {
        let mut conversation = Conversation::default();
        conversation.push_screen_exchange("Q1".into(), "A1".into(), image(1));
        conversation.push_exchange("Follow-up".into(), "A2".into());
        conversation.push_screen_exchange("Q3".into(), "A3".into(), image(3));

        let messages = conversation.messages();
        assert_eq!(attached(messages), [None, None, None, None, Some(3), None]);
        // The earlier question still shows it had a screenshot.
        assert!(messages[0].screenshot);
        assert!(!messages[2].screenshot);
        assert_eq!(messages[0].content, "Q1");
    }

    #[test]
    fn follow_up_keeps_latest_screenshot_attached() {
        let mut conversation = Conversation::default();
        conversation.push_screen_exchange("What is this?".into(), "A form.".into(), image(1));
        let turns = conversation.with_question("Summarise the second paragraph");
        assert_eq!(attached(&turns), [Some(1), None, None]);
        assert_eq!(turns[2].content, "Summarise the second paragraph");
        assert!(conversation.has_screenshot());
    }

    #[test]
    fn new_screen_question_sends_only_its_own_screenshot() {
        let mut conversation = Conversation::default();
        conversation.push_screen_exchange("Q1".into(), "A1".into(), image(1));
        let turns = conversation.with_screen_question("Q2", image(2));
        assert_eq!(attached(&turns), [None, None, Some(2)]);
        assert!(turns[0].content.starts_with(DROPPED_SCREENSHOT_NOTE));
        assert!(turns[0].content.ends_with("Q1"));
        // The stored history is unchanged until the answer arrives.
        assert_eq!(attached(conversation.messages()), [Some(1), None]);
        assert_eq!(conversation.messages()[0].content, "Q1");
    }

    #[test]
    fn screenshots_only_go_to_the_approved_provider() {
        let mut conversation = Conversation::default();
        conversation.push_screen_exchange("Q1".into(), "A1".into(), image(1));
        let turns = conversation.with_question("Q2");
        assert_eq!(
            attached(&keep_images_for(turns.clone(), "anthropic")),
            [Some(1), None, None]
        );
        let elsewhere = keep_images_for(turns, "openai");
        assert_eq!(attached(&elsewhere), [None, None, None]);
        assert!(elsewhere[0].content.starts_with(DROPPED_SCREENSHOT_NOTE));
    }

    #[test]
    fn screen_prompt_is_added_only_with_screenshots() {
        let mut conversation = Conversation::default();
        let plain = conversation.with_question("Hi");
        assert_eq!(conversation.prompt_for(&plain), DEFAULT_SYSTEM_PROMPT);
        let screen = conversation.with_screen_question("What's this?", image(1));
        assert!(conversation.prompt_for(&screen).ends_with(SCREEN_PROMPT));
        conversation.push_screen_exchange("What's this?".into(), "A chart.".into(), image(1));
        let follow_up = conversation.with_question("And the axis?");
        assert!(conversation.prompt_for(&follow_up).ends_with(SCREEN_PROMPT));
    }

    #[test]
    fn clear_empties_history_but_keeps_prompt() {
        let mut conversation = Conversation::new("Custom");
        conversation.push_exchange("Q".into(), "A".into());
        conversation.clear();
        assert!(conversation.messages().is_empty());
        assert_eq!(conversation.prompt_for(&[]), "Custom");
    }
}
