#[tokio::test]
async fn deleting_config_preserves_credentials_shared_by_other_scopes() {
    let temp = tempfile::tempdir().unwrap();

    // This integration-test binary contains only this test, and GROK_HOME is
    // set before any config path is resolved.
    unsafe { std::env::set_var("GROK_HOME", temp.path()) };

    let project = temp.path().join("project");
    let project_grok = project.join(".grok");
    std::fs::create_dir_all(&project_grok).unwrap();
    git2::Repository::init(&project).unwrap();

    let user_config_path = temp.path().join("config.toml");
    let config_path = project_grok.join("config.toml");
    let compatibility_config_path = project.join(".mcp.json");
    let credentials_path = temp.path().join("mcp_credentials.json");
    std::fs::write(
        &user_config_path,
        r#"
[mcp_servers.shared]
url = "https://shared.example/mcp"
tool_timeout_sec = 10

[mcp_servers.disabled]
url = "https://disabled.example/mcp"
enabled = false
"#,
    )
    .unwrap();
    std::fs::write(
        compatibility_config_path,
        r#"{"mcpServers":{"github.com":{"url":"https://compat.example/mcp"}}}"#,
    )
    .unwrap();
    std::fs::write(
        &config_path,
        r#"
[mcp_servers.shared]
url = "https://shared.example/mcp"
tool_timeout_sec = 20

[mcp_servers.other]
url = "https://other.example/mcp"
"#,
    )
    .unwrap();
    let credentials = r#"{
  "shared:https://shared.example/mcp": {
    "client_id": "shared-client",
    "token_response": null
  },
  "other:https://other.example/mcp": {
    "client_id": "other-client",
    "token_response": null
  }
}"#;
    std::fs::write(&credentials_path, credentials).unwrap();

    let project_override =
        xai_grok_shell::util::config::get_mcp_server_config_with_project("shared", &project)
            .unwrap();
    assert_eq!(project_override.tool_timeout_sec, Some(20));
    let compatibility_server =
        xai_grok_shell::util::config::get_effective_mcp_server_config("github.com", &project)
            .unwrap();
    assert!(matches!(
        compatibility_server.transport,
        xai_grok_shell::util::config::McpServerTransportConfig::StreamableHttp { url, .. }
            if url == "https://compat.example/mcp"
    ));
    let disabled_server =
        xai_grok_shell::util::config::get_effective_mcp_server_config("disabled", &project)
            .unwrap();
    assert!(!disabled_server.enabled);

    let deleted = xai_grok_shell::util::config::delete_mcp_server_config_at(&config_path, "shared")
        .await
        .unwrap();

    assert!(deleted);
    assert_eq!(
        std::fs::read_to_string(&credentials_path).unwrap(),
        credentials
    );
    let remaining = std::fs::read_to_string(config_path).unwrap();
    assert!(!remaining.contains("mcp_servers.shared"));
    assert!(remaining.contains("mcp_servers.other"));

    // The user-scoped definition survives and still resolves with the retained
    // credential after its project-scoped override is removed.
    assert!(xai_grok_shell::util::config::mcp_server_defined_at(
        &user_config_path,
        "shared"
    ));
    let user_fallback =
        xai_grok_shell::util::config::get_mcp_server_config_with_project("shared", &project)
            .unwrap();
    assert_eq!(user_fallback.tool_timeout_sec, Some(10));

    // Logout is explicit and removes only the selected composite key.
    let shared_url = url::Url::parse("https://shared.example/mcp").unwrap();
    assert!(
        xai_grok_shell::util::config::forget_mcp_credentials("shared", &shared_url)
            .await
            .unwrap()
    );
    let persisted: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(credentials_path).unwrap()).unwrap();
    assert!(persisted.get("shared:https://shared.example/mcp").is_none());
    assert!(persisted.get("other:https://other.example/mcp").is_some());
}
