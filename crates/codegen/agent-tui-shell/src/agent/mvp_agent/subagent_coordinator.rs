//! Subagent coordinator drain task and spawn-context construction for [`MvpAgent`].
//! Co-located child of `mvp_agent` (`use super::*`); tested by `tests/subagent_spawn_context_tests.rs`.
use super::*;
use crate::session::repo_changes::UploadMethod;
use agent_tui_tools::implementations::grok_build::task::coordinator;
struct ShellChildRunner {
    agent_ref: LocalRef<MvpAgent>,
}
impl coordinator::ChildRunner for ShellChildRunner {
    type Control = crate::agent::subagent::ShellChildRuntime;
    type CompletionData = crate::agent::subagent::ShellCompletionData;
    type RunFuture = coordinator::LocalBoxFuture<coordinator::ChildRunOutput<Self::CompletionData>>;
    type ValidateFuture = coordinator::LocalBoxFuture<
        agent_tui_tools::implementations::grok_build::task::types::SubagentValidateTypeOutcome,
    >;
    type DescribeFuture = coordinator::LocalBoxFuture<
        agent_tui_tools::implementations::grok_build::task::types::SubagentDescribeOutcome,
    >;
    fn run(&self, run: coordinator::ChildRunRequest<Self::Control>) -> Self::RunFuture {
        let agent_ref = self.agent_ref.clone();
        Box::pin(async move {
            let this = agent_ref.get();
            let parent_sid = run.request.parent_session_id.clone();
            let Some(mut ctx) = this.try_build_subagent_spawn_context(&parent_sid) else {
                tracing::warn!(
                    parent_session_id = %parent_sid,
                    subagent_id = %run.request.id,
                    "Spawn for unknown or evicted parent session"
                );
                return coordinator::ChildRunOutput {
                    result: agent_tui_tools::implementations::grok_build::task::types::SubagentResult {
                        success: false,
                        error: Some(
                            "Parent session not found (evicted or torn down); cannot spawn subagent."
                                .to_owned(),
                        ),
                        subagent_id: run.request.id.clone(),
                        child_session_id: run.request.id,
                        ..Default::default()
                    },
                    completion_data: Default::default(),
                    snapshot_ref: None,
                };
            };
            let parent_handle = {
                let parent_sid = acp::SessionId::new(parent_sid);
                this.resident_handle(&parent_sid)
            };
            if let Some(handle) = parent_handle {
                ctx.parent_mcp_pool = handle.snapshot_mcp_pool().await;
                ctx.client_hooks = handle.snapshot_client_hooks().await;
                let definitions = handle.snapshot_tool_definitions().await;
                ctx.parent_tool_definitions = (!definitions.is_empty()).then_some(definitions);
            }
            crate::agent::subagent::run_shell_child(run, ctx, &this.gateway).await
        })
    }
    fn validate_type(
        &self,
        subagent_type: String,
        parent_session_id: String,
    ) -> Self::ValidateFuture {
        let agent_ref = self.agent_ref.clone();
        Box::pin(async move {
            let this = agent_ref.get();
            let ctx = this.build_subagent_validation_context(&parent_session_id);
            crate::agent::subagent::validate_subagent_type(&subagent_type, &ctx)
        })
    }
    fn describe_type(
        &self,
        subagent_type: String,
        harness_agent_type: Option<String>,
        parent_session_id: String,
    ) -> Self::DescribeFuture {
        let agent_ref = self.agent_ref.clone();
        Box::pin(async move {
            let this = agent_ref.get();
            match this.try_build_subagent_spawn_context(&parent_session_id) {
                Some(ctx) => crate::agent::subagent::describe_subagent_type(
                    &subagent_type,
                    harness_agent_type.as_deref(),
                    &ctx,
                ),
                None => {
                    tracing::warn!(
                        parent_session_id,
                        subagent_type,
                        "DescribeType for unknown/evicted parent session, replying Unavailable",
                    );
                    agent_tui_tools::implementations::grok_build::task::types::SubagentDescribeOutcome::Unavailable
                }
            }
        })
    }
    fn on_completed(&self, completion: coordinator::ChildCompletion<Self::CompletionData>) {
        let gateway = self.agent_ref.get().gateway.clone();
        crate::agent::subagent::present_child_completion(completion, &gateway);
    }
    fn running_count_changed(&self, running: usize) {
        self.agent_ref
            .get()
            .activity
            .subagent_gauge()
            .store(running, std::sync::atomic::Ordering::Relaxed);
    }
    fn persisted_output_ref(&self, completion_data: &Self::CompletionData) -> Option<String> {
        completion_data
            .persisted_output_dir()
            .map(|path| path.to_string_lossy().into_owned())
    }
    fn load_persisted_output(&self, reference: &str) -> Option<std::sync::Arc<str>> {
        crate::agent::subagent::read_subagent_output(std::path::Path::new(reference))
            .map(std::sync::Arc::from)
    }
}
/// Injected as the coordinator's limit sink: it cannot link the telemetry
/// crate (dependency cycle through the sampling types).
fn log_limit_notice(notice: coordinator::SubagentLimitNotice) {
    use coordinator::{LimitedSpawnOrigin, SubagentLimitDecision};
    use agent_tui_telemetry::events::{
        SubagentLimitDisposition, SubagentLimitHit, SubagentOwnerKind,
    };
    let (disposition, limit) = match notice.decision {
        SubagentLimitDecision::QueuedAtConcurrentLimit { limit } => {
            (SubagentLimitDisposition::Queued, limit as u64)
        }
        SubagentLimitDecision::RejectedAtConcurrentLimit { limit } => {
            (SubagentLimitDisposition::Failed, limit as u64)
        }
    };
    agent_tui_telemetry::session_ctx::log_event(SubagentLimitHit::session_concurrent(
        notice.parent_session_id,
        disposition,
        limit,
        u32::try_from(notice.running).unwrap_or(u32::MAX),
        u32::try_from(notice.queue_depth).unwrap_or(u32::MAX),
        match notice.origin {
            LimitedSpawnOrigin::SchedulerLoop => SubagentOwnerKind::SchedulerLoop,
            LimitedSpawnOrigin::Task => SubagentOwnerKind::Task,
        },
    ));
}
impl MvpAgent {
    /// Start the subagent coordinator drain task.
    ///
    /// Takes the `subagent_event_rx` receiver (once) and spawns a `spawn_local` task
    /// that receives `SubagentRequest`s and delegates each to
    /// `handle_subagent_request()` on its own `spawn_local` task.
    ///
    /// Uses `LocalRef` to reference `self` from
    /// `spawn_local` closures. Idempotent: subsequent calls are no-ops.
    pub(super) fn start_subagent_coordinator(&self) {
        let Some(mut rx) = self.subagent_event_rx.borrow_mut().take() else {
            return;
        };
        let agent_ref = LocalRef::new(self);
        use crate::agent::subagent::{BlockWaitSlot, is_running, resolve_snapshot};
        use agent_tui_tools::implementations::grok_build::task::types::{
            SubagentCancelOutcome, SubagentCancelTarget, SubagentEvent,
        };
        let limit_sink: coordinator::SubagentLimitSink = std::sync::Arc::new(log_limit_notice);
        let config = coordinator::CoordinatorConfig {
            foreground_budget:
                agent_tui_tools::implementations::grok_build::task::backend::env_duration_or(
                    "GROK_SUBAGENT_AWAIT_BUDGET_MS",
                    std::time::Duration::from_secs(600),
                ),
            limits: agent_tui_tools::implementations::grok_build::task::admission::SubagentLimits {
                max_concurrent: self.cfg.borrow().subagents_max_concurrent,
                behavior: self.cfg.borrow().subagents_limit_behavior,
            },
            limit_sink: Some(limit_sink),
            buffer_completions: true,
            buffered_completion_output_cap: None,
        };
        tokio::task::spawn_local(coordinator::SubagentCoordinator::new(rx, runner, config).run());
        let (trace_tx, mut trace_rx) = tokio::sync::mpsc::unbounded_channel::<
            crate::upload::turn::SyntheticTurnTraceRequest,
        >();
        self.subagent_presentation.borrow_mut().synthetic_trace_tx = Some(trace_tx);
        tokio::task::spawn_local({
            let agent_ref = agent_ref.clone();
            async move {
                while let Some(event) = rx.recv().await {
                    match event {
                        SubagentEvent::Spawn(boxed) => {
                            let request = *boxed;
                            let agent_ref = agent_ref.clone();
                            tokio::task::spawn_local(async move {
                                let this = agent_ref.get();
                                let parent_sid = request.parent_session_id.clone();
                                let mut ctx = this.build_subagent_spawn_context(&parent_sid);
                                let parent_handle = {
                                    let parent_sid_acp = acp::SessionId::new(parent_sid.clone());
                                    this.sessions.borrow().get(&parent_sid_acp).cloned()
                                };
                                if let Some(handle) = parent_handle {
                                    ctx.parent_mcp_pool = handle.snapshot_mcp_pool().await;
                                    ctx.client_hooks = handle.snapshot_client_hooks().await;
                                    let parent_tools = handle.snapshot_tool_definitions().await;
                                    ctx.parent_tool_snapshot =
                                        (!parent_tools.is_empty()).then_some(parent_tools);
                                }
                                crate::agent::subagent::handle_subagent_request(
                                    request,
                                    ctx,
                                    &this.subagent_coordinator,
                                    &this.gateway,
                                )
                                .await;
                            });
                        }
                        SubagentEvent::Query(query) => {
                            let agent_ref = agent_ref.clone();
                            tokio::task::spawn_local(async move {
                                let subagent_id = query.subagent_id;
                                let block = query.block;
                                let timeout_ms = query.timeout_ms;
                                let slot: BlockWaitSlot = std::rc::Rc::new(
                                    std::cell::RefCell::new(Some(query.respond_to)),
                                );
                                let send_via_slot =
                                    |slot: &BlockWaitSlot, snap| match slot.borrow_mut().take() {
                                        Some(tx) => tx.send(snap).is_ok(),
                                        None => false,
                                    };
                                let lookup = {
                                    let this = agent_ref.get();
                                    let result =
                                        this.subagent_coordinator.borrow().lookup(&subagent_id);
                                    if block && result.is_some() {
                                        this.subagent_coordinator
                                            .borrow_mut()
                                            .register_block_wait(&subagent_id, slot.clone());
                                    }
                                    this.subagent_coordinator
                                        .borrow_mut()
                                        .evict_stale_completed();
                                    result
                                };
                                let snapshot = resolve_snapshot(lookup).await;
                                let should_block =
                                    block && snapshot.as_ref().is_some_and(is_running);
                                if should_block {
                                    let timeout_ms = timeout_ms.unwrap_or(30_000);
                                    let deadline = tokio::time::Instant::now()
                                        + tokio::time::Duration::from_millis(timeout_ms);
                                    loop {
                                        tokio::time::sleep(tokio::time::Duration::from_millis(200))
                                            .await;
                                        let receiver_gone =
                                            slot.borrow().as_ref().is_none_or(|tx| tx.is_closed());
                                        if receiver_gone {
                                            let this = agent_ref.get();
                                            let mut coord = this.subagent_coordinator.borrow_mut();
                                            coord.clear_block_waited(&subagent_id);
                                            coord.unregister_block_wait(&subagent_id, &slot);
                                            return;
                                        }
                                        let lookup = {
                                            let this = agent_ref.get();
                                            this.subagent_coordinator.borrow().lookup(&subagent_id)
                                        };
                                        let snap = resolve_snapshot(lookup).await;
                                        let still_running = snap.as_ref().is_some_and(is_running);
                                        if !still_running || tokio::time::Instant::now() >= deadline
                                        {
                                            {
                                                let this = agent_ref.get();
                                                let mut coord =
                                                    this.subagent_coordinator.borrow_mut();
                                                if still_running {
                                                    coord.clear_block_waited(&subagent_id);
                                                }
                                                coord.unregister_block_wait(&subagent_id, &slot);
                                            }
                                            if !send_via_slot(&slot, snap) && !still_running {
                                                let this = agent_ref.get();
                                                this.subagent_coordinator
                                                    .borrow_mut()
                                                    .clear_block_waited(&subagent_id);
                                            }
                                            return;
                                        }
                                    }
                                } else {
                                    let delivered = send_via_slot(&slot, snapshot);
                                    if block {
                                        let this = agent_ref.get();
                                        let mut coord = this.subagent_coordinator.borrow_mut();
                                        coord.unregister_block_wait(&subagent_id, &slot);
                                        if !delivered {
                                            coord.clear_block_waited(&subagent_id);
                                        }
                                    }
                                }
                            });
                        }
                        SubagentEvent::Cancel(request) => {
                            let this = agent_ref.get();
                            let outcome = {
                                let mut coord = this.subagent_coordinator.borrow_mut();
                                match request.target {
                                    SubagentCancelTarget::SubagentId(ref subagent_id) => {
                                        coord.mark_explicitly_killed(subagent_id);
                                        coord.cancel_with_outcome(subagent_id)
                                    }
                                    SubagentCancelTarget::ParentPromptId(ref parent_prompt_id) => {
                                        coord.cancel_by_parent_prompt_id(parent_prompt_id);
                                        SubagentCancelOutcome::Cancelled
                                    }
                                }
                            };
                            let _ = request.respond_to.send(outcome);
                        }
                        SubagentEvent::ListActive(request) => {
                            let this = agent_ref.get();
                            let summaries = this
                                .subagent_coordinator
                                .borrow()
                                .active_summaries_for(&request.parent_session_id);
                            let _ = request.respond_to.send(summaries);
                        }
                        SubagentEvent::Completions(request) => {
                            let this = agent_ref.get();
                            let mut completions = this
                                .subagent_coordinator
                                .borrow_mut()
                                .drain_pending_completions();
                            completions.retain(|c| !request.suppress_ids.contains(&c.subagent_id));
                            let _ = request.respond_to.send(completions);
                        }
                        SubagentEvent::Outstanding(request) => {
                            let this = agent_ref.get();
                            let reply = this
                                .subagent_coordinator
                                .borrow()
                                .outstanding_reply_for_prompt(&request.prompt_id);
                            let _ = request.respond_to.send(reply);
                        }
                        SubagentEvent::ClearUsageNotApplied(request) => {
                            let this = agent_ref.get();
                            this.subagent_coordinator
                                .borrow_mut()
                                .clear_subagent_usage_not_applied(&request.prompt_id);
                        }
                        SubagentEvent::MarkUsageNotApplied(request) => {
                            let this = agent_ref.get();
                            this.subagent_coordinator
                                .borrow_mut()
                                .mark_subagent_usage_not_applied(&request.prompt_id);
                            let _ = request.respond_to.send(());
                        }
                        SubagentEvent::ValidateType(request) => {
                            let agent_ref = agent_ref.clone();
                            tokio::task::spawn_local(async move {
                                let this = agent_ref.get();
                                let ctx = this
                                    .build_subagent_validation_context(&request.parent_session_id);
                                let outcome = crate::agent::subagent::validate_subagent_type(
                                    &request.subagent_type,
                                    &ctx,
                                );
                                let _ = request.respond_to.send(outcome);
                            });
                        }
                        SubagentEvent::DescribeType(request) => {
                            let agent_ref = agent_ref.clone();
                            tokio::task::spawn_local(async move {
                                use agent_tui_tools::implementations::grok_build::task::types::SubagentDescribeOutcome;
                                let this = agent_ref.get();
                                let outcome = match this
                                    .try_build_subagent_spawn_context(&request.parent_session_id)
                                {
                                    Some(ctx) => crate::agent::subagent::describe_subagent_type(
                                        &request.subagent_type,
                                        request.harness_agent_type.as_deref(),
                                        &ctx,
                                    ),
                                    None => {
                                        tracing::warn!(
                                            parent_session_id = % request.parent_session_id,
                                            subagent_type = % request.subagent_type,
                                            "DescribeType for unknown/evicted parent session, replying Unavailable",
                                        );
                                        SubagentDescribeOutcome::Unavailable
                                    }
                                };
                                let _ = request.respond_to.send(outcome);
                            });
                        }
                    }
                }
            }
        });
        {
            let (trace_tx, mut trace_rx) = tokio::sync::mpsc::unbounded_channel::<
                crate::upload::turn::SyntheticTurnTraceRequest,
            >();
            self.subagent_coordinator.borrow_mut().synthetic_trace_tx = Some(trace_tx);
            tokio::task::spawn_local({
                let agent_ref = agent_ref.clone();
                async move {
                    while let Some(request) = trace_rx.recv().await {
                        tokio::task::spawn_local({
                            let agent_ref = agent_ref.clone();
                            async move {
                                handle_synthetic_turn_trace(agent_ref, request).await;
                            }
                        });
                    }
                }
            });
        }
    }
    /// Lightweight context for the `SubagentEvent::ValidateType` drain arm;
    /// tolerates evicted parent sessions (returns built-in defaults + warns).
    pub(super) fn build_subagent_validation_context(
        &self,
        parent_session_id: &str,
    ) -> crate::agent::subagent::SubagentValidationContext {
        let parent_sid = acp::SessionId::new(parent_session_id);
        let (parent_cwd, allowed_subagent_types) = {
            let ps = self.resident_handle(&parent_sid);
            warn_on_missing_parent_session_for_validate_type(parent_session_id, ps.is_some());
            (
                ps.as_ref()
                    .map(|h| std::path::PathBuf::from(&h.info.cwd))
                    .unwrap_or_default(),
                ps.as_ref().and_then(|h| h.allowed_subagent_types.clone()),
            )
        };
        let cli_agent_names: Vec<String> = {
            let cfg = self.cfg.borrow();
            cfg.cli_agents.iter().map(|d| d.name.clone()).collect()
        };
        crate::agent::subagent::SubagentValidationContext {
            parent_cwd,
            plugin_registry: self.plugin_registry_handle.snapshot(),
            subagent_toggle: self.subagent_toggle.clone(),
            allowed_subagent_types,
            cli_agent_names,
        }
    }
    /// Build a `SubagentSpawnContext` from the current agent state and the
    /// parent session's shared resources.
    ///
    /// This is the ONLY subagent-related method on MvpAgent besides the
    /// coordinator startup.
    /// Build a spawn context for a real subagent spawn. The parent session is
    /// guaranteed present here because the parent just issued the spawn request,
    /// so a missing parent is a real invariant violation and panics. Read-only
    /// callers that can race a parent teardown (e.g. `DescribeType`) must use
    /// [`Self::try_build_subagent_spawn_context`] instead.
    pub(super) fn build_subagent_spawn_context(
        &self,
        parent_session_id: &str,
    ) -> crate::agent::subagent::SubagentSpawnContext {
        self.try_build_subagent_spawn_context(parent_session_id)
            .expect("parent session must exist when spawning subagents")
    }
    /// Fallible variant of [`Self::build_subagent_spawn_context`]: returns
    /// `None` when the parent `SessionHandle` is absent (evicted / torn down)
    /// instead of panicking, so read-only paths that can race a teardown can
    /// fail open.
    pub(super) fn try_build_subagent_spawn_context(
        &self,
        parent_session_id: &str,
    ) -> Option<crate::agent::subagent::SubagentSpawnContext> {
        let parent_sid = acp::SessionId::new(parent_session_id);
        let parent_handle = self.resident_handle(&parent_sid);
        let (
            parent_model_id,
            parent_chat_state,
            parent_cmd_tx,
            parent_cwd,
            yolo_mode,
            parent_depth,
            hunk_tracker_handle,
            hunk_tracking_enabled,
            fs,
            terminal,
            session_env,
            parent_attribution_callback,
            parent_agent_name,
            parent_managed_mcp_proxy_base_url,
        ) = {
            let ps = parent_handle.as_ref();
            (
                ps.as_ref()
                    .map(|h| h.model_id.clone())
                    .unwrap_or_else(|| self.models_manager.current_model_id()),
                ps.as_ref().map(|h| h.chat_state_handle.clone()),
                ps.as_ref().map(|h| h.cmd_tx.clone()),
                ps.as_ref()
                    .map(|h| std::path::PathBuf::from(&h.info.cwd))
                    .unwrap_or_default(),
                ps.as_ref()
                    .map(|h| h.yolo_mode)
                    .unwrap_or(self.default_yolo_mode),
                ps.as_ref()
                    .map(|h| h.tool_context.subagent_depth)
                    .unwrap_or(0),
                ps.as_ref()
                    .map(|h| h.tool_context.hunk_tracker_handle.clone())
                    .unwrap_or_else(agent_tui_hunk_tracker::HunkTrackerHandle::noop),
                ps.as_ref()
                    .map(|h| h.tool_context.hunk_tracking_enabled)
                    .unwrap_or(false),
                ps.as_ref()
                    .map(|h| h.tool_context.fs.inner().clone())
                    .unwrap_or_else(|| {
                        let cwd = ps
                            .as_ref()
                            .map(|h| std::path::PathBuf::from(&h.info.cwd))
                            .unwrap_or_default();
                        std::sync::Arc::new(agent_tui_workspace::file_system::LocalFs::new(cwd))
                    }),
                ps.as_ref()
                    .map(|h| h.tool_context.terminal.clone())
                    .unwrap_or_else(|| {
                        std::sync::Arc::new(crate::terminal::TerminalRunner::new(
                            std::sync::Arc::new(self.gateway.clone()),
                            parent_sid.clone(),
                        ))
                    }),
                ps.as_ref()
                    .map(|h| h.tool_context.session_env.clone())
                    .unwrap_or_else(|| std::sync::Arc::new(std::collections::HashMap::new())),
                ps.as_ref().and_then(|h| h.attribution_callback.clone()),
                ps.as_ref().map(|h| h.agent_name.clone()),
                ps.as_ref().map(|h| h.managed_mcp_proxy_base_url.clone()),
            )
        };
        let (
            parent_workspace_ops,
            parent_terminal_backend,
            parent_notification_handle,
            parent_scheduler_handle,
        ) = parent_handle.as_ref().map(|ps| {
            (
                ps.workspace_ops.clone(),
                ps.terminal_backend.clone(),
                ps.tools_notification_handle.clone(),
                ps.scheduler_handle.clone(),
            )
        })?;
        let available_models = self.models_manager.models();
        let (parent_lsp, parent_process_scope) = {
            let parent = parent_handle.as_ref();
            (
                parent.as_ref().and_then(|h| h.tool_context.lsp.clone()),
                parent
                    .as_ref()
                    .and_then(|h| h.tool_context.process_scope.clone()),
            )
        };
        let am = self.auth_manager.clone();
        let inference_idle_timeout_secs = {
            let per_model = config::find_model_by_id(&available_models, parent_model_id.0.as_ref())
                .and_then(|e| e.info.inference_idle_timeout_secs);
            let cfg = self.cfg.borrow();
            let remote = cfg
                .remote_settings
                .as_ref()
                .and_then(|s| s.inference_idle_timeout_secs);
            per_model.or(remote).unwrap_or(600).max(10)
        };
        let parent_hook_registry = parent_handle.as_ref().and_then(|h| h.hook_registry.clone());
        let parent_max_turns = parent_handle.as_ref().and_then(|h| h.max_turns);
        let parent_model_agent_type =
            config::find_model_by_id(&available_models, parent_model_id.0.as_ref())
                .map(|e| e.info.agent_type.clone());
        let ask_user_question_enabled = parent_handle
            .as_ref()
            .map(|h| h.ask_user_question_enabled)
            .unwrap_or_else(|| self.cfg.borrow().resolve_ask_user_question().value);
        let (gcs_upload_method, gcs_bucket_url) = match self.trace_upload_config_snapshot() {
            Some(method) => {
                use crate::session::repo_changes::UploadMethod;
                let bucket = match &method {
                    UploadMethod::Direct { .. } => self
                        .cfg
                        .borrow()
                        .endpoints
                        .resolve_trace_bucket_url()
                        .map(|r| r.value),
                    UploadMethod::Proxy { .. } => Some("proxy-managed".to_string()),
                    UploadMethod::S3 { bucket, .. } => Some(format!("s3://{bucket}")),
                };
                match bucket {
                    Some(url) => (Some(method), Some(url)),
                    None => (None, None),
                }
            }
            None => (None, None),
        };
        let project_trusted = crate::agent::folder_trust::project_scope_allowed(&parent_cwd);
        let (base_roles, base_personas, subagent_model_overrides, subagent_toggle) = {
            let cfg = self.cfg.borrow();
            (
                cfg.subagent_roles.clone(),
                cfg.subagent_personas.clone(),
                cfg.subagent_model_overrides.clone(),
                cfg.subagent_toggle.clone(),
            )
        };
        let (subagent_roles, subagent_personas) =
            crate::config::SubagentsConfig::effective_definition_maps(
                &base_roles,
                &base_personas,
                &parent_cwd,
                project_trusted,
            );
        let inherited_tool_overrides = parent_handle
            .as_ref()
            .and_then(|ps| ps.resolved_tool_overrides.load_full().map(|o| (*o).clone()));
        Some(crate::agent::subagent::SubagentSpawnContext {
            lsp: parent_lsp,
            process_scope: parent_process_scope,
            client_hooks: Default::default(),
            sampling_config: self.sampling_config.borrow().clone(),
            managed_mcp_proxy_base_url: parent_managed_mcp_proxy_base_url
                .unwrap_or_else(|| self.cli_chat_proxy_base_url()),
            alpha_test_key: self.alpha_test_key(),
            auth_method_id: self
                .auth_method_id
                .load()
                .as_deref()
                .cloned()
                .unwrap_or_else(|| acp::AuthMethodId::new("default")),
            model_id: parent_model_id,
            storage_mode: self.storage_mode,
            auth: self.current_or_buffered_auth(),
            parent_cwd: parent_cwd.clone(),
            parent_session_id: parent_session_id.to_string(),
            yolo_mode,
            subagent_event_tx: self.subagent_event_tx.clone(),
            parent_depth,
            subagents_max_depth: self.cfg.borrow().subagents_max_depth,
            workflow_max_concurrent_agents: self.cfg.borrow().workflow_max_concurrent_agents,
            inference_idle_timeout_secs,
            auto_compact_threshold_tiers:
                crate::agent::subagent::AutoCompactThresholdTiers::capture(&self.cfg.borrow()),
            hunk_tracker_handle,
            hunk_tracking_enabled,
            fs,
            terminal,
            session_env,
            memory_config: self.memory_config.clone(),
            web_search_sampling_config: self.prepare_web_search_sampling_config(),
            web_fetch_config: self.prepare_web_fetch_config(),
            image_gen_config: self.prepare_image_gen_config(),
            video_gen_config: self.prepare_video_gen_config(),
            app_builder_deployer_config: self.prepare_app_builder_deployer_config(),
            write_file_enabled: self.cfg.borrow().resolve_write_file().value,
            goal_enabled: self.cfg.borrow().resolve_goal().value,
            ask_user_question_enabled,
            parent_cmd_tx: parent_cmd_tx.clone(),
            parent_session_info: parent_handle.as_ref().map(|h| crate::session::info::Info {
                id: parent_sid.clone(),
                cwd: h.info.cwd.clone(),
            }),
            parent_chat_state,
            parent_max_turns,
            available_models,
            subagent_model_overrides: self.subagent_model_overrides.clone(),
            subagent_toggle: self.subagent_toggle.clone(),
            subagent_roles: self.subagent_roles.clone(),
            subagent_personas: self.subagent_personas.clone(),
            persona_io_summaries: self.persona_io_summaries.clone(),
            disable_web_search: self.cfg.borrow().disable_web_search,
            todo_gate: self.cfg.borrow().todo_gate,
            remote_settings: self.cfg.borrow().remote_settings.clone(),
            laziness_debug_log: self.cfg.borrow().laziness_debug_log.clone(),
            backend_tools_enabled: self.cfg.borrow().resolve_backend_tools().value,
            respect_gitignore: self.cfg.borrow().respect_gitignore,
            path_not_found_hints: self.cfg.borrow().path_not_found_hints,
            plugin_registry: self.plugin_registry_handle.snapshot(),
            models_manager: self.models_manager.clone(),
            file_tool_overrides: {
                let cfg = self.cfg.borrow();
                let effective = cfg
                    .toolset
                    .resolve_file_toolset(cfg.remote_settings.as_ref());
                if effective != crate::tools::FileToolset::Standard {
                    effective.tool_configs(&cfg.toolset.hashline).ok()
                } else {
                    None
                }
            },
            gcs_bucket_url,
            agent_config: Some(self.cfg.borrow().clone()),
            gcs_upload_method,
            hook_registry: parent_hook_registry,
            permission_handle: parent_handle.as_ref().map(|h| h.permission_handle.clone()),
            worktree_type: self.worktree_type,
            api_key_provider: Some(Arc::new(crate::auth::manager::SharedAuthKeyProvider(
                am.clone(),
            ))),
            image_description_model: self.resolve_image_description_model(),
            workspace_ops: parent_workspace_ops.clone(),
            auth_manager: am.clone(),
            attribution_callback: parent_attribution_callback,
            parent_agent_name,
            parent_model_agent_type,
            allowed_subagent_types: parent_handle
                .as_ref()
                .and_then(|h| h.allowed_subagent_types.clone()),
            parent_mcp_configs: parent_handle
                .as_ref()
                .map(|h| h.mcp_servers.clone())
                .unwrap_or_default(),
            managed_mcp_state: self.managed_mcp_cache.clone(),
            parent_mcp_pool: None,
            parent_tool_snapshot: None,
            parent_skills: None,
            parent_skills_config: self.cfg.borrow().skills.clone(),
            parent_compat: self.cfg.borrow().compat_resolved,
            task_completion_reservations: parent_handle
                .as_ref()
                .and_then(|h| h.tool_context.task_completion_reservations.clone()),
            synthetic_trace_tx: parent_handle
                .as_ref()
                .and_then(|h| h.tool_context.synthetic_trace_tx.clone()),
            task_output_tool_name: parent_handle
                .as_ref()
                .map(|h| h.tool_context.task_output_tool_name.clone())
                .unwrap_or_else(|| {
                    agent_tui_tools::reminders::task_completion::DEFAULT_TASK_OUTPUT_TOOL.to_string()
                }),
            auto_wake_enabled: self.cfg.borrow().auto_wake_enabled,
            goal_loop_active: parent_handle
                .as_ref()
                .map(|h| h.tool_context.goal_loop_active_gate.clone())
                .unwrap_or_else(|| std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))),
            parent_terminal_backend: parent_terminal_backend.clone(),
            parent_notification_handle: parent_notification_handle.clone(),
            parent_scheduler_handle: parent_scheduler_handle.clone(),
        })
    }
}
