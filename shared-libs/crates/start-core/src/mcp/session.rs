use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use patch_db::json_ptr::JsonPointer;
use patch_db::DiffPatch;
use serde_json::Value as JsonValue;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::context::RpcContext;
use crate::db::DbSubscriber;
use crate::prelude::*;
use crate::util::sync::SyncMutex;

use super::protocol::McpNotification;

pub(crate) type SessionMap = Arc<SyncMutex<HashMap<String, McpSession>>>;

/// Maximum time a session can exist without a GET stream before being cleaned up.
const SESSION_STALE_TIMEOUT: Duration = Duration::from_secs(60);

/// Maximum buffered notifications before backpressure kicks in.
const NOTIFICATION_CHANNEL_BOUND: usize = 256;

pub(crate) struct McpSession {
    pub notification_tx: mpsc::Sender<JsonValue>,
    pub notification_rx: Option<mpsc::Receiver<JsonValue>>,
    pub subscriptions: HashMap<String, JoinHandle<()>>,
    pub created_at: Instant,
}

pub(crate) fn create_session(sessions: &SessionMap) -> String {
    let id = Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::channel(NOTIFICATION_CHANNEL_BOUND);
    sessions.mutate(|map| {
        map.insert(
            id.clone(),
            McpSession {
                notification_tx: tx,
                notification_rx: Some(rx),
                subscriptions: HashMap::new(),
                created_at: Instant::now(),
            },
        );
    });
    id
}

/// Sweep stale sessions. Call this from any frequent code path (POST handler, create_session).
pub(crate) fn sweep_stale_sessions_if_needed(sessions: &SessionMap) {
    sessions.mutate(|map| sweep_stale_sessions(map));
}

/// Remove sessions that were created but never connected a GET stream within the timeout.
fn sweep_stale_sessions(map: &mut HashMap<String, McpSession>) {
    let stale: Vec<String> = map
        .iter()
        .filter(|(_, session)| {
            // Session is stale if rx is still present (no GET connected) and it's old
            session.notification_rx.is_some()
                && session.created_at.elapsed() > SESSION_STALE_TIMEOUT
        })
        .map(|(id, _)| id.clone())
        .collect();
    for id in stale {
        tracing::info!(
            target: "mcp_audit",
            session_id = %id,
            "Sweeping stale MCP session (no GET stream connected)"
        );
        if let Some(session) = map.remove(&id) {
            for (_, handle) in session.subscriptions {
                handle.abort();
            }
        }
    }
}

pub(crate) fn remove_session(sessions: &SessionMap, id: &str) {
    sessions.mutate(|map| {
        if let Some(session) = map.remove(id) {
            for (_, handle) in session.subscriptions {
                handle.abort();
            }
        }
    });
}

/// Take the notification receiver from a session (for use by the SSE stream).
/// Returns None if the session doesn't exist or the rx was already taken.
pub(crate) fn take_notification_rx(
    sessions: &SessionMap,
    id: &str,
) -> Option<mpsc::Receiver<JsonValue>> {
    sessions.mutate(|map| map.get_mut(id)?.notification_rx.take())
}

/// Check whether the given session ID exists in the session map.
pub(crate) fn session_exists(sessions: &SessionMap, id: &str) -> bool {
    sessions.peek(|map| map.contains_key(id))
}

/// Parse a `startos:///...` URI into a JsonPointer.
fn parse_resource_uri(uri: &str) -> Result<JsonPointer, Error> {
    let path = uri.strip_prefix("startos://").ok_or_else(|| {
        Error::new(
            eyre!("Invalid resource URI: must start with startos://"),
            ErrorKind::InvalidRequest,
        )
    })?;
    path.parse::<JsonPointer>()
        .with_kind(ErrorKind::InvalidRequest)
}

pub(crate) async fn subscribe_resource(
    ctx: &RpcContext,
    sessions: &SessionMap,
    session_id: &str,
    uri: &str,
) -> Result<(), Error> {
    let pointer = parse_resource_uri(uri)?;

    let (dump, sub) = ctx.db.dump_and_sub(pointer).await;
    let mut db_sub = DbSubscriber {
        rev: dump.id,
        sub,
        sync_db: ctx.sync_db.subscribe(),
    };

    let tx = sessions
        .peek(|map| map.get(session_id).map(|s| s.notification_tx.clone()))
        .ok_or_else(|| Error::new(eyre!("Session not found"), ErrorKind::InvalidRequest))?;

    let uri_owned = uri.to_string();

    let handle = tokio::spawn(async move {
        loop {
            // Wait for first revision
            let first = match db_sub.recv().await {
                Some(rev) => rev,
                None => break,
            };

            // Debounce: collect more revisions for up to 500ms
            let mut merged_id = first.id;
            let mut merged_patch = first.patch;

            let debounce = tokio::time::sleep(Duration::from_millis(500));
            tokio::pin!(debounce);

            loop {
                tokio::select! {
                    _ = &mut debounce => break,
                    rev = db_sub.recv() => {
                        match rev {
                            Some(rev) => {
                                merged_id = rev.id;
                                merged_patch.append(rev.patch);
                            }
                            None => {
                                // Subscriber closed — send what we have and exit
                                let _ = send_notification(&tx, &uri_owned, merged_id, &merged_patch);
                                return;
                            }
                        }
                    }
                }
            }

            if send_notification(&tx, &uri_owned, merged_id, &merged_patch).is_err() {
                break; // SSE stream closed or channel full
            }
        }
    });

    // Store the task handle, aborting any prior subscription for the same URI
    sessions.mutate(|map| {
        if let Some(session) = map.get_mut(session_id) {
            if let Some(old_handle) = session.subscriptions.remove(uri) {
                tracing::info!(
                    target: "mcp_audit",
                    uri = %uri,
                    session_id = %session_id,
                    "Aborting prior subscription for re-subscribed URI"
                );
                old_handle.abort();
            }
            session.subscriptions.insert(uri.to_string(), handle);
        }
    });

    Ok(())
}

fn send_notification(
    tx: &mpsc::Sender<JsonValue>,
    uri: &str,
    id: u64,
    patch: &DiffPatch,
) -> Result<(), ()> {
    let notification = McpNotification {
        jsonrpc: "2.0",
        method: "notifications/resources/updated",
        params: serde_json::json!({
            "uri": uri,
            "revision": {
                "id": id,
                "patch": patch,
            }
        }),
    };
    tx.try_send(serde_json::to_value(&notification).unwrap_or_default())
        .map_err(|e| {
            tracing::warn!(
                target: "mcp_audit",
                uri = %uri,
                "Notification channel full or closed, dropping notification: {e}"
            );
        })
}

pub(crate) fn unsubscribe_resource(sessions: &SessionMap, session_id: &str, uri: &str) {
    sessions.mutate(|map| {
        if let Some(session) = map.get_mut(session_id) {
            if let Some(handle) = session.subscriptions.remove(uri) {
                handle.abort();
            }
        }
    });
}
