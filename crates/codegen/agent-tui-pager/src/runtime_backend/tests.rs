use super::*;

#[test]
fn parse_runtime_aliases() {
    assert_eq!(RuntimeBackend::parse("grok"), Some(RuntimeBackend::Grok));
    assert_eq!(RuntimeBackend::parse("xai"), Some(RuntimeBackend::Grok));
    assert_eq!(RuntimeBackend::parse("codex"), Some(RuntimeBackend::Codex));
    assert_eq!(RuntimeBackend::parse("openai"), Some(RuntimeBackend::Codex));
    assert_eq!(
        RuntimeBackend::parse("claude"),
        Some(RuntimeBackend::Claude)
    );
    assert_eq!(RuntimeBackend::parse("lazar"), Some(RuntimeBackend::Lazar));
    assert_eq!(
        RuntimeBackend::parse("hermes"),
        Some(RuntimeBackend::Hermes)
    );
    assert_eq!(
        RuntimeBackend::parse("anthropic"),
        Some(RuntimeBackend::Claude)
    );
    assert_eq!(RuntimeBackend::parse("nope"), None);
}

#[test]
fn compiled_runtime_registry_is_unique_and_round_trips() {
    let mut slugs = std::collections::HashSet::new();
    for runtime in RuntimeBackend::all() {
        assert!(slugs.insert(runtime.as_str()));
        assert_eq!(RuntimeBackend::parse(runtime.as_str()), Some(*runtime));
    }
}

#[test]
fn codex_entries_become_model_state() {
    let entries = vec![
        agent_tui_codex_runtime::CodexModelEntry {
            id: "gpt-5.4".into(),
            display_name: "GPT-5.4".into(),
            description: Some("flagship".into()),
            is_default: true,
            hidden: false,
            default_reasoning_effort: Some("medium".into()),
            supported_reasoning_efforts: vec!["low".into(), "medium".into(), "high".into()],
            input_modalities: vec!["text".into(), "image".into()],
            context_window: Some(272_000),
        },
        agent_tui_codex_runtime::CodexModelEntry {
            id: "hidden-x".into(),
            display_name: "Hidden".into(),
            description: None,
            is_default: false,
            hidden: true,
            default_reasoning_effort: None,
            supported_reasoning_efforts: vec![],
            input_modalities: vec!["text".into()],
            context_window: None,
        },
    ];
    let state = model_state_from_codex_entries(&entries, None);
    assert_eq!(state.available.len(), 1);
    assert_eq!(state.current_model_id_str(), Some("gpt-5.4"));
    assert_eq!(state.current_model_name().as_deref(), Some("GPT-5.4"));
    assert_eq!(state.get_context_window(), Some(272_000));
}

#[test]
fn claude_context_window_tokens_honors_1m_suffix() {
    assert_eq!(
        claude_context_window_tokens("claude-opus-4-8[1m]"),
        1_000_000
    );
    assert_eq!(claude_context_window_tokens("claude-opus-4-8"), 200_000);
    assert_eq!(claude_context_window_tokens("sonnet"), 200_000);
}

#[test]
fn codex_heuristic_window_when_list_omits_field() {
    let entries = vec![agent_tui_codex_runtime::CodexModelEntry {
        id: "gpt-5.4".into(),
        display_name: "GPT-5.4".into(),
        description: None,
        is_default: true,
        hidden: false,
        default_reasoning_effort: None,
        supported_reasoning_efforts: vec![],
        input_modalities: vec!["text".into()],
        context_window: None,
    }];
    let state = model_state_from_codex_entries(&entries, None);
    assert_eq!(state.get_context_window(), Some(200_000));
}
