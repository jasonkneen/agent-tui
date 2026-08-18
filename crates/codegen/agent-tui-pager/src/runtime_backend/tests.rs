use super::*;
use agent_client_protocol as acp;
use indexmap::IndexMap;

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
fn vendor_catalogs_do_not_clobber_each_other() {
    let _lock = catalog_test_lock();
    reset_runtime_catalogs_for_test();

    let mut codex = crate::acp::model_state::ModelState::default();
    let cid = acp::ModelId::new(std::sync::Arc::from("gpt-5.4"));
    let mut cmap = IndexMap::new();
    cmap.insert(cid.clone(), acp::ModelInfo::new(cid.clone(), "GPT-5.4"));
    codex.update_catalog(cmap);
    codex.set_current(cid, None);
    store_vendor_catalog(RuntimeBackend::Codex, codex);

    let mut claude = crate::acp::model_state::ModelState::default();
    let aid = acp::ModelId::new(std::sync::Arc::from("opus"));
    let mut amap = IndexMap::new();
    amap.insert(aid.clone(), acp::ModelInfo::new(aid.clone(), "Opus"));
    claude.update_catalog(amap);
    claude.set_current(aid, None);
    store_vendor_catalog(RuntimeBackend::Claude, claude);

    assert_eq!(
        vendor_catalog(RuntimeBackend::Codex)
            .and_then(|s| s.current_model_id_str().map(str::to_string))
            .as_deref(),
        Some("gpt-5.4")
    );
    assert_eq!(
        vendor_catalog(RuntimeBackend::Claude)
            .and_then(|s| s.current_model_id_str().map(str::to_string))
            .as_deref(),
        Some("opus")
    );
    assert!(
        !vendor_refresh_applies(RuntimeBackend::Codex, RuntimeBackend::Claude),
        "a late Codex fetch must not paint over Claude"
    );
    assert!(vendor_refresh_applies(
        RuntimeBackend::Claude,
        RuntimeBackend::Claude
    ));
}

#[test]
fn chrome_after_switch_uses_that_vendor_not_the_other() {
    let _lock = catalog_test_lock();
    reset_runtime_catalogs_for_test();

    let mut claude = crate::acp::model_state::ModelState::default();
    let aid = acp::ModelId::new(std::sync::Arc::from("sonnet"));
    let mut amap = IndexMap::new();
    amap.insert(
        aid.clone(),
        acp::ModelInfo::new(aid.clone(), "Sonnet"),
    );
    claude.update_catalog(amap);
    claude.set_current(aid, None);
    store_vendor_catalog(RuntimeBackend::Claude, claude);

    let chrome = chrome_after_runtime_switch(RuntimeBackend::Claude);
    assert_eq!(chrome.current_model_id_str(), Some("sonnet"));

    replace_stashed_grok_catalog({
        let mut grok = crate::acp::model_state::ModelState::default();
        let gid = acp::ModelId::new(std::sync::Arc::from("grok-4.5"));
        let mut gmap = IndexMap::new();
        gmap.insert(
            gid.clone(),
            acp::ModelInfo::new(gid.clone(), "Grok 4.5"),
        );
        grok.update_catalog(gmap);
        grok.set_current(gid, None);
        grok
    });
    let grok_chrome = chrome_after_runtime_switch(RuntimeBackend::Grok);
    assert_eq!(grok_chrome.current_model_id_str(), Some("grok-4.5"));
}

#[test]
fn grok_stash_if_empty_does_not_clobber_leave_stash() {
    let _lock = catalog_test_lock();
    reset_runtime_catalogs_for_test();

    let mut first = crate::acp::model_state::ModelState::default();
    let a = acp::ModelId::new(std::sync::Arc::from("grok-keep"));
    first
        .available
        .insert(a.clone(), acp::ModelInfo::new(a.clone(), "Keep"));
    first.set_current(a, None);
    replace_stashed_grok_catalog(first);

    let mut second = crate::acp::model_state::ModelState::default();
    let b = acp::ModelId::new(std::sync::Arc::from("grok-other"));
    second
        .available
        .insert(b.clone(), acp::ModelInfo::new(b.clone(), "Other"));
    second.set_current(b, None);
    stash_grok_catalog(second);

    assert_eq!(
        stashed_grok_catalog()
            .and_then(|s| s.current_model_id_str().map(str::to_string))
            .as_deref(),
        Some("grok-keep")
    );
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
