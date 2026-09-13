use super::*;
use codex_app_server_protocol::ImageGenerationItem;
use codex_app_server_protocol::ItemCompletedNotification;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotificationEnvelope;
use codex_app_server_protocol::ThreadItemEntry;
use codex_app_server_protocol::ThreadItemsListResponse;
use codex_app_server_transport::OutgoingResponse;
use codex_utils_absolute_path::test_support::PathBufExt;
use codex_utils_absolute_path::test_support::test_path_buf;
use pretty_assertions::assert_eq;

#[test]
fn inline_media_is_scoped_to_local_wsl_desktop() {
    assert!(uses_inline_images(
        ConnectionOrigin::Stdio,
        Some("Codex Desktop"),
        Some("NixOS")
    ));
    assert!(!uses_inline_images(
        ConnectionOrigin::Stdio,
        Some("other-client"),
        Some("NixOS")
    ));
    assert!(!uses_inline_images(
        ConnectionOrigin::InProcess,
        Some("Codex Desktop"),
        Some("NixOS")
    ));
    assert!(!uses_inline_images(
        ConnectionOrigin::Stdio,
        Some("Codex Desktop"),
        /*distribution*/ None
    ));
    assert!(!uses_inline_images(
        ConnectionOrigin::Stdio,
        Some("Codex Desktop"),
        Some("")
    ));
}

fn generated_image() -> ThreadItem {
    ThreadItem::ImageGeneration(ImageGenerationItem {
        id: "image-1".to_string(),
        status: "completed".to_string(),
        revised_prompt: Some("A square".to_string()),
        result: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=".to_string(),
        transparent_background: None,
        failure: None,
        saved_path: Some(test_path_buf("/tmp/generated.png").abs()),
        imagegen_request_id: None,
        generation_id: None,
    })
}

fn completed_notification(item: ThreadItem) -> OutgoingMessage {
    OutgoingMessage::AppServerNotification(ServerNotificationEnvelope {
        notification: ServerNotification::ItemCompleted(ItemCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            item,
            completed_at_ms: 1,
        }),
        emitted_at_ms: Some(1),
    })
}

#[test]
fn live_image_uses_inline_result_without_changing_source_item() {
    let source = generated_image();
    let original = serde_json::to_value(&source).expect("serialize source");
    let mut notification = completed_notification(source.clone());
    let mut expected = serde_json::to_value(&notification).expect("serialize notification");
    expected["params"]["item"]
        .as_object_mut()
        .unwrap()
        .remove("savedPath");

    prefer_inline_images(&mut notification);

    assert_eq!(serde_json::to_value(notification).unwrap(), expected);
    assert_eq!(serde_json::to_value(source).unwrap(), original);
}

#[test]
fn reopened_image_page_uses_inline_result() {
    let mut message = OutgoingMessage::Response(OutgoingResponse {
        id: RequestId::Integer(1),
        result: Box::new(ClientResponsePayload::ThreadItemsList(
            ThreadItemsListResponse {
                data: vec![ThreadItemEntry {
                    turn_id: "turn-1".to_string(),
                    item: generated_image(),
                }],
                next_cursor: None,
                backwards_cursor: None,
            },
        )),
    });
    let mut expected = serde_json::to_value(&message).expect("serialize page");
    expected["result"]["data"][0]["item"]
        .as_object_mut()
        .unwrap()
        .remove("savedPath");

    prefer_inline_images(&mut message);

    assert_eq!(serde_json::to_value(message).unwrap(), expected);
}

#[test]
fn image_without_inline_data_retains_its_only_source() {
    let mut image = generated_image();
    if let ThreadItem::ImageGeneration(image) = &mut image {
        image.result.clear();
    }
    let mut message = completed_notification(image);
    let expected = serde_json::to_value(&message).unwrap();
    prefer_inline_images(&mut message);
    assert_eq!(serde_json::to_value(message).unwrap(), expected);
}

#[test]
fn uninitialized_and_non_stdio_connections_keep_native_paths() {
    for origin in [ConnectionOrigin::Stdio, ConnectionOrigin::InProcess] {
        let session = ConnectionSessionState::new(origin);
        let mut message = completed_notification(generated_image());
        let expected = serde_json::to_value(&message).unwrap();
        prepare_generated_images(&mut message, &session);
        assert_eq!(serde_json::to_value(message).unwrap(), expected);
    }
}
