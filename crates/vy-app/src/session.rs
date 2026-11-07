//! Session connection state and lifecycle transitions.
//!
//! Tracks client connection state machine independently of GPUI.

use vy_proto::{InitializeParams, PROTOCOL_VERSION};

/// Represents the connection state of the client session with the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    /// Disconnected from server, recording number of reconnection attempts made.
    Disconnected { attempts: u32 },
    /// In the process of establishing a connection or waiting for Initialize reply.
    Connecting { generation: u64 },
    /// Session is fully connected and ready for editor operations.
    Ready { generation: u64 },
    /// Session reconnected and restoring panes/buffers/state.
    Restoring { generation: u64 },
}

impl SessionState {
    /// Creates a new initial disconnected state.
    pub fn disconnected() -> Self {
        Self::Disconnected { attempts: 0 }
    }

    /// Returns the current generation, if connected or connecting.
    pub fn generation(&self) -> Option<u64> {
        match self {
            Self::Disconnected { .. } => None,
            Self::Connecting { generation }
            | Self::Ready { generation }
            | Self::Restoring { generation } => Some(*generation),
        }
    }

    /// Whether the session is ready for commands.
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    /// Transition to Connecting for a new connection attempt or immediate reconnection.
    pub fn start_connecting(&mut self, next_generation: u64) {
        *self = Self::Connecting {
            generation: next_generation,
        };
    }

    /// Transition to Restoring when initialize has been acknowledged.
    pub fn mark_restoring(&mut self) {
        if let Self::Connecting { generation } = *self {
            *self = Self::Restoring { generation };
        }
    }

    /// Transition to Ready when session setup/restore is complete.
    pub fn mark_ready(&mut self) {
        if let Self::Restoring { generation } | Self::Connecting { generation } = *self {
            *self = Self::Ready { generation };
        }
    }

    /// Transition to Disconnected upon connection loss or failure.
    pub fn mark_disconnected(&mut self) {
        let attempts = match *self {
            Self::Disconnected { attempts } => attempts.saturating_add(1),
            Self::Connecting { .. } | Self::Ready { .. } | Self::Restoring { .. } => 0,
        };
        *self = Self::Disconnected { attempts };
    }
}

/// Lifecycle state of a plugin runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSlot {
    /// Runtime thread is being spawned or evaluated before pump start.
    Spawning,
    /// Runtime event and request pumps are actively running with the given generation.
    Running { generation: u64 },
    /// Runtime has stopped or was terminated.
    Stopped,
}

impl RuntimeSlot {
    /// Returns the generation if currently running.
    pub fn generation(&self) -> Option<u64> {
        match self {
            Self::Running { generation } => Some(*generation),
            _ => None,
        }
    }

    /// Whether the runtime pump is actively running.
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    /// Transition to Running with an assigned generation.
    pub fn mark_running(&mut self, generation: u64) {
        *self = Self::Running { generation };
    }

    /// Transition to Stopped.
    pub fn mark_stopped(&mut self) {
        *self = Self::Stopped;
    }
}

/// Builds standard `InitializeParams` for connecting to the core server.
pub fn build_initialize_params(workspace_uri: Option<String>) -> InitializeParams {
    InitializeParams {
        protocol_version: PROTOCOL_VERSION,
        client_version: env!("CARGO_PKG_VERSION").into(),
        client_capabilities: vec!["session-restore".to_string()],
        workspace: workspace_uri,
        resume_session: None,
    }
}

/// Alias for `build_initialize_params`.
pub fn initialize_request(workspace_uri: Option<String>) -> InitializeParams {
    build_initialize_params(workspace_uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_state_transitions() {
        let mut state = SessionState::disconnected();
        assert_eq!(state, SessionState::Disconnected { attempts: 0 });
        assert!(!state.is_ready());
        assert_eq!(state.generation(), None);

        // Disconnected failure increments attempts
        state.mark_disconnected();
        assert_eq!(state, SessionState::Disconnected { attempts: 1 });

        // Connect
        state.start_connecting(1);
        assert_eq!(state, SessionState::Connecting { generation: 1 });
        assert_eq!(state.generation(), Some(1));
        assert!(!state.is_ready());

        // Restore
        state.mark_restoring();
        assert_eq!(state, SessionState::Restoring { generation: 1 });
        assert!(!state.is_ready());

        // Ready
        state.mark_ready();
        assert_eq!(state, SessionState::Ready { generation: 1 });
        assert!(state.is_ready());

        // Disconnect resets attempts to 0 initially
        state.mark_disconnected();
        assert_eq!(state, SessionState::Disconnected { attempts: 0 });

        // Consecutive disconnects increment attempts
        state.mark_disconnected();
        assert_eq!(state, SessionState::Disconnected { attempts: 1 });
    }

    #[test]
    fn runtime_slot_transitions() {
        let mut slot = RuntimeSlot::Spawning;
        assert!(!slot.is_running());
        assert_eq!(slot.generation(), None);

        slot.mark_running(42);
        assert!(slot.is_running());
        assert_eq!(slot.generation(), Some(42));

        slot.mark_stopped();
        assert!(!slot.is_running());
        assert_eq!(slot.generation(), None);
    }

    #[test]
    fn initialize_params_builder() {
        let params = build_initialize_params(Some("file:///test/ws".into()));
        assert_eq!(params.protocol_version, PROTOCOL_VERSION);
        assert_eq!(params.workspace, Some("file:///test/ws".into()));
        assert_eq!(params.client_capabilities, vec!["session-restore"]);

        let alias_params = initialize_request(Some("file:///test/ws".into()));
        assert_eq!(params, alias_params);
    }
}
