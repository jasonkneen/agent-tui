//! General subprocess plumbing shared by the harnesses in this crate.

use std::sync::Arc;

/// Pipe all three stdio handles, `kill_on_drop`, spawn, and drain the child's
/// stderr into the returned buffer on a background task. The one spawn path
/// shared by every subprocess harness in this crate (`GrokStdioClient`,
/// `RawStdioClient`, `leader::LeaderStdioClient`); env/args stay with the
/// callers, whose hermeticity models differ (sandbox-inherit vs `env_clear`).
/// The drain future is `Send`, so this works on and off a `LocalSet`.
pub(crate) fn spawn_piped_with_stderr_capture(
    mut cmd: tokio::process::Command,
) -> (tokio::process::Child, Arc<std::sync::Mutex<Vec<u8>>>) {
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    // Derived from `cmd` itself so the panic can never name a different binary
    // than the one actually spawned.
    let program = cmd.as_std().get_program().to_string_lossy().into_owned();
    let mut child = cmd
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn grok at {program}: {e}"));

    let stderr = Arc::new(std::sync::Mutex::new(Vec::new()));
    let stderr_capture = stderr.clone();
    let mut child_stderr = child.stderr.take().expect("child stderr missing");
    tokio::spawn(async move {
        use tokio::io::AsyncReadExt as _;

        let mut buf = [0_u8; 1024];
        loop {
            match child_stderr.read(&mut buf).await {
                Ok(0) => break,
                Ok(read) => stderr_capture
                    .lock()
                    .unwrap()
                    .extend_from_slice(&buf[..read]),
                Err(_) => break,
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for pid file {}",
                path.display()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[cfg(unix)]
    async fn wait_until_gone(pid: u32) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while pid_is_alive(pid) && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!pid_is_alive(pid), "pid {pid} leaked");
    }

    #[cfg(unix)]
    #[test]
    fn non_reaping_exit_observation_validates_pid() {
        let error = process_has_exited_without_reap(0, "invalid fixture")
            .expect_err("zero pid must be rejected");
        assert_eq!(error.kind(), ErrorKind::InvalidInput);
        assert!(error.to_string().contains("invalid fixture pid 0"));
    }

    #[cfg(unix)]
    #[test]
    fn non_reaping_exit_observation_preserves_wait_status() {
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "exit 23"]);
        agent_tui_tty_utils::detach_std_command(&mut command);
        #[allow(clippy::disallowed_methods)] // test fixture; the test reaps it
        let mut child = command.spawn().expect("spawn observation fixture");
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !process_has_exited_without_reap(child.id(), "observation fixture")
            .expect("observe child")
        {
            assert!(
                std::time::Instant::now() < deadline,
                "observation fixture did not exit"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(child.wait().expect("reap observed child").code(), Some(23));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn direct_exit_captures_status_and_output_tails() {
        let sandbox = TestSandbox::new();
        let mut process = TestProcess::spawn(
            shell("printf 'stdout-final'; printf 'stderr-final' >&2; exit 7"),
            &sandbox,
            TestProcessConfig::new().label("direct-exit"),
        )
        .expect("spawn direct child");

        let status = process
            .wait_with_deadline(Duration::from_secs(3))
            .await
            .expect("wait direct child")
            .expect("direct child timed out");

        assert_eq!(status.code(), Some(7));
        assert_eq!(process.status(), Some(status));
        assert_eq!(
            process.termination_reason(),
            Some(TestProcessTermination::NaturalExit)
        );
        assert_eq!(process.stdout_tail().text, "stdout-final");
        assert_eq!(process.stderr_tail().text, "stderr-final");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn wait_timeout_then_hard_kill_reaps_grandchild_tree() {
        let sandbox = TestSandbox::new();
        let pid_file = sandbox.temp_dir().join("grandchild.pid");
        let mut process = TestProcess::spawn(
            shell("sleep 1000 & echo $! > \"$PID_FILE\"; wait"),
            &sandbox,
            TestProcessConfig::new()
                .label("grandchild-timeout")
                .env("PID_FILE", &pid_file),
        )
        .expect("spawn child tree");
        let grandchild_pid = wait_for_pid_file(&pid_file).await;

        assert!(
            process
                .wait_with_deadline(Duration::from_millis(50))
                .await
                .expect("deadline wait")
                .is_none()
        );
        process.kill().await.expect("hard-kill child tree");

        wait_until_gone(grandchild_pid).await;
        assert_eq!(
            process.termination_reason(),
            Some(TestProcessTermination::HardKill)
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn graceful_termination_exits_without_escalation() {
        let sandbox = TestSandbox::new();
        let ready_file = sandbox.temp_dir().join("term-ready.pid");
        let mut process = TestProcess::spawn(
            shell("trap 'exit 0' TERM; echo $$ > \"$READY_FILE\"; while :; do sleep 1; done"),
            &sandbox,
            TestProcessConfig::new()
                .label("handle-term")
                .env("READY_FILE", &ready_file),
        )
        .expect("spawn TERM-handling child");
        wait_for_pid_file(&ready_file).await;

        let status = process.close().await.expect("graceful close");
        assert!(status.success());
        assert_eq!(
            process.termination_reason(),
            Some(TestProcessTermination::GracefulTerminate)
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn graceful_termination_escalates_when_sigterm_is_ignored() {
        let sandbox = TestSandbox::new();
        let ready_file = sandbox.temp_dir().join("ignore-term-ready.pid");
        let mut process = TestProcess::spawn(
            shell("trap '' TERM; echo $$ > \"$READY_FILE\"; while :; do sleep 1; done"),
            &sandbox,
            TestProcessConfig::new()
                .label("ignore-term")
                .env("READY_FILE", &ready_file)
                .grace_period(Duration::from_millis(100)),
        )
        .expect("spawn TERM-resistant child");
        wait_for_pid_file(&ready_file).await;

        process.close().await.expect("close with hard fallback");
        assert_eq!(
            process.termination_reason(),
            Some(TestProcessTermination::HardKillAfterGrace)
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bounded_tail_and_diagnostics_report_truncation_without_secrets() {
        let mut sandbox = TestSandbox::new();
        sandbox.set_env("CUSTOM_TOKEN", "do-not-print-this-token");
        let mut process = TestProcess::spawn(
            shell(
                "printf '%0256d' 0; printf 'stdout-final'; \
                 printf 'CUSTOM_TOKEN=%s\\nstderr-final' \"$CUSTOM_TOKEN\" >&2",
            ),
            &sandbox,
            TestProcessConfig::new()
                .label("bounded-output")
                .tail_bytes(64),
        )
        .expect("spawn bounded-output child");
        process
            .wait_with_deadline(Duration::from_secs(3))
            .await
            .expect("wait bounded-output child")
            .expect("bounded-output child timed out");

        let stdout = process.stdout_tail();
        assert!(stdout.truncated);
        assert!(stdout.bytes_seen > 64);
        assert!(stdout.text.ends_with("stdout-final"));
        let diagnostics = process.diagnostic_summary();
        assert!(diagnostics.contains("stdout_truncated=true"));
        assert!(diagnostics.contains("stdout-final"));
        assert!(diagnostics.contains("stderr-final"));
        assert!(!diagnostics.contains("do-not-print-this-token"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn panic_like_owner_drop_kills_direct_child_and_grandchild() {
        let sandbox = TestSandbox::new();
        let pid_file = sandbox.temp_dir().join("drop-grandchild.pid");
        let process = TestProcess::spawn(
            shell("sleep 1000 & echo $! > \"$PID_FILE\"; wait"),
            &sandbox,
            TestProcessConfig::new()
                .label("panic-drop")
                .env("PID_FILE", &pid_file),
        )
        .expect("spawn panic-drop child tree");
        let direct_pid = process.pid().expect("live direct child pid");
        let grandchild_pid = wait_for_pid_file(&pid_file).await;
        let mut owner = Some(process);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let process_owner = owner.take().expect("process owner");
            drop(process_owner);
            panic!("simulated owner panic");
        }));
        assert!(result.is_err());

        wait_until_gone(direct_pid).await;
        wait_until_gone(grandchild_pid).await;
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_job_kill_reaps_spawned_grandchild() {
        let sandbox = TestSandbox::new();
        let pid_file = sandbox.temp_dir().join("windows-grandchild.pid");
        let mut cmd = tokio::process::Command::new("powershell.exe");
        cmd.args([
            "-NoProfile",
            "-Command",
            "$p = Start-Process powershell.exe -ArgumentList '-NoProfile','-Command','Start-Sleep 1000' -PassThru; Set-Content -NoNewline -Path $env:PID_FILE -Value $p.Id; Wait-Process -Id $p.Id",
        ]);
        let mut process = TestProcess::spawn(
            cmd,
            &sandbox,
            TestProcessConfig::new()
                .label("windows-job-tree")
                .env("PID_FILE", &pid_file),
        )
        .expect("spawn Windows job tree");
        if !process.tree.is_attached() {
            eprintln!(
                "SKIP: Windows Job attachment is best effort: {}",
                process.diagnostic_summary()
            );
            process
                .kill()
                .await
                .expect("clean unattached Windows child");
            return;
        }
    });

    (child, stderr)
}
