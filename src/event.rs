mod adapter;
mod kind;

pub use adapter::{EventAdapter, resolve_adapter};
pub use kind::AgentEventKind;

use serde_json::Value;

/// Internal event representation. All fields are pre-extracted by the adapter.
/// The core handler never reads raw JSON or checks agent names.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    SessionStart {
        agent: String,
        cwd: String,
        permission_mode: String,
        source: String,
        agent_id: Option<String>,
        session_id: Option<String>,
    },
    SessionEnd {
        end_reason: String,
    },
    UserPromptSubmit {
        agent: String,
        cwd: String,
        permission_mode: String,
        prompt: String,
        agent_id: Option<String>,
        session_id: Option<String>,
    },
    Notification {
        agent: String,
        cwd: String,
        permission_mode: String,
        wait_reason: String,
        /// When true, only refresh pane metadata without changing status/attention.
        /// Used for events like idle_prompt that carry metadata but should not
        /// trigger a visible status change.
        meta_only: bool,
        agent_id: Option<String>,
        session_id: Option<String>,
    },
    Stop {
        agent: String,
        cwd: String,
        permission_mode: String,
        last_message: String,
        response: Option<String>,
        agent_id: Option<String>,
        session_id: Option<String>,
    },
    StopFailure {
        agent: String,
        cwd: String,
        permission_mode: String,
        error: String,
        agent_id: Option<String>,
        session_id: Option<String>,
    },
    SubagentStart {
        agent_type: String,
        agent_id: Option<String>,
    },
    SubagentStop {
        agent_type: String,
        agent_id: Option<String>,
        last_message: String,
        transcript_path: String,
    },
    ActivityLog {
        tool_name: String,
        tool_input: Value,
        tool_response: Value,
    },
    PermissionDenied {
        agent: String,
        cwd: String,
        permission_mode: String,
        agent_id: Option<String>,
        session_id: Option<String>,
    },
    CwdChanged {
        cwd: String,
        agent_id: Option<String>,
        session_id: Option<String>,
    },
    TaskCreated {
        task_id: String,
        task_subject: String,
    },
    TaskCompleted {
        task_id: String,
        task_subject: String,
    },
    TeammateIdle {
        teammate_name: String,
        team_name: String,
        idle_reason: String,
    },
}

impl AgentEvent {
    /// Project an `AgentEvent` down to its `AgentEventKind` discriminant.
    pub fn kind(&self) -> AgentEventKind {
        match self {
            Self::SessionStart { .. } => AgentEventKind::SessionStart,
            Self::SessionEnd { .. } => AgentEventKind::SessionEnd,
            Self::UserPromptSubmit { .. } => AgentEventKind::UserPromptSubmit,
            Self::Notification { .. } => AgentEventKind::Notification,
            Self::Stop { .. } => AgentEventKind::Stop,
            Self::StopFailure { .. } => AgentEventKind::StopFailure,
            Self::SubagentStart { .. } => AgentEventKind::SubagentStart,
            Self::SubagentStop { .. } => AgentEventKind::SubagentStop,
            Self::ActivityLog { .. } => AgentEventKind::ActivityLog,
            Self::PermissionDenied { .. } => AgentEventKind::PermissionDenied,
            Self::CwdChanged { .. } => AgentEventKind::CwdChanged,
            Self::TaskCreated { .. } => AgentEventKind::TaskCreated,
            Self::TaskCompleted { .. } => AgentEventKind::TaskCompleted,
            Self::TeammateIdle { .. } => AgentEventKind::TeammateIdle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_start_preserves_agent_and_session_ids() {
        let event = AgentEvent::SessionStart {
            agent: "claude".into(),
            cwd: "/tmp".into(),
            permission_mode: "default".into(),
            source: String::new(),
            agent_id: None,
            session_id: None,
        };
        match event {
            AgentEvent::SessionStart {
                agent_id,
                session_id,
                ..
            } => {
                assert!(agent_id.is_none());
                assert!(session_id.is_none());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn task_completed_kind_round_trips() {
        let event = AgentEvent::TaskCompleted {
            task_id: "t1".into(),
            task_subject: "ship".into(),
        };
        assert_eq!(event.kind(), AgentEventKind::TaskCompleted);
    }
}
