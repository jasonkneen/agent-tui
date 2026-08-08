//! `GROK_HOME` override tests in an isolated binary so `grok_home()`'s
//! process-wide `OnceLock` initializes from the overridden env var.

use std::path::PathBuf;

#[test]
#[serial_test::serial(GROK_HOME)]
fn grok_home_override_path_helpers() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let grok_home = tmp.path().to_path_buf();
    unsafe {
        std::env::set_var("GROK_HOME", &grok_home);
    }

    assert_eq!(
        agent_tui_pager::util::pager_toml_path(),
        grok_home.join("pager.toml")
    );
    assert_eq!(
        agent_tui_pager::util::display_grok_home_prefix(),
        "$GROK_HOME"
    );
    assert_eq!(
        agent_tui_pager::util::display_user_grok_path("config.toml"),
        "$GROK_HOME/config.toml"
    );

    let memory_path = grok_home.join("memory/MEMORY.md");
    assert_eq!(
        agent_tui_pager::util::abbreviate_path(&memory_path.display().to_string()),
        "$GROK_HOME/memory/MEMORY.md"
    );

    assert!(agent_tui_pager::util::is_under_user_grok_home(&memory_path));
    assert!(!agent_tui_pager::util::is_under_user_grok_home(
        PathBuf::from("/tmp/other").as_path()
    ));
}

/// Isolated because `grok_home()`'s `OnceLock` is already initialized by the
/// time the shared lib-test binary reaches a case like this.
#[test]
#[serial_test::serial(GROK_HOME)]
fn disk_usage_run_creates_no_grok_home() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ghost = tmp.path().join("ghost-home");
    unsafe {
        std::env::set_var("GROK_HOME", &ghost);
    }

    for json in [false, true] {
        agent_tui_pager::disk_usage_cmd::run(agent_tui_pager::disk_usage_cmd::DiskUsageArgs { json })
            .expect("a missing home is not an error");
        assert!(
            !ghost.exists(),
            "grok du must not create the home it reports on (json={json})"
        );
    }
}
