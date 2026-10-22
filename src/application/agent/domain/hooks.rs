//! Session hooks: a command the harness runs when a session starts, ends a
//! turn and ends. The agent group only renders them; what the command does
//! is up to whoever gives it.

use serde_json::{Value, json};

/// A moment of a session a hook can run at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEvent {
    /// The session starts, or starts again after a clear or a compaction.
    Start,
    /// The agent ends a turn and is about to wait for the user.
    Stop,
    /// The session ends.
    End,
}

impl SessionEvent {
    /// The name Claude Code gives this event in its settings.
    fn claude_name(self) -> &'static str {
        match self {
            Self::Start => "SessionStart",
            Self::Stop => "Stop",
            Self::End => "SessionEnd",
        }
    }
}

/// Longest a hook may run before Claude Code gives up on it, in seconds.
const CLAUDE_HOOK_TIMEOUT: u64 = 10;

/// The settings file Claude Code reads with `--settings`, running `command`
/// at each of `events`. Claude Code loads it on top of the user's own
/// settings, which stay as they are.
pub fn claude_settings(command: &str, events: &[SessionEvent]) -> String {
    let mut hooks = serde_json::Map::new();
    for event in events {
        let handler: Value = json!([{
            "hooks": [{ "type": "command", "command": command, "timeout": CLAUDE_HOOK_TIMEOUT }]
        }]);
        hooks.insert(event.claude_name().to_string(), handler);
    }
    let settings = json!({ "hooks": hooks });
    serde_json::to_string_pretty(&settings).expect("JSON values always serialize") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_settings_run_the_command_at_each_event() {
        // Given a command and the three events
        let events = [SessionEvent::Start, SessionEvent::Stop, SessionEvent::End];

        // When the settings are rendered
        let settings = claude_settings("node /home/casulo/.casulo/memory-hook.mjs", &events);

        // Then each event runs the command
        insta::assert_snapshot!(settings);
    }
}
