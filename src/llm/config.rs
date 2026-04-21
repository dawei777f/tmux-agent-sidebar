use std::collections::HashMap;

pub const DEFAULT_TIMEOUT_MS: u64 = 15_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmConfig {
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
    pub auto_rename: bool,
    pub timeout_ms: u64,
}

impl LlmConfig {
    /// Returns `None` when the feature is not configured. The minimum
    /// required opt-in is `@sidebar_llm_endpoint` + `@sidebar_llm_model`.
    pub fn from_tmux_options(opts: &HashMap<String, String>) -> Option<Self> {
        let endpoint = non_empty(opts.get("@sidebar_llm_endpoint"))?;
        let model = non_empty(opts.get("@sidebar_llm_model"))?;
        let api_key = non_empty(opts.get("@sidebar_llm_api_key"));
        let auto_rename = read_bool(opts, "@sidebar_llm_auto_rename").unwrap_or(false);
        let timeout_ms = opts
            .get("@sidebar_llm_timeout_ms")
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|&ms| ms > 0)
            .unwrap_or(DEFAULT_TIMEOUT_MS);

        Some(Self {
            endpoint,
            model,
            api_key,
            auto_rename,
            timeout_ms,
        })
    }
}

fn non_empty(value: Option<&String>) -> Option<String> {
    value
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(String::from)
}

fn read_bool(opts: &HashMap<String, String>, key: &str) -> Option<bool> {
    let raw = opts.get(key)?.trim().to_ascii_lowercase();
    match raw.as_str() {
        "on" | "true" | "1" => Some(true),
        "off" | "false" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn returns_none_when_endpoint_missing() {
        let cfg = LlmConfig::from_tmux_options(&opts(&[("@sidebar_llm_model", "llama3.2")]));
        assert!(cfg.is_none());
    }

    #[test]
    fn returns_none_when_model_missing() {
        let cfg = LlmConfig::from_tmux_options(&opts(&[(
            "@sidebar_llm_endpoint",
            "http://localhost:11434/v1/chat/completions",
        )]));
        assert!(cfg.is_none());
    }

    #[test]
    fn returns_none_when_endpoint_is_whitespace() {
        let cfg = LlmConfig::from_tmux_options(&opts(&[
            ("@sidebar_llm_endpoint", "   "),
            ("@sidebar_llm_model", "llama3.2"),
        ]));
        assert!(cfg.is_none());
    }

    #[test]
    fn minimum_config_uses_defaults() {
        let cfg = LlmConfig::from_tmux_options(&opts(&[
            (
                "@sidebar_llm_endpoint",
                "http://localhost:11434/v1/chat/completions",
            ),
            ("@sidebar_llm_model", "llama3.2:3b"),
        ]))
        .unwrap();
        assert_eq!(cfg.endpoint, "http://localhost:11434/v1/chat/completions");
        assert_eq!(cfg.model, "llama3.2:3b");
        assert!(cfg.api_key.is_none());
        assert!(!cfg.auto_rename);
        assert_eq!(cfg.timeout_ms, DEFAULT_TIMEOUT_MS);
    }

    #[test]
    fn full_config_parses_all_fields() {
        let cfg = LlmConfig::from_tmux_options(&opts(&[
            (
                "@sidebar_llm_endpoint",
                "http://example:8080/v1/chat/completions",
            ),
            ("@sidebar_llm_model", "gpt-4o-mini"),
            ("@sidebar_llm_api_key", "sk-123"),
            ("@sidebar_llm_auto_rename", "on"),
            ("@sidebar_llm_timeout_ms", "5000"),
        ]))
        .unwrap();
        assert_eq!(cfg.api_key.as_deref(), Some("sk-123"));
        assert!(cfg.auto_rename);
        assert_eq!(cfg.timeout_ms, 5_000);
    }

    #[test]
    fn auto_rename_accepts_multiple_truthy_values() {
        for truthy in ["on", "true", "1", "ON", "True"] {
            let cfg = LlmConfig::from_tmux_options(&opts(&[
                ("@sidebar_llm_endpoint", "http://x/v1/chat/completions"),
                ("@sidebar_llm_model", "m"),
                ("@sidebar_llm_auto_rename", truthy),
            ]))
            .unwrap();
            assert!(cfg.auto_rename, "expected {truthy:?} to parse as true");
        }
    }

    #[test]
    fn auto_rename_accepts_multiple_falsy_values() {
        for falsy in ["off", "false", "0", "OFF"] {
            let cfg = LlmConfig::from_tmux_options(&opts(&[
                ("@sidebar_llm_endpoint", "http://x/v1/chat/completions"),
                ("@sidebar_llm_model", "m"),
                ("@sidebar_llm_auto_rename", falsy),
            ]))
            .unwrap();
            assert!(!cfg.auto_rename, "expected {falsy:?} to parse as false");
        }
    }

    #[test]
    fn auto_rename_garbage_is_ignored_and_defaults_false() {
        let cfg = LlmConfig::from_tmux_options(&opts(&[
            ("@sidebar_llm_endpoint", "http://x/v1/chat/completions"),
            ("@sidebar_llm_model", "m"),
            ("@sidebar_llm_auto_rename", "maybe"),
        ]))
        .unwrap();
        assert!(!cfg.auto_rename);
    }

    #[test]
    fn timeout_zero_or_garbage_falls_back_to_default() {
        let cfg = LlmConfig::from_tmux_options(&opts(&[
            ("@sidebar_llm_endpoint", "http://x/v1/chat/completions"),
            ("@sidebar_llm_model", "m"),
            ("@sidebar_llm_timeout_ms", "0"),
        ]))
        .unwrap();
        assert_eq!(cfg.timeout_ms, DEFAULT_TIMEOUT_MS);

        let cfg = LlmConfig::from_tmux_options(&opts(&[
            ("@sidebar_llm_endpoint", "http://x/v1/chat/completions"),
            ("@sidebar_llm_model", "m"),
            ("@sidebar_llm_timeout_ms", "abc"),
        ]))
        .unwrap();
        assert_eq!(cfg.timeout_ms, DEFAULT_TIMEOUT_MS);
    }

    #[test]
    fn empty_api_key_is_treated_as_none() {
        let cfg = LlmConfig::from_tmux_options(&opts(&[
            ("@sidebar_llm_endpoint", "http://x/v1/chat/completions"),
            ("@sidebar_llm_model", "m"),
            ("@sidebar_llm_api_key", "   "),
        ]))
        .unwrap();
        assert!(cfg.api_key.is_none());
    }
}
