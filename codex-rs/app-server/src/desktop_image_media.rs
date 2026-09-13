use codex_app_server_protocol::ClientResponsePayload;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadTimelineEntry;
use codex_app_server_protocol::Turn;

use crate::message_processor::ConnectionSessionState;
use crate::transport::ConnectionOrigin;
use crate::transport::OutgoingMessage;

/// Windows Desktop's native file preview cannot resolve executor paths inside WSL.
/// Use its existing inline-image fallback for this local connection. Stored history
/// and responses to other clients retain the executor's saved path.
pub(crate) fn prepare_generated_images(
    message: &mut OutgoingMessage,
    session: &ConnectionSessionState,
) {
    let distribution = if cfg!(target_os = "linux") {
        std::env::var("WSL_DISTRO_NAME").ok()
    } else {
        None
    };
    if !uses_inline_images(
        session.origin,
        session.app_server_client_name(),
        distribution.as_deref(),
    ) {
        return;
    }
    prefer_inline_images(message);
}

fn uses_inline_images(
    origin: ConnectionOrigin,
    client_name: Option<&str>,
    distribution: Option<&str>,
) -> bool {
    origin == ConnectionOrigin::Stdio
        && client_name == Some("Codex Desktop")
        && distribution.is_some_and(|name| !name.is_empty())
}

fn prefer_inline_images(message: &mut OutgoingMessage) {
    match message {
        OutgoingMessage::AppServerNotification(envelope) => match &mut envelope.notification {
            ServerNotification::ItemStarted(notification) => prepare_item(&mut notification.item),
            ServerNotification::ItemCompleted(notification) => prepare_item(&mut notification.item),
            ServerNotification::ThreadStarted(notification) => {
                prepare_turns(&mut notification.thread.turns)
            }
            ServerNotification::TurnStarted(notification) => {
                prepare_turns(std::slice::from_mut(&mut notification.turn))
            }
            ServerNotification::TurnCompleted(notification) => {
                prepare_turns(std::slice::from_mut(&mut notification.turn))
            }
            _ => {}
        },
        OutgoingMessage::Response(response) => match response.result.as_mut() {
            ClientResponsePayload::ThreadStart(response) => {
                prepare_turns(&mut response.thread.turns)
            }
            ClientResponsePayload::ThreadResume(response) => {
                prepare_turns(&mut response.thread.turns)
            }
            ClientResponsePayload::ThreadFork(response) => {
                prepare_turns(&mut response.thread.turns)
            }
            ClientResponsePayload::ThreadRead(response) => {
                prepare_turns(&mut response.thread.turns)
            }
            ClientResponsePayload::ThreadUnarchive(response) => {
                prepare_turns(&mut response.thread.turns)
            }
            ClientResponsePayload::ThreadRevert(response) => {
                prepare_turns(&mut response.thread.turns)
            }
            ClientResponsePayload::ThreadTurnsList(response) => prepare_turns(&mut response.data),
            ClientResponsePayload::ThreadItemsList(response) => {
                for entry in &mut response.data {
                    prepare_item(&mut entry.item);
                }
            }
            ClientResponsePayload::ThreadTimelineList(response) => {
                for entry in &mut response.data {
                    if let ThreadTimelineEntry::Item { item, .. } = entry {
                        prepare_item(item);
                    }
                }
            }
            _ => {}
        },
        OutgoingMessage::Request(_) | OutgoingMessage::Error(_) => {}
    }
}

fn prepare_turns(turns: &mut [Turn]) {
    for turn in turns {
        for item in &mut turn.items {
            prepare_item(item);
        }
    }
}

fn prepare_item(item: &mut ThreadItem) {
    if let ThreadItem::ImageGeneration(image) = item
        && !image.result.is_empty()
    {
        // savedPath is an optional display hint. Clearing it on the outgoing copy
        // makes Desktop use result instead of its Windows-only native file URL.
        image.saved_path = None;
    }
}

#[cfg(test)]
#[path = "desktop_image_media_tests.rs"]
mod tests;
