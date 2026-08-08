//! E2E enforcement tests for kernel-enforced profile `deny` paths.
//!
//! Drives the GENERIC path-deny primitive via a custom sandbox profile whose
//! `deny` list names concrete files. `SandboxManager::apply` is process-wide and
//! irreversible, so kernel enforcement is verified in an isolated subprocess.
//!
//! On Linux, read-deny requires bwrap bind-over; the subprocess re-execs inside
//! bwrap when `bwrap` is available. macOS uses Seatbelt platform rules directly
//! via `SandboxManager::apply`.

#![cfg(all(unix, feature = "enforce"))]

use std::fs;
use std::path::Path;
use std::process::Command;

const SCENARIO_ENV: &str = "SANDBOX_E2E_SCENARIO";
const WORKSPACE_ENV: &str = "SANDBOX_E2E_WORKSPACE";
/// Custom profile name, comma-joined deny targets, and comma-joined control
/// files, passed to the subprocess so one entry point drives every deny case
/// (exact paths and globs alike).
const PROFILE_ENV: &str = "SANDBOX_E2E_PROFILE";
const TARGETS_ENV: &str = "SANDBOX_E2E_TARGETS";
const CONTROLS_ENV: &str = "SANDBOX_E2E_CONTROLS";
/// Paths NOT present at apply time that match a deny glob; the macOS runtime
/// regex must deny creating them post-launch (the differentiator vs exact paths).
const POSTLAUNCH_ENV: &str = "SANDBOX_E2E_POSTLAUNCH";
const MARKER: &str = "deny-paths-e2e-marker-9f3c1a";

/// Re-invoke this test binary as a subprocess driving `profile` over `targets`
/// (denied) and `controls` (must stay readable). `postlaunch` paths are created
/// AFTER apply to exercise the macOS runtime-regex (post-launch) coverage.
fn run_scenario(
    workspace: &Path,
    profile: &str,
    targets: &[&str],
    controls: &[&str],
    postlaunch: &[&str],
) -> (std::process::ExitStatus, String) {
    let exe = std::env::current_exe().expect("current_exe");
    let output = Command::new(exe)
        .env(SCENARIO_ENV, "block_deny")
        .env(WORKSPACE_ENV, workspace.as_os_str())
        .env(PROFILE_ENV, profile)
        .env(TARGETS_ENV, targets.join(","))
        .env(CONTROLS_ENV, controls.join(","))
        .env(POSTLAUNCH_ENV, postlaunch.join(","))
        .arg("--ignored")
        .arg("--exact")
        .arg("--nocapture")
        .arg("subprocess_entry")
        .output()
        .expect("failed to spawn subprocess");
    // All assertions read stderr; the subprocess prints only diagnostics there.
    (
        output.status,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Decode a comma-joined env list (empty/missing -> empty vec).
fn list_from_env(key: &str) -> Vec<String> {
    std::env::var(key)
        .ok()
        .map(|v| {
            v.split(',')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

// EROFS too: a root writer on Linux bypasses the mode-000 DAC check via
// CAP_DAC_OVERRIDE and hits the read-only bind-mount instead — still a denial.
fn is_permission_denied(e: &std::io::Error) -> bool {
    matches!(
        e.raw_os_error(),
        Some(libc::EACCES) | Some(libc::EPERM) | Some(libc::EROFS)
    )
}

/// Spawn a child command and `exit(1)` if its stdout exposes the secret MARKER.
/// Asserts marker-absence rather than a non-zero exit: a root reader of the
/// mode-000 placeholder gets empty output, which still means the path is shadowed.
fn assert_child_cannot_read(label: &str, program: &str, args: &[&str]) {
    let out = Command::new(program)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn {program}: {e}"));
    if String::from_utf8_lossy(&out.stdout).contains(MARKER) {
        eprintln!("FAIL: {label} exposed MARKER");
        std::process::exit(1);
    }
}

/// Assert a denied file's bytes are unreadable via an in-process read, a `cat`
/// child (the `bash`/`grep` tools), and a nested `sh -c "cat"` child (the shell a
/// subagent shells out through). The property is MARKER-absence (EACCES/EPERM, or
/// empty output under root, all satisfy it).
fn assert_read_blocked(label: &str, path: &Path) {
    if let Ok(content) = fs::read_to_string(path)
        && content.contains(MARKER)
    {
        eprintln!("FAIL: {label} in-process read exposed MARKER");
        std::process::exit(1);
    }
    let s = path.display().to_string();
    assert_child_cannot_read(label, "cat", &[s.as_str()]);
    let sh_cmd = format!("cat '{s}'");
    assert_child_cannot_read(label, "sh", &["-c", sh_cmd.as_str()]);
    eprintln!("OK: {label} read blocked");
}

/// Assert a denied file cannot be overwritten (write must EACCES/EPERM, not
/// succeed — a permitted write would enable the relocation bypass below).
fn assert_write_denied(label: &str, path: &Path) {
    match fs::write(path, "overwrite-attempt") {
        Err(e) if is_permission_denied(&e) => eprintln!("OK: {label} write denied"),
        Err(e) => {
            eprintln!("FAIL: unexpected {label} write error: {e}");
            std::process::exit(1);
        }
        Ok(()) => {
            eprintln!("FAIL: {label} write was permitted (relocation bypass possible)");
            std::process::exit(1);
        }
    }
}

/// Assert the `mv x y && cat y` relocation bypass does not expose the bytes:
/// the rename must fail (unlink of the source is denied) so the moved copy never
/// materializes with the secret.
fn assert_rename_bypass_blocked(label: &str, path: &Path, workspace: &Path) {
    let name = path.file_name().unwrap().to_string_lossy();
    let moved = workspace.join(format!("exfil-{name}"));
    let _ = fs::rename(path, &moved); // expected to fail; bytes must not leak
    match fs::read_to_string(&moved) {
        Ok(c) if c.contains(MARKER) => {
            eprintln!("FAIL: {label} rename bypass exposed MARKER");
            std::process::exit(1);
        }
        _ => eprintln!("OK: {label} rename bypass blocked"),
    }
}

#[cfg(target_os = "linux")]
fn bwrap_available() -> bool {
    // `--version` only checks the binary exists; remote CI may have bwrap but
    // deny user namespace creation ("Creating new namespace failed: Operation not permitted").
    Command::new("bwrap")
        .args(["--bind", "/", "/", "--", "true"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The custom profile under test, read from the env the parent set.
fn profile_from_env() -> agent_tui_sandbox::ProfileName {
    agent_tui_sandbox::ProfileName::Custom(std::env::var(PROFILE_ENV).expect(PROFILE_ENV))
}

// ── Subprocess entry point ──────────────────────────────────────────────

/// `#[ignore]`d — only runs when invoked by the parent test via `run_scenario`.
#[test]
#[ignore]
fn subprocess_entry() {
    let scenario = match std::env::var(SCENARIO_ENV) {
        Ok(s) => s,
        Err(_) => return,
    };
    let workspace = std::env::var(WORKSPACE_ENV).expect(WORKSPACE_ENV);
    let workspace = dunce::canonicalize(&workspace).expect("canonicalize workspace");
    let workspace = workspace.as_path();
    let targets = list_from_env(TARGETS_ENV);
    let controls = list_from_env(CONTROLS_ENV);

    #[cfg(target_os = "linux")]
    {
        if !agent_tui_sandbox::is_inside_bwrap() {
            // Drive the REAL routing the shell uses at startup — computing the
            // custom profile's deny set (exact paths AND launch-time glob
            // expansion), building placeholders, and failing closed on a partial
            // bind — rather than hand-rolling a single-path `bwrap_reexec_command`.
            match agent_tui_sandbox::bwrap_reexec_for_profile(&profile_from_env(), workspace) {
                Some(mut cmd) => {
                    use std::os::unix::process::CommandExt;
                    let err = cmd.exec(); // returns only if exec failed
                    eprintln!("bwrap re-exec failed: {err}");
                    std::process::exit(2);
                }
                // Outside bwrap with no command means the read-deny set could not
                // be secured. The shell fails closed here; mirror that.
                None => {
                    eprintln!("FAIL: bwrap_reexec_for_profile returned None outside bwrap");
                    std::process::exit(2);
                }
            }
        }
    }

    match scenario.as_str() {
        "block_deny" => {
            let mut sandbox = agent_tui_sandbox::SandboxManager::new(profile_from_env(), workspace);
            if let Err(e) = sandbox.apply(workspace) {
                eprintln!("sandbox apply failed: {e}");
                std::process::exit(3);
            }
            if !sandbox.is_applied() {
                eprintln!("sandbox was not applied (unsupported platform?)");
                std::process::exit(4);
            }

            // Each denied target must be read-, write-, and rename-denied — via the
            // read_file tool (in-process), `bash`/`grep` (cat child), and the shell
            // a subagent uses (sh -c child). Targets exercise nested glob matches
            // (`sub/dir/key.pem`) and the denied-directory (subpath) path alike.
            for rel in &targets {
                let path = workspace.join(rel);
                assert_read_blocked(rel, &path);
                assert_write_denied(rel, &path);
                assert_rename_bypass_blocked(rel, &path, workspace);
            }

            // Non-denied control files (incl. a sibling of a glob match) stay readable.
            for rel in &controls {
                match fs::read_to_string(workspace.join(rel)) {
                    Ok(c) if c.contains("hello") => eprintln!("OK: {rel} control readable"),
                    Ok(_) => {
                        eprintln!("FAIL: control {rel} readable but missing marker");
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("FAIL: control {rel} should stay readable: {e}");
                        std::process::exit(1);
                    }
                }
            }

            // macOS-only: the runtime regex denies paths that match a glob even
            // when created AFTER apply — the differentiator vs the exact-path flow
            // (and the macOS-airtight half of the documented asymmetry). On Linux
            // post-launch matches are best-effort and NOT covered, so skip there.
            #[cfg(target_os = "macos")]
            for rel in list_from_env(POSTLAUNCH_ENV) {
                match fs::write(workspace.join(&rel), MARKER) {
                    Err(e) if is_permission_denied(&e) => {
                        eprintln!("OK: {rel} post-launch write denied")
                    }
                    Err(e) => {
                        eprintln!("FAIL: unexpected {rel} post-launch write error: {e}");
                        std::process::exit(1);
                    }
                    Ok(()) => {
                        eprintln!("FAIL: {rel} post-launch matching path was writable");
                        std::process::exit(1);
                    }
                }
            }
            // A NON-matching post-launch path must still be writable — proves the
            // denial above is the glob, not a blanket workspace write-deny.
            #[cfg(target_os = "macos")]
            if !list_from_env(POSTLAUNCH_ENV).is_empty() {
                match fs::write(workspace.join("late-control.txt"), "hello") {
                    Ok(()) => eprintln!("OK: post-launch control writable"),
                    Err(e) => {
                        eprintln!("FAIL: non-matching post-launch path should be writable: {e}");
                        std::process::exit(1);
                    }
                }
            }

            std::process::exit(0);
        }
        other => {
            eprintln!("unknown scenario: {other}");
            std::process::exit(99);
        }
    }
}

// ── Parent test cases ───────────────────────────────────────────────────

/// Drive one deny case end-to-end: define a custom profile whose `deny` list is
/// `deny_entries` (exact paths and/or globs), create each `target` (with the
/// MARKER) and each `control` (readable), then assert in an isolated subprocess
/// that every target is read/write/rename-denied and every control stays
/// readable. Shared by the exact-path and glob cases.
fn run_deny_case(
    tag: &str,
    profile: &str,
    deny_entries: &[&str],
    targets: &[&str],
    controls: &[&str],
    postlaunch: &[&str],
) {
    // When set, missing prerequisites must FAIL loudly instead of skipping, so a
    // CI lane can guarantee the deny enforcement is actually exercised.
    let require = std::env::var("SANDBOX_E2E_REQUIRE_ENFORCEMENT").is_ok();

    let support = agent_tui_sandbox::SandboxManager::support_info();
    if !support.is_supported {
        if require {
            panic!(
                "SANDBOX_E2E_REQUIRE_ENFORCEMENT set but sandbox unsupported: {}",
                support.details
            );
        }
        eprintln!("skipping: sandbox not supported ({})", support.details);
        return;
    }

    #[cfg(target_os = "linux")]
    if !bwrap_available() {
        if require {
            panic!(
                "SANDBOX_E2E_REQUIRE_ENFORCEMENT set but bwrap unavailable (required for Linux read-deny)"
            );
        }
        eprintln!("skipping: bwrap not installed (required for Linux read-deny)");
        return;
    }

    let tmp = std::env::temp_dir().join(format!(
        "grok-sandbox-e2e-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&tmp).expect("create temp workspace");
    let tmp = dunce::canonicalize(&tmp).expect("canonicalize temp workspace");
    let _cleanup = TempDirGuard(tmp.clone());

    // Define the custom profile whose `deny` list holds the entries under test.
    let deny_list = deny_entries
        .iter()
        .map(|p| format!("\"{p}\""))
        .collect::<Vec<_>>()
        .join(", ");
    fs::create_dir_all(tmp.join(".grok")).expect("mkdir .grok");
    fs::write(
        tmp.join(".grok").join("sandbox.toml"),
        format!("[profiles.{profile}]\nextends = \"workspace\"\ndeny = [{deny_list}]\n"),
    )
    .expect("write sandbox.toml");

    // Create each denied target with the MARKER (parents created as needed, e.g.
    // `sub/dir/` for a nested glob match, `secretdir/` for a denied directory)
    // plus each readable control.
    for rel in targets {
        let path = tmp.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir denied parent");
        }
        fs::write(&path, format!("SECRET={MARKER}")).expect("write denied file");
    }
    for rel in controls {
        let path = tmp.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir control parent");
        }
        fs::write(&path, "hello workspace").expect("write control");
    }

    let (status, stderr) = run_scenario(&tmp, profile, targets, controls, postlaunch);
    assert!(
        status.success(),
        "[{tag}] custom-profile deny should block read/write/rename\nstderr: {stderr}"
    );
    for rel in targets {
        assert!(
            stderr.contains(&format!("OK: {rel} read blocked")),
            "[{tag}] expected '{rel}' read block confirmation\nstderr: {stderr}"
        );
        assert!(
            stderr.contains(&format!("OK: {rel} write denied")),
            "[{tag}] expected '{rel}' write to be denied\nstderr: {stderr}"
        );
        assert!(
            stderr.contains(&format!("OK: {rel} rename bypass blocked")),
            "[{tag}] expected '{rel}' rename bypass to be blocked\nstderr: {stderr}"
        );
    }
    for rel in controls {
        assert!(
            stderr.contains(&format!("OK: {rel} control readable")),
            "[{tag}] expected non-denied control '{rel}' to stay readable\nstderr: {stderr}"
        );
    }
    // The post-launch (runtime-regex) coverage is macOS-only; Linux best-effort
    // expansion does not cover files created after launch.
    #[cfg(target_os = "macos")]
    for rel in postlaunch {
        assert!(
            stderr.contains(&format!("OK: {rel} post-launch write denied")),
            "[{tag}] expected post-launch matching '{rel}' to be write-denied\nstderr: {stderr}"
        );
    }
    #[cfg(target_os = "macos")]
    if !postlaunch.is_empty() {
        assert!(
            stderr.contains("OK: post-launch control writable"),
            "[{tag}] expected non-matching post-launch path to stay writable\nstderr: {stderr}"
        );
    }
}

#[test]
fn deny_exact_paths_block_read_write_rename() {
    // Exact-path entries: two files plus a directory (exercised via a file inside
    // it), covering the literal-file and the subpath / Linux dir-placeholder paths.
    run_deny_case(
        "exact",
        "denytest",
        &[".env", "src/server.pem", "secretdir"],
        &[".env", "src/server.pem", "secretdir/inner.pem"],
        &["readable.txt"],
        &[], // exact paths have no runtime/post-launch coverage to assert
    );
}

#[test]
fn deny_globs_block_read_write_rename() {
    // Glob entries exercising: nested `*.pem`, a `.env` at root AND nested, and a
    // trailing-`**` prefix dir. The control inside a matched directory
    // (`sub/dir/keep.txt`) proves the glob denies only matches, not the whole tree.
    // `postlaunch` (`late.pem`) pins the macOS runtime-regex post-launch coverage.
    run_deny_case(
        "glob",
        "denyglob",
        &["**/*.pem", "**/.env", "secrets/**"],
        &["sub/dir/key.pem", ".env", "sub/.env", "secrets/inner.key"],
        &["readable.txt", "sub/dir/keep.txt"],
        &["late.pem"],
    );
}

/// Hard-linked registry file must refuse sandbox startup (writable alias).
#[test]
fn hardlinked_hooks_paths_refuses_startup() {
    if skip_if_enforcement_unavailable() {
        return;
    }
    let (home, grok, workspace, _ch, _cg, _cw) = fixture_homes("hook-hl");
    fs::create_dir_all(grok.join("hooks")).unwrap();
    let reg = grok.join("hooks-paths");
    let alias = grok.join("hooks-paths-alias");
    fs::write(&reg, b"").unwrap();
    fs::hard_link(&reg, &alias).unwrap();

    let (status, stderr) =
        run_hook_write_deny_scenario(&home, &grok, &workspace, "hook_write_deny");
    assert!(
        !status.success(),
        "hard-linked hooks-paths must refuse startup\nstderr: {stderr}"
    );
    // Plan/materialization path should surface hard-link or identity failure.
    assert!(
        stderr.contains("hard-link")
            || stderr.contains("HardLink")
            || stderr.contains("hook write-deny")
            || stderr.contains("nlink"),
        "expected hard-link refusal signal\nstderr: {stderr}"
    );
}

/// Workspace profile: Grok-owned direct hook sources are write-denied but readable;
/// create / overwrite / unlink / rename / mkdir fail; absolute hooks-paths
/// targets are denied; parent rename is blocked; Grok/CWD/temp siblings stay writable.
#[test]
fn workspace_protects_direct_hook_sources() {
    if skip_if_enforcement_unavailable() {
        return;
    }

    let (home, grok, workspace, _ch, _cg, _cw) = fixture_homes("hook");

    fs::create_dir_all(grok.join("hooks")).expect("mkdir hooks");
    fs::write(grok.join("hooks").join("keep.json"), r#"{"keep-me":true}"#)
        .expect("write keep.json");
    let dynamic = grok.join("sessions").join("extra-hooks");
    fs::create_dir_all(&dynamic).expect("mkdir dynamic hooks target");
    fs::write(dynamic.join("x.json"), r#"{"x":1}"#).expect("write dynamic hook");
    // Configured target under workspace (absolute) for grant-root ancestor pins.
    let ws_hooks = workspace.join("extra-parent").join("vendor-hooks");
    fs::create_dir_all(&ws_hooks).expect("mkdir ws vendor hooks");
    fs::write(ws_hooks.join("x.json"), r#"{"x":1}"#).expect("write ws hook");
    fs::write(
        grok.join("hooks-paths"),
        format!("{}\n{}\n", dynamic.display(), ws_hooks.display()),
    )
    .expect("write hooks-paths");

    let (status, stderr) =
        run_hook_write_deny_scenario(&home, &grok, &workspace, "hook_write_deny");
    assert!(
        status.success(),
        "hook write-deny e2e failed: {status}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("OK: hook write-deny e2e passed"),
        "missing pass marker\nstderr: {stderr}"
    );
    for needle in [
        "OK: hooks readable",
        "OK: hooks file write denied",
        "OK: hooks-paths write denied",
        "OK: dynamic target write denied",
        "OK: hooks-paths unlink denied",
        "OK: hooks rename denied",
        "OK: hooks nested dir mkdir denied",
        "OK: parent rename denied",
        "OK: sessions sibling writable",
        "OK: workspace parent rename denied",
        "OK: workspace sibling under parent writable",
        "OK: grok runtime sibling writable",
        "OK: workspace sibling writable",
        "OK: temp sibling writable",
    ] {
        assert!(
            stderr.contains(needle),
            "expected '{needle}'\nstderr: {stderr}"
        );
    }
    #[cfg(target_os = "linux")]
    assert!(
        stderr.contains("OK: nested userns did not rewrite hooks"),
        "expected nested userns check\nstderr: {stderr}"
    );
}

/// Hard-linked or symlinked discovery JSON under hooks/ must refuse startup.
#[test]
fn hardlinked_hooks_json_refuses_startup() {
    if skip_if_enforcement_unavailable() {
        return;
    }
    let (home, grok, workspace, _ch, _cg, _cw) = fixture_homes("hook-json-hl");
    fs::create_dir_all(grok.join("hooks")).unwrap();
    fs::write(grok.join("hooks-paths"), b"").unwrap();
    let active = grok.join("hooks").join("active.json");
    let alias = grok.join("hooks").join("active-alias.json");
    fs::write(&active, r#"{"hooks":{}}"#).unwrap();
    fs::hard_link(&active, &alias).unwrap();

    let (status, stderr) =
        run_hook_write_deny_scenario(&home, &grok, &workspace, "hook_write_deny");
    assert!(
        !status.success(),
        "hard-linked hooks JSON must refuse startup\nstderr: {stderr}"
    );
}

#[test]
#[cfg(unix)]
fn symlinked_hooks_json_refuses_startup() {
    if skip_if_enforcement_unavailable() {
        return;
    }
    let (home, grok, workspace, _ch, _cg, _cw) = fixture_homes("hook-json-sym");
    fs::create_dir_all(grok.join("hooks")).unwrap();
    fs::write(grok.join("hooks-paths"), b"").unwrap();
    let real = grok.join("real-active.json");
    let active = grok.join("hooks").join("active.json");
    fs::write(&real, r#"{"hooks":{}}"#).unwrap();
    std::os::unix::fs::symlink(&real, &active).unwrap();

    let (status, stderr) =
        run_hook_write_deny_scenario(&home, &grok, &workspace, "hook_write_deny");
    assert!(
        !status.success(),
        "symlinked hooks JSON must refuse startup\nstderr: {stderr}"
    );
}

/// First-run: missing fixed slots are created as real Grok state before apply,
/// then write-denied. Parent asserts post-exit host tree is valid (no vendor stubs).
#[test]
fn workspace_protects_direct_hook_sources_first_run() {
    if skip_if_enforcement_unavailable() {
        return;
    }

    let (home, grok, workspace, _ch, _cg, _cw) = fixture_homes("hook-fr");
    // Intentionally leave hooks/ and hooks-paths absent (first-run ensure path).

    let (status, stderr) =
        run_hook_write_deny_scenario(&home, &grok, &workspace, "hook_write_deny_first_run");
    assert!(
        status.success(),
        "hook write-deny first-run e2e failed: {status}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("OK: hook write-deny e2e passed"),
        "missing pass marker\nstderr: {stderr}"
    );
    for needle in [
        "OK: first-run Grok hook slots denied",
        "OK: hooks-paths (first-run) write denied",
        "OK: hooks nested (first-run) mkdir denied",
        "OK: hooks nested file (first-run) write denied",
        "OK: grok runtime sibling writable",
        "OK: workspace sibling writable",
        "OK: temp sibling writable",
    ] {
        assert!(
            stderr.contains(needle),
            "expected '{needle}'\nstderr: {stderr}"
        );
    }

    // Post-exit host: Grok slots exist and are valid; no vendor artifacts.
    assert!(
        grok.join("hooks").is_dir(),
        "post-exit: hooks dir must exist as a real directory"
    );
    assert!(
        grok.join("hooks-paths").is_file(),
        "post-exit: hooks-paths must exist as a real file"
    );
    assert_eq!(
        fs::read(grok.join("hooks-paths")).expect("read hooks-paths"),
        b"",
        "post-exit: first-run hooks-paths must be empty"
    );
    assert!(
        !home.join(".claude").exists(),
        "post-exit: must not create ~/.claude"
    );
    assert!(
        !home.join(".cursor").exists(),
        "post-exit: must not create ~/.cursor"
    );
}

/// Marker spoof in an isolated subprocess (no env-mutating unit test).
#[test]
fn hook_write_deny_refuses_marker_spoof() {
    // Always runnable on Linux unit path via soft-skip only when not requiring
    // kernel enforcement — marker spoof only needs the verify API.
    #[cfg(not(target_os = "linux"))]
    {}
    #[cfg(target_os = "linux")]
    {
        let (home, grok, workspace, _ch, _cg, _cw) = fixture_homes("hook-spoof");
        fs::create_dir_all(grok.join("hooks")).unwrap();
        fs::write(grok.join("hooks").join("x.json"), b"{}").unwrap();
        fs::write(grok.join("hooks-paths"), b"").unwrap();
        let (status, stderr) =
            run_hook_write_deny_scenario(&home, &grok, &workspace, "hook_write_deny_marker_spoof");
        assert!(
            status.success(),
            "marker spoof e2e failed: {status}\nstderr: {stderr}"
        );
        assert!(
            stderr.contains("OK: marker spoof refused"),
            "expected spoof refusal\nstderr: {stderr}"
        );
    }
}

struct TempDirGuard(std::path::PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
