//! Ties the browser tab to the app's lifetime.
//!
//! Every open GitLeaf tab keeps one Server-Sent Events connection to `/api/events`. That gives two
//! behaviours people expect from a desktop app:
//! - quitting GitLeaf (menu-bar Quit, Ctrl-C, system shutdown) tells each tab, which closes itself
//!   or, if the browser refuses, says that GitLeaf has quit; an update restart is announced
//!   separately so tabs wait and reload instead;
//! - closing the last tab quits GitLeaf after a short grace period, so reloading or briefly losing
//!   the connection does not. This is only enabled when GitLeaf opened the browser itself (or
//!   `GITLEAF_EXIT_WHEN_CLOSED=1`), never for `--no-open` servers such as tests.
//!
//! Browsers keep event streams open in background tabs, so an idle tab does not count as closed.
//! A tab the browser freezes or discards sends `/api/presence/hold` first, which keeps GitLeaf
//! running until a tab connects again.

use std::{
    convert::Infallible,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
    time::Duration,
};

use axum::{
    extract::State,
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::{self, Stream};
use tokio::sync::watch;

use crate::AppState;

pub struct Presence {
    clients: AtomicUsize,
    seen: AtomicBool,
    held: AtomicBool,
    exit_when_closed: AtomicBool,
    grace: Duration,
    ending: watch::Sender<Option<&'static str>>,
}

impl Default for Presence {
    fn default() -> Self {
        Self::with_grace(Duration::from_secs(8))
    }
}

impl Presence {
    pub fn with_grace(grace: Duration) -> Self {
        Self {
            clients: AtomicUsize::new(0),
            seen: AtomicBool::new(false),
            held: AtomicBool::new(false),
            exit_when_closed: AtomicBool::new(false),
            grace,
            ending: watch::channel(None).0,
        }
    }

    pub fn set_exit_when_closed(&self, enabled: bool) {
        self.exit_when_closed.store(enabled, SeqCst);
    }

    pub fn exit_when_closed(&self) -> bool {
        self.exit_when_closed.load(SeqCst)
    }

    /// Tell every open tab why the server is stopping: "quit" or "restart".
    pub fn announce(&self, reason: &'static str) {
        self.ending.send_replace(Some(reason));
    }

    pub fn clients(&self) -> usize {
        self.clients.load(SeqCst)
    }
}

/// One open tab. Dropping it (the tab closed or lost its connection) may start the quit timer.
pub struct Connected {
    state: AppState,
}

pub fn connect(state: &AppState) -> Connected {
    let presence = &state.presence;
    presence.clients.fetch_add(1, SeqCst);
    presence.seen.store(true, SeqCst);
    presence.held.store(false, SeqCst);
    Connected {
        state: state.clone(),
    }
}

impl Drop for Connected {
    fn drop(&mut self) {
        if self.state.presence.clients.fetch_sub(1, SeqCst) == 1 {
            schedule_exit_check(self.state.clone());
        }
    }
}

fn schedule_exit_check(state: AppState) {
    let presence = &state.presence;
    if !presence.exit_when_closed() || !presence.seen.load(SeqCst) {
        return;
    }
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let grace = presence.grace;
    runtime.spawn(async move {
        tokio::time::sleep(grace).await;
        let presence = &state.presence;
        let still_closed = presence.clients() == 0
            && !presence.held.load(SeqCst)
            && presence.ending.borrow().is_none();
        if still_closed {
            println!("last GitLeaf tab closed; quitting");
            state.shutdown.notify_one();
        }
    });
}

/// `GET /api/events`: one long-lived stream per tab.
pub async fn events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let guard = connect(&state);
    let receiver = state.presence.ending.subscribe();
    let hello =
        serde_json::json!({ "exit_when_closed": state.presence.exit_when_closed() }).to_string();
    // Phase 0 sends "hello", phase 1 waits for an ending; the guard lives exactly as long as the stream.
    let events = stream::unfold(
        (Some(guard), receiver, 0u8, hello),
        |(guard, mut receiver, phase, hello)| async move {
            let guard = guard?;
            if phase == 0 {
                let event = Event::default().event("hello").data(hello.clone());
                return Some((Ok(event), (Some(guard), receiver, 1, hello)));
            }
            loop {
                let current = *receiver.borrow_and_update();
                if let Some(reason) = current {
                    let event = Event::default().event("ending").data(reason);
                    // Dropping the guard here ends the stream after this event.
                    drop(guard);
                    return Some((Ok(event), (None, receiver, 2, hello)));
                }
                if receiver.changed().await.is_err() {
                    return None;
                }
            }
        },
    );
    Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(3)))
}

/// `POST /api/presence/hold`: a tab is being frozen or discarded by the browser; do not treat its
/// lost connection as "closed". Cleared when any tab connects again.
pub async fn hold(State(state): State<AppState>) -> StatusCode {
    state.presence.held.store(true, SeqCst);
    StatusCode::NO_CONTENT
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn state(exit: bool) -> AppState {
        let presence = Presence::with_grace(Duration::from_millis(60));
        presence.set_exit_when_closed(exit);
        let scratch = std::env::temp_dir();
        AppState {
            root: scratch.join("gitleaf-presence-root"),
            cache: scratch.join("gitleaf-presence-cache"),
            port: 8077,
            update: Default::default(),
            shutdown: Default::default(),
            restart: Default::default(),
            locks: Default::default(),
            gallery_cache: Default::default(),
            presence: Arc::new(presence),
        }
    }

    async fn quits_within(state: &AppState, wait: Duration) -> bool {
        tokio::time::timeout(wait, state.shutdown.notified())
            .await
            .is_ok()
    }

    #[tokio::test]
    async fn closing_the_last_tab_quits_after_the_grace_period() {
        let state = state(true);
        let first = connect(&state);
        let second = connect(&state);
        drop(first);
        assert!(
            !quits_within(&state, Duration::from_millis(200)).await,
            "another tab is still open"
        );
        drop(second);
        assert!(
            quits_within(&state, Duration::from_secs(2)).await,
            "last tab closed"
        );
    }

    #[tokio::test]
    async fn a_reload_within_the_grace_period_keeps_gitleaf_running() {
        let state = state(true);
        drop(connect(&state));
        let _reloaded = connect(&state);
        assert!(!quits_within(&state, Duration::from_millis(300)).await);
    }

    #[tokio::test]
    async fn servers_without_exit_when_closed_never_quit_on_their_own() {
        let state = state(false);
        drop(connect(&state));
        assert!(!quits_within(&state, Duration::from_millis(300)).await);
    }

    #[tokio::test]
    async fn a_frozen_tab_holds_gitleaf_open_until_a_tab_reconnects() {
        let state = state(true);
        let tab = connect(&state);
        state.presence.held.store(true, SeqCst);
        drop(tab);
        assert!(
            !quits_within(&state, Duration::from_millis(300)).await,
            "held by a frozen tab"
        );
        drop(connect(&state));
        assert!(
            quits_within(&state, Duration::from_secs(2)).await,
            "hold cleared by reconnect"
        );
    }

    #[tokio::test]
    async fn an_announced_quit_or_restart_does_not_trigger_a_second_shutdown() {
        let state = state(true);
        let tab = connect(&state);
        state.presence.announce("restart");
        drop(tab);
        assert!(!quits_within(&state, Duration::from_millis(300)).await);
    }
}
