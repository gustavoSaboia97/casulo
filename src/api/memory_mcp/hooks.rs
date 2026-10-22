//! The hooks side of the memory server: `POST /hooks/<harness>`, called by
//! the harness when a session starts, stops and ends. The answer is the
//! JSON the harness reads from its hook's output.

use std::sync::PoisonError;

use serde_json::{Value, json};

use super::Server;
use super::http::HttpResponse;
use crate::application::memory::domain::{SessionId, StopDecision};
use crate::application::memory::services::session::{SessionContext, StartKind, end, start, stop};

impl Server {
    /// Answers one hook of `harness` in `session`. Only the event's name,
    /// the start's `source` and the end's `reason` are read from the body:
    /// the rest holds the transcript's path and the last messages, which
    /// are neither needed nor kept. A hook without a session, or an event
    /// this server does not handle, changes nothing.
    pub(super) fn hook(
        &self,
        harness: &str,
        session: Option<&SessionId>,
        body: &[u8],
    ) -> HttpResponse {
        let Ok(message) = serde_json::from_slice::<Value>(body) else {
            return HttpResponse::json(400, json!({ "error": "the body is not valid JSON" }));
        };
        let Some(id) = session else {
            return HttpResponse::json(200, json!({}));
        };
        let context = self.session_context(harness, id);
        let git = self.git.as_ref();
        // See `call_tool` for why a poisoned lock is still safe to use.
        let mut database = self.database.lock().unwrap_or_else(PoisonError::into_inner);
        let field = |name: &str| message.get(name).and_then(Value::as_str);
        let answer = match field("hook_event_name") {
            Some("SessionStart") => {
                // Only a launch starts a new session: a clear, a compaction
                // or a resume goes on with the same run.
                let kind = match field("source") {
                    Some("startup") => StartKind::Startup,
                    _ => StartKind::Continued,
                };
                start(&mut *database, git, &context, kind).map(|()| json!({}))
            }
            Some("Stop") => stop(&mut *database, git, &context).map(|decision| match decision {
                StopDecision::Allow => json!({}),
                StopDecision::Block(reason) => json!({ "decision": "block", "reason": reason }),
            }),
            // A clear ends the conversation, not the run: the SessionStart
            // that follows goes on with the same session.
            Some("SessionEnd") if field("reason") == Some("clear") => Ok(json!({})),
            Some("SessionEnd") => end(&mut *database, &context).map(|()| json!({})),
            _ => Ok(json!({})),
        };
        match answer {
            Ok(answer) => HttpResponse::json(200, answer),
            Err(error) => {
                eprintln!("memory: the {harness} hook failed: {error:#}");
                // The hook prints nothing for an error, so the agent goes on
                // as if there were no hook.
                HttpResponse::json(
                    500,
                    json!({ "error": "the hook failed; see the memory log" }),
                )
            }
        }
    }

    /// The session `id` of `harness`, in this server's checkout, now.
    pub(super) fn session_context(&self, harness: &str, id: &SessionId) -> SessionContext {
        SessionContext {
            id: id.clone(),
            harness: harness.to_string(),
            workspace: self.workspace.clone(),
            at: self.clock.now(),
        }
    }
}
