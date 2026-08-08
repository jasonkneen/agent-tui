//! AcpTerminalAdapter: implements `agent-tui-tools::TerminalBackend` using ACP gateway calls.
//!
//! This adapter enables bash tool execution over ACP (remote execution).
//! It translates agent-tui-tools' `TerminalBackend` trait into ACP protocol calls:
//!   `run()` → create_terminal → wait_for_exit → terminal_output → release_terminal
//!   `run_background()` → create_terminal + spawn exit watcher
//!   `get_task()` → terminal_output (merged with tracked metadata)
//!   `kill_task()` → kill_terminal_command (watcher detects exit)
//!   `wait_for_completion()` → wait_for_terminal_exit with timeout

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_client_protocol as acp;
use agent_tui_acp_lib::{AcpAgentGatewaySender as GatewaySender, acp_channel_failure};
use agent_tui_tools::computer::types::{
    BackgroundHandle, ComputerError, KillOutcome, TaskSnapshot, TerminalBackend,
    TerminalRunRequest, TerminalRunResult,
};
use agent_tui_tools::notification::types::ToolNotificationHandle;

// ── Tracked task state ───────────────────────────────────────────────

struct TrackedTask {
    command: String,
    display_command: Option<String>,
    cwd: String,
    output_file: PathBuf,
    start_time: std::time::SystemTime,
    completed: bool,
    exit_code: Option<i32>,
    signal: Option<String>,
    last_output: String,
    last_truncated: bool,
    block_waited: bool,
    explicitly_killed: bool,
}

impl TrackedTask {
    fn mark_completed(
        &mut self,
        exit_code: Option<i32>,
        signal: Option<String>,
        output: String,
        truncated: bool,
    ) {
        self.completed = true;
        self.exit_code = exit_code;
        self.signal = signal;
        self.last_output = output;
        self.last_truncated = truncated;
    }

    fn to_snapshot(
        &self,
        task_id: &str,
        output: String,
        truncated: bool,
        exit_code: Option<i32>,
        signal: Option<String>,
    ) -> TaskSnapshot {
        let completed = self.completed || exit_code.is_some();
        TaskSnapshot {
            task_id: task_id.to_string(),
            command: self.command.clone(),
            display_command: self.display_command.clone(),
            cwd: self.cwd.clone(),
            start_time: self.start_time,
            end_time: completed.then(std::time::SystemTime::now),
            output,
            output_file: self.output_file.clone(),
            truncated,
            exit_code,
            signal,
            completed,
            block_waited: self.block_waited,
            explicitly_killed: self.explicitly_killed,
            kind: self.kind,
            owner_session_id: self.owner_session_id.clone(),
            description: self.description.clone(),
            // ACP tracked tasks are only registered via run_background.
            is_backgrounded: true,
            output_total_bytes: 0,
        }
    }

    let (exit_code, signal, output_text, truncated) = match gateway
        .send(acp::TerminalOutputRequest::new(
            session_id.clone(),
            terminal_id.clone(),
        ))
        .await
    {
        Ok(o) => {
            let (code, sig) = parse_exit(&o.exit_status);
            (code, sig, o.output, o.truncated)
        }
        Err(_) => (None, None, String::new(), false),
    };

    let snapshot = {
        let mut tasks = tasks.lock().unwrap();
        let Some(task) = tasks.get_mut(&task_id) else {
            return;
        };
        task.mark_completed(exit_code, signal.clone(), output_text.clone(), truncated);
        task.to_snapshot(&task_id, output_text, truncated, exit_code, signal)
    };

    notification_handle.send_task_complete(snapshot);

    let _ = gateway
        .send(acp::ReleaseTerminalRequest::new(session_id, terminal_id))
        .await;
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Poll `TerminalOutputRequest` at 500ms intervals until `exit_status` is
/// present, a deadline is hit, or 60 consecutive gateway errors occur.
/// Returns `true` when an exit was detected.
async fn poll_for_terminal_exit(
    gateway: &GatewaySender,
    session_id: &acp::SessionId,
    terminal_id: &acp::TerminalId,
    deadline: Option<tokio::time::Instant>,
) -> bool {
    let mut consecutive_errors = 0u32;
    loop {
        if let Some(dl) = deadline
            && tokio::time::Instant::now() >= dl
        {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        match gateway
            .send(acp::TerminalOutputRequest::new(
                session_id.clone(),
                terminal_id.clone(),
            ))
            .await
        {
            Ok(output) => {
                consecutive_errors = 0;
                if output.exit_status.is_some() {
                    return true;
                }
            }
            Err(e) => {
                consecutive_errors += 1;
                if consecutive_errors >= 60 {
                    tracing::error!(
                        terminal_id = %terminal_id.0,
                        error = %e,
                        "gateway unreachable after 60 consecutive poll failures"
                    );
                    return false;
                }
            }
        }
    }
}

fn wrap_command(command: &str) -> Result<String, ComputerError> {
    // On Windows the ACP client (grok-desktop) spawns with `shell: true`
    // which delegates to cmd.exe.  Wrapping in /bin/bash would fail because
    // that path doesn't exist on Windows.  Send the raw command instead.
    #[cfg(not(unix))]
    {
        let _ = command;
        Ok(command.to_string())
    }
    #[cfg(unix)]
    {
        let quoted = shlex::try_quote(command).map_err(|_| ComputerError::CommandNotQuoted)?;
        Ok(format!(
            "{} -lc {quoted}",
            crate::terminal::default_shell_path()
        ))
    }
}

fn to_env(env: HashMap<String, String>) -> Vec<acp::EnvVariable> {
    env.into_iter()
        .map(|(name, value)| acp::EnvVariable::new(name, value))
        .collect()
}

fn parse_exit(status: &Option<acp::TerminalExitStatus>) -> (Option<i32>, Option<String>) {
    match status {
        Some(e) => (e.exit_code.map(|v| v as i32), e.signal.clone()),
        None => (None, None),
    }
}

// ── Adapter ──────────────────────────────────────────────────────────

/// Wraps agent-tui-shell's ACP gateway to satisfy agent-tui-tools' TerminalBackend.
pub struct AcpTerminalAdapter {
    gateway: GatewaySender,
    session_id: acp::SessionId,
    tasks: TaskMap,
}

impl AcpTerminalAdapter {
    pub fn new(gateway: GatewaySender, session_id: acp::SessionId) -> Self {
        Self {
            gateway,
            session_id,
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn create_terminal(
        &self,
        command: String,
        request: &TerminalRunRequest,
    ) -> Result<acp::CreateTerminalResponse, ComputerError> {
        self.gateway
            .send(
                acp::CreateTerminalRequest::new(self.session_id.clone(), command)
                    .args(vec![])
                    .env(to_env(request.env.clone()))
                    .cwd(Some(request.working_directory.clone()))
                    .output_byte_limit(Some(request.output_byte_limit as u64)),
            )
            .await
            .map_err(|e| ComputerError::io(e.to_string()))
    }

    fn terminal_id(&self, task_id: &str) -> acp::TerminalId {
        acp::TerminalId::new(task_id)
    }
}

#[async_trait::async_trait]
impl TerminalBackend for AcpTerminalAdapter {
    async fn run(&self, request: TerminalRunRequest) -> Result<TerminalRunResult, ComputerError> {
        let command = wrap_command(&request.command)?;
        let create_res = self.create_terminal(command, &request).await?;

        let timed_out = match tokio::time::timeout(
            request.timeout,
            self.gateway.send(acp::WaitForTerminalExitRequest::new(
                self.session_id.clone(),
                create_res.terminal_id.clone(),
            )),
        )
        .await
        {
            Ok(Ok(_)) => false,
            Ok(Err(e)) => return Err(ComputerError::io(e.to_string())),
            Err(_) => {
                let _ = self
                    .gateway
                    .send(acp::KillTerminalRequest::new(
                        self.session_id.clone(),
                        create_res.terminal_id.clone(),
                    ))
                    .await;
                true
            }
        };

        let output = self
            .gateway
            .send(acp::TerminalOutputRequest::new(
                self.session_id.clone(),
                create_res.terminal_id.clone(),
            ))
            .await
            .map_err(|e| ComputerError::io(e.to_string()))?;

        let _ = self
            .gateway
            .send(acp::ReleaseTerminalRequest::new(
                self.session_id.clone(),
                create_res.terminal_id,
            ))
            .await;

        let (exit_code, signal) = parse_exit(&output.exit_status);
        let total_bytes = output.output.len();
        Ok(TerminalRunResult {
            combined_output: output.output,
            exit_code,
            truncated: output.truncated,
            signal,
            timed_out,
            output_file: request.output_file,
            total_bytes,
            // ACP gateway does not surface a local PID -- the process
            // runs on the remote side.
            pid: None,
        })
    }

    async fn run_background(
        &self,
        request: TerminalRunRequest,
    ) -> Result<BackgroundHandle, ComputerError> {
        let command = wrap_command(&request.command)?;
        let notification_handle = request.notification_handle.clone();
        let display_command = request.display_command.clone();
        let cwd = request.working_directory.to_string_lossy().to_string();
        let output_file = request.output_file.clone();

        let create_res = self.create_terminal(command.clone(), &request).await?;
        let task_id = create_res.terminal_id.0.to_string();

        {
            let mut tasks = self.tasks.lock().unwrap();
            tasks.insert(
                task_id.clone(),
                TrackedTask {
                    command,
                    display_command,
                    cwd,
                    output_file: output_file.clone(),
                    start_time: std::time::SystemTime::now(),
                    completed: false,
                    exit_code: None,
                    signal: None,
                    last_output: String::new(),
                    last_truncated: false,
                    block_waited: false,
                    explicitly_killed: false,
                },
            );
        }

        tokio::spawn(watch_for_exit(
            self.gateway.clone(),
            self.session_id.clone(),
            task_id.clone(),
            Arc::clone(&self.tasks),
            notification_handle,
        ));

        Ok(BackgroundHandle {
            task_id,
            output_file,
            // ACP gateway does not surface a local PID -- the process
            // runs on the remote side.
            pid: None,
        })
    }

    async fn get_task(&self, task_id: &str) -> Option<TaskSnapshot> {
        let live = self
            .gateway
            .send(acp::TerminalOutputRequest::new(
                self.session_id.clone(),
                self.terminal_id(task_id),
            ))
            .await
            .ok();

        let tasks = self.tasks.lock().unwrap();
        let tracked = tasks.get(task_id);

        match (live, tracked) {
            (Some(output), Some(tracked)) => {
                let (exit_code, signal) = parse_exit(&output.exit_status);
                Some(tracked.to_snapshot(
                    task_id,
                    output.output,
                    output.truncated,
                    exit_code,
                    signal,
                ))
            }
            (Some(output), None) => {
                let (exit_code, signal) = parse_exit(&output.exit_status);
                let completed = exit_code.is_some();
                Some(TaskSnapshot {
                    task_id: task_id.to_string(),
                    command: String::new(),
                    display_command: None,
                    cwd: String::new(),
                    start_time: std::time::SystemTime::now(),
                    end_time: completed.then(std::time::SystemTime::now),
                    output: output.output,
                    output_file: PathBuf::new(),
                    truncated: output.truncated,
                    exit_code,
                    signal,
                    completed,
                    kind: agent_tui_tools::computer::types::TaskKind::Bash,
                    block_waited: false,
                    explicitly_killed: false,
                    owner_session_id: None,
                })
            }
            (None, Some(tracked)) if tracked.completed => Some(tracked.to_snapshot(
                task_id,
                tracked.last_output.clone(),
                tracked.last_truncated,
                tracked.exit_code,
                tracked.signal.clone(),
            )),
            _ => None,
        }
    }

    async fn kill_task(&self, task_id: &str) -> KillOutcome {
        enum Tracked {
            Running,
            Completed,
            Unknown,
        }
        let tracked = {
            let mut tasks = self.tasks.lock().unwrap();
            match tasks.get_mut(task_id) {
                Some(task) if task.completed => Tracked::Completed,
                Some(task) => {
                    task.explicitly_killed = true;
                    Tracked::Running
                }
                None => Tracked::Unknown,
            }
        };

        match tracked {
            Tracked::Completed => return KillOutcome::AlreadyExited,
            Tracked::Running => {}
            Tracked::Unknown => {
                let probe = self
                    .gateway
                    .send(acp::TerminalOutputRequest::new(
                        self.session_id.clone(),
                        self.terminal_id(task_id),
                    ))
                    .await;
                match probe {
                    Err(err) if acp_channel_failure(&err).is_none() => {
                        return KillOutcome::NotFound;
                    }
                    Err(_) => {}
                    Ok(output) if output.exit_status.is_some() => {
                        return KillOutcome::AlreadyExited;
                    }
                    Ok(_) => {}
                }
            }
        }

        match self
            .gateway
            .send(acp::KillTerminalRequest::new(
                self.session_id.clone(),
                self.terminal_id(task_id),
            ))
            .await
        {
            Ok(_) => KillOutcome::Killed,
            Err(_) => KillOutcome::NotFound,
        }
    }

    async fn wait_for_completion(
        &self,
        task_id: &str,
        timeout: Option<Duration>,
    ) -> Option<TaskSnapshot> {
        let timeout = timeout.unwrap_or(Duration::from_secs(30));

        let already_completed = {
            let mut tasks = self.tasks.lock().unwrap();
            match tasks.get_mut(task_id) {
                Some(task) => {
                    task.block_waited = true;
                    task.completed
                }
                None => false,
            }
        };
        if already_completed {
            return self.get_task(task_id).await;
        }

        let gateway_result = tokio::time::timeout(
            timeout,
            self.gateway.send(acp::WaitForTerminalExitRequest::new(
                self.session_id.clone(),
                self.terminal_id(task_id),
            )),
        )
        .await;

        match &gateway_result {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                let completed_meanwhile = {
                    let tasks = self.tasks.lock().unwrap();
                    tasks.get(task_id).is_some_and(|task| task.completed)
                };
                if !completed_meanwhile {
                    tracing::warn!(task_id, error = %e, "gateway error waiting for terminal exit, falling back to polling");
                    let deadline = tokio::time::Instant::now() + timeout;
                    poll_for_terminal_exit(
                        &self.gateway,
                        &self.session_id,
                        &self.terminal_id(task_id),
                        Some(deadline),
                    )
                    .await;
                }
            }
            Err(_) => {
                tracing::debug!(task_id, "timeout waiting for terminal exit");
                // The block timed out: the agent did not receive the
                // completion result, so auto-wake should still fire
                // when the task eventually completes.
                let mut tasks = self.tasks.lock().unwrap();
                if let Some(task) = tasks.get_mut(task_id) {
                    task.block_waited = false;
                }
            }
        }

        self.get_task(task_id).await
    }

    async fn list_tasks(&self) -> Vec<TaskSnapshot> {
        let task_ids: Vec<String> = {
            let tasks = self.tasks.lock().unwrap();
            tasks.keys().cloned().collect()
        };
        let mut snapshots = Vec::new();
        for task_id in task_ids {
            if let Some(snapshot) = self.get_task(&task_id).await {
                snapshots.push(snapshot);
            }
        }
        snapshots
    }

    async fn kill_all_background_tasks(&self) {
        let task_ids: Vec<String> = {
            let tasks = self.tasks.lock().unwrap();
            tasks
                .iter()
                .filter(|(_, t)| !t.completed)
                .map(|(id, _)| id.clone())
                .collect()
        };
        for task_id in task_ids {
            self.kill_task(&task_id).await;
        }
    }

    async fn kill_foreground_commands(&self) {
        let session_id = self.session_id.0.to_string();
        crate::terminal::kill_and_release_all_for_session(&session_id).await;
    }
}

#[cfg(test)]
#[path = "adapter_tests.rs"]
mod tests;
