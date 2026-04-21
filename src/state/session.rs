use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SessionNamesState {
    /// User-set names from `~/.claude/sessions/*.json` (authoritative when
    /// present — the user explicitly chose this via Claude's `/rename`).
    pub names: HashMap<String, String>,
    /// Fallback names produced by the local-LLM rename feature. Used when
    /// `names` has no entry for a given `session_id`.
    pub generated: HashMap<String, String>,
    pub dirty: bool,
}

impl SessionNamesState {
    pub fn new() -> Self {
        Self {
            names: HashMap::new(),
            generated: HashMap::new(),
            dirty: true,
        }
    }
}

impl Default for SessionNamesState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_dirty_with_empty_maps() {
        let state = SessionNamesState::new();
        assert!(
            state.dirty,
            "fresh state should be dirty so the first refresh runs"
        );
        assert!(state.names.is_empty());
        assert!(state.generated.is_empty());
    }

    #[test]
    fn default_delegates_to_new() {
        let default_state = SessionNamesState::default();
        assert!(default_state.dirty);
        assert!(default_state.names.is_empty());
        assert!(default_state.generated.is_empty());
    }
}
