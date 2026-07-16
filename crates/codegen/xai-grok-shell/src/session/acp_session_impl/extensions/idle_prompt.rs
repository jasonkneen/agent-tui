//! Debounced `idle_prompt` notification extension.

use std::rc::Rc;
use std::time::Duration;

use xai_agent_lifecycle::LocalExtensionRegistryBuilder;
use xai_agent_lifecycle::{LocalSessionLifecycleContributor, LocalTurnLifecycleContributor};
use xai_agent_lifecycle::{
    SessionIdleInput, TurnAbortInput, TurnDoneInput, TurnErrorInput, TurnStartInput,
};

use super::super::*;
use super::{NotificationEvent, NotificationEventSink};

/// Default `idle_prompt` debounce (60s of user inactivity).
const DEFAULT_IDLE_NOTIFICATION_DELAY: Duration = Duration::from_secs(60);

/// Debounce between the session settling idle and the `idle_prompt` notification, so it fires only on sustained inactivity.
/// `GROK_IDLE_NOTIFICATION_DELAY_MS` overrides it (used by E2E tests).
fn idle_notification_delay() -> Duration {
    resolve_idle_notification_delay(std::env::var("GROK_IDLE_NOTIFICATION_DELAY_MS").ok())
}

/// Split from [`idle_notification_delay`] so the env parsing is testable without touching the process env.
fn resolve_idle_notification_delay(raw: Option<String>) -> Duration {
    raw.and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_IDLE_NOTIFICATION_DELAY)
}

/// Fires the `idle_prompt` notification hook once the session stays idle for the delay. Synthetic turns (auto-wake, drain, cron) only defer an
/// earned ping: they cancel the timer like any turn start, and their own idle settle re-arms it.
/// Covered by the headless E2E via `GROK_IDLE_NOTIFICATION_DELAY_MS`.
struct IdlePromptExtension {
    notification_event_sink: Rc<dyn NotificationEventSink>,
    timer: TaskSlot<()>,
    delay: Duration,
    /// Only a completed turn earns a ping; aborted and errored turns do not (matching the old
    /// end_turn-only arming).
    last_turn_completed: std::cell::Cell<bool>,
}

#[async_trait::async_trait(?Send)]
impl LocalTurnLifecycleContributor for IdlePromptExtension {
    async fn on_turn_start(&self, _input: &TurnStartInput) {
        self.timer.cancel();
    }

    async fn on_turn_done(&self, _input: &TurnDoneInput) {
        self.last_turn_completed.set(true);
    }

    async fn on_turn_abort(&self, _input: &TurnAbortInput) {
        self.last_turn_completed.set(false);
    }

    async fn on_turn_error(&self, _input: &TurnErrorInput<'_>) {
        self.last_turn_completed.set(false);
    }
}

#[async_trait::async_trait(?Send)]
impl LocalSessionLifecycleContributor for IdlePromptExtension {
    async fn on_session_idle(&self, _input: &SessionIdleInput) {
        if !self.last_turn_completed.get() {
            return;
        }
        let notification_event_sink = Rc::clone(&self.notification_event_sink);
        let delay = self.delay;
        let handle = tokio::task::spawn_local(async move {
            tokio::time::sleep(delay).await;
            notification_event_sink.emit(NotificationEvent {
                notification_type: "idle_prompt",
                message: Some("Turn complete".into()),
                title: None,
                level: Some("info".into()),
            });
        });
        self.timer.arm(handle);
    }
}

pub(super) fn install(
    builder: &mut LocalExtensionRegistryBuilder,
    notification_event_sink: Rc<dyn NotificationEventSink>,
) {
    let extension = Rc::new(IdlePromptExtension {
        notification_event_sink,
        timer: TaskSlot::new(),
        delay: idle_notification_delay(),
        last_turn_completed: std::cell::Cell::new(false),
    });
    builder.turn_lifecycle_contributor(extension.clone());
    builder.session_lifecycle_contributor(extension);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::time::Duration;

    #[derive(Default)]
    struct RecordingSink {
        events: RefCell<Vec<NotificationEvent>>,
    }

    impl NotificationEventSink for RecordingSink {
        fn emit(&self, event: NotificationEvent) {
            self.events.borrow_mut().push(event);
        }
    }

    fn extension(delay: Duration) -> (IdlePromptExtension, Rc<RecordingSink>) {
        let sink = Rc::new(RecordingSink::default());
        let extension = IdlePromptExtension {
            notification_event_sink: sink.clone(),
            timer: TaskSlot::new(),
            delay,
            last_turn_completed: std::cell::Cell::new(false),
        };
        (extension, sink)
    }

    async fn advance(duration: Duration) {
        tokio::time::advance(duration).await;
        tokio::task::yield_now().await;
    }

    /// Missing env var → 60s default.
    #[test]
    fn defaults_to_claude_code_threshold() {
        assert_eq!(
            resolve_idle_notification_delay(None),
            Duration::from_secs(60)
        );
        assert_eq!(
            resolve_idle_notification_delay(None),
            DEFAULT_IDLE_NOTIFICATION_DELAY
        );
    }

    /// Pins the public `GROK_IDLE_NOTIFICATION_DELAY_MS` contract: a valid override is interpreted as milliseconds (the E2E seam depends on this).
    #[test]
    fn env_override_parses_millis() {
        assert_eq!(
            resolve_idle_notification_delay(Some("250".into())),
            Duration::from_millis(250)
        );
    }

    /// A malformed override falls back to the default instead of panicking.
    #[test]
    fn invalid_override_falls_back_to_default() {
        assert_eq!(
            resolve_idle_notification_delay(Some("not-a-number".into())),
            DEFAULT_IDLE_NOTIFICATION_DELAY
        );
    }

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn completed_turn_emits_once_only_after_full_idle_delay() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let delay = Duration::from_millis(100);
                let (extension, sink) = extension(delay);
                extension.on_turn_done(&TurnDoneInput).await;
                extension.on_session_idle(&SessionIdleInput).await;
                tokio::task::yield_now().await;

                advance(delay - Duration::from_millis(1)).await;
                assert!(sink.events.borrow().is_empty());
                advance(Duration::from_millis(1)).await;

                let events = sink.events.borrow();
                assert_eq!(events.len(), 1);
                assert_eq!(events[0].notification_type, "idle_prompt");
                assert_eq!(events[0].message.as_deref(), Some("Turn complete"));
                assert_eq!(events[0].title, None);
                assert_eq!(events[0].level.as_deref(), Some("info"));
                drop(events);

                advance(delay).await;
                assert_eq!(sink.events.borrow().len(), 1, "timer must fire only once");
            })
            .await;
    }

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn turn_start_cancels_pending_notification_and_next_completion_rearms() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let delay = Duration::from_millis(100);
                let (extension, sink) = extension(delay);
                extension.on_turn_done(&TurnDoneInput).await;
                extension.on_session_idle(&SessionIdleInput).await;
                tokio::task::yield_now().await;
                advance(Duration::from_millis(50)).await;

                extension.on_turn_start(&TurnStartInput::new(true)).await;
                advance(delay).await;
                assert!(
                    sink.events.borrow().is_empty(),
                    "synthetic turn start must cancel the earned idle notification"
                );

                extension.on_turn_done(&TurnDoneInput).await;
                extension.on_session_idle(&SessionIdleInput).await;
                tokio::task::yield_now().await;
                advance(delay).await;
                assert_eq!(sink.events.borrow().len(), 1);
            })
            .await;
    }

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn aborted_and_errored_turns_never_arm_idle_notification() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let delay = Duration::from_millis(100);
                let (extension, sink) = extension(delay);

                extension.on_turn_done(&TurnDoneInput).await;
                extension
                    .on_turn_abort(&TurnAbortInput::new(
                        xai_agent_lifecycle::TurnAbortReason::Interrupted,
                    ))
                    .await;
                extension.on_session_idle(&SessionIdleInput).await;
                advance(delay).await;
                assert!(sink.events.borrow().is_empty());

                extension.on_turn_done(&TurnDoneInput).await;
                extension
                    .on_turn_error(&TurnErrorInput {
                        message: "sampler failed",
                    })
                    .await;
                extension.on_session_idle(&SessionIdleInput).await;
                advance(delay).await;
                assert!(sink.events.borrow().is_empty());
            })
            .await;
    }
}
