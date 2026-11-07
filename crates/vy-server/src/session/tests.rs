use super::*;
use vy_core::{Position, Range, Selection, SplitDirection, TextEdit};
use vy_proto::value::object;
use vy_proto::{
    DidChangeParams, InitializeParams, LayoutGetResult, PROTOCOL_VERSION, PaneOpenParams,
    PaneOpenResult, RpcResult, SyncDocument, SyncRequestParams, SyncRequestResult,
};

fn request(session: &mut Session, id: u64, method: &str, params: Value) -> Outcome {
    session.handle(RpcMessage::request(id, 1, method, params))
}

fn initialize(session: &mut Session) {
    let outcome = session.handle(RpcMessage::request(
        1,
        0,
        "initialize",
        to_value(&InitializeParams {
            protocol_version: PROTOCOL_VERSION,
            client_version: "0.1.0".into(),
            client_capabilities: Vec::new(),
            workspace: Some("file:///tmp".into()),
            resume_session: None,
        }),
    ));
    assert!(!outcome.close);
    assert_eq!(outcome.messages.len(), 2);
}

fn ok_value(message: &RpcMessage) -> &Value {
    match message {
        RpcMessage::Response {
            result: RpcResult::Ok { value },
            ..
        } => value,
        other => panic!("expected Ok: {other:?}"),
    }
}

fn is_error(message: &RpcMessage, code: ErrorCode) -> bool {
    matches!(
        message,
        RpcMessage::Response {
            result: RpcResult::Err { code: actual, .. },
            ..
        } if *actual == code
    )
}

fn open_untitled(session: &mut Session) -> (u64, u64) {
    let outcome = request(
        session,
        2,
        "pane/open",
        to_value(&PaneOpenParams {
            view: None,
            uri: "vy:untitled".into(),
            placement: None,
        }),
    );
    let value = ok_value(&outcome.messages[0]);
    let opened: PaneOpenResult = decode_params(value).expect("PaneOpenResult");
    (opened.document_id, opened.pane_id)
}

fn insert_change(document_id: u64, base_version: u64, text: &str) -> RpcMessage {
    RpcMessage::notification(
        "buffer/did_change",
        to_value(&DidChangeParams {
            document_id,
            base_version,
            version: base_version + 1,
            transaction_id: 1,
            edits: vec![TextEdit::new(
                Range::new(Position::new(0, 0), Position::new(0, 0)),
                text,
            )],
            selection_before: Vec::new(),
            selection_after: Vec::new(),
        }),
    )
}

#[test]
fn answers_initialize_and_notifies() {
    let mut session = Session::new();
    let outcome = session.handle(RpcMessage::request(
        1,
        0,
        "initialize",
        object([
            ("protocol_version", Value::from(PROTOCOL_VERSION)),
            ("workspace", Value::from("file:///tmp")),
        ]),
    ));

    assert!(!outcome.close);
    assert_eq!(outcome.messages.len(), 2);
    assert_eq!(
        get(ok_value(&outcome.messages[0]), "session_id").and_then(Value::as_u64),
        Some(1)
    );

    match &outcome.messages[1] {
        RpcMessage::Notification { method, params } => {
            assert_eq!(method, "initialized");
            assert_eq!(
                get(params, "protocol_version").and_then(Value::as_u64),
                Some(PROTOCOL_VERSION as u64)
            );
            assert_eq!(get(params, "session_id").and_then(Value::as_u64), Some(1));
        }
        other => panic!("expected a notification: {other:?}"),
    }

    assert_eq!(
        session.workspace,
        Some(Uri::parse("file:///tmp").expect("uri"))
    );
}

#[test]
fn closes_on_a_mismatched_protocol_version() {
    let mut session = Session::new();
    let outcome = session.handle(RpcMessage::request(
        1,
        0,
        "initialize",
        object([("protocol_version", Value::from(99u64))]),
    ));

    assert!(outcome.close);
    assert!(is_error(
        &outcome.messages[0],
        ErrorCode::UnsupportedProtocolVersion
    ));
}

#[test]
fn requires_initialize_first() {
    let mut session = Session::new();
    let outcome = request(&mut session, 1, "pane/open", Value::Map(Vec::new()));
    assert!(is_error(&outcome.messages[0], ErrorCode::InvalidRequest));
}

#[test]
fn rejects_a_foreign_session_id() {
    let mut session = Session::new();
    initialize(&mut session);
    let outcome = session.handle(RpcMessage::request(
        2,
        9,
        "buffer/snapshot",
        Value::Map(Vec::new()),
    ));
    assert!(is_error(&outcome.messages[0], ErrorCode::InvalidRequest));
}

#[test]
fn opens_a_document_and_returns_a_snapshot() {
    let mut session = Session::new();
    initialize(&mut session);
    let (document_id, pane_id) = open_untitled(&mut session);

    assert_eq!(document_id, 1);
    assert_eq!(pane_id, 1);

    let outcome = request(
        &mut session,
        3,
        "buffer/snapshot",
        object([("document_id", Value::from(document_id))]),
    );
    let value = ok_value(&outcome.messages[0]);
    assert_eq!(get(value, "version").and_then(Value::as_u64), Some(0));
    assert_eq!(get(value, "text").and_then(Value::as_str), Some(""));
}

#[test]
fn applies_a_change_and_undoes_it() {
    let mut session = Session::new();
    initialize(&mut session);
    let (document_id, _) = open_untitled(&mut session);

    let outcome = session.handle(insert_change(document_id, 0, "こんにちは"));
    assert!(outcome.messages.is_empty());

    let outcome = request(
        &mut session,
        3,
        "buffer/snapshot",
        object([("document_id", Value::from(document_id))]),
    );
    let value = ok_value(&outcome.messages[0]);
    assert_eq!(
        get(value, "text").and_then(Value::as_str),
        Some("こんにちは")
    );
    assert_eq!(get(value, "version").and_then(Value::as_u64), Some(1));

    let outcome = request(
        &mut session,
        4,
        "buffer/undo",
        object([("document_id", Value::from(document_id))]),
    );
    let value = ok_value(&outcome.messages[0]);
    assert_eq!(get(value, "version").and_then(Value::as_u64), Some(2));
    match get(value, "edits") {
        Some(Value::Array(batches)) => {
            assert_eq!(batches.len(), 1);
            match &batches[0] {
                Value::Array(edits) => assert_eq!(edits.len(), 1),
                other => panic!("a batch is not an array: {other:?}"),
            }
        }
        other => panic!("edits is not an array: {other:?}"),
    }

    let outcome = request(
        &mut session,
        5,
        "buffer/snapshot",
        object([("document_id", Value::from(document_id))]),
    );
    assert_eq!(
        get(ok_value(&outcome.messages[0]), "text").and_then(Value::as_str),
        Some("")
    );
}

#[test]
fn asks_for_a_resync_when_the_base_version_is_stale() {
    let mut session = Session::new();
    initialize(&mut session);
    let (document_id, _) = open_untitled(&mut session);

    let outcome = session.handle(insert_change(document_id, 9, "x"));
    assert_eq!(outcome.messages.len(), 1);
    match &outcome.messages[0] {
        RpcMessage::Notification { method, params } => {
            assert_eq!(method, "buffer/resync");
            assert_eq!(
                get(params, "document_id").and_then(Value::as_u64),
                Some(document_id)
            );
        }
        other => panic!("expected a notification: {other:?}"),
    }
}

#[test]
fn reports_unknown_methods() {
    let mut session = Session::new();
    initialize(&mut session);
    let outcome = request(&mut session, 2, "nope/nope", Value::Map(Vec::new()));
    assert!(is_error(&outcome.messages[0], ErrorCode::InvalidRequest));
}

#[test]
fn closes_on_shutdown() {
    let mut session = Session::new();
    initialize(&mut session);
    let outcome = request(&mut session, 2, "shutdown", Value::Map(Vec::new()));
    assert!(outcome.close);
}

// Make sure the position types match the wire format.
#[test]
fn serializes_core_types_as_maps() {
    let edit = TextEdit::new(Range::new(Position::new(1, 2), Position::new(1, 4)), "abc");
    let value = to_value(&edit);
    assert_eq!(get(&value, "text").and_then(Value::as_str), Some("abc"));
    let start = get(&value, "range")
        .and_then(|range| get(range, "start"))
        .expect("start");
    assert_eq!(get(start, "line").and_then(Value::as_u64), Some(1));
    assert_eq!(get(start, "character").and_then(Value::as_u64), Some(2));

    let selection = Selection::caret(Position::new(0, 0));
    let decoded: Selection = decode_params(&to_value(&selection)).expect("selection round trip");
    assert_eq!(decoded, selection);
}

fn layout_notification(outcome: &Outcome) -> (&Value, &Value) {
    match &outcome.messages[1] {
        RpcMessage::Notification { method, params } => {
            assert_eq!(method, "layout/did_change");
            let layout = get(params, "layout").expect("layout");
            (params, layout)
        }
        other => panic!("expected a layout notification: {other:?}"),
    }
}

#[test]
fn opens_a_second_pane_next_to_the_first_and_shares_the_buffer() {
    let mut session = Session::new();
    initialize(&mut session);
    let (document_id, pane_id) = open_untitled(&mut session);

    let outcome = request(
        &mut session,
        3,
        "pane/open",
        object([
            ("uri", Value::from("vy:untitled")),
            (
                "placement",
                object([
                    ("target", Value::from(pane_id)),
                    ("direction", Value::from("right")),
                ]),
            ),
        ]),
    );

    let value = ok_value(&outcome.messages[0]);
    assert_eq!(
        get(value, "document_id").and_then(Value::as_u64),
        Some(document_id),
        "an open URI reuses the document"
    );
    assert_eq!(get(value, "pane_id").and_then(Value::as_u64), Some(2));

    let (params, layout) = layout_notification(&outcome);
    assert_eq!(get(params, "active_pane").and_then(Value::as_u64), Some(2));
    assert_eq!(get(params, "reason").and_then(Value::as_str), Some("open"));
    assert_eq!(
        decode_params::<vy_core::LayoutNode>(layout),
        Some(vy_core::LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(vy_core::LayoutNode::Leaf { pane_id }),
            second: Box::new(vy_core::LayoutNode::Leaf { pane_id: 2 }),
        })
    );
}

#[test]
fn opens_a_custom_view_pane() {
    let mut session = Session::new();
    initialize(&mut session);
    let (document_id, pane_id) = open_untitled(&mut session);

    let outcome = request(
        &mut session,
        3,
        "pane/open",
        object([
            ("view", Value::from("example.tree")),
            ("uri", Value::from("vy:untitled")),
            (
                "placement",
                object([
                    ("target", Value::from(pane_id)),
                    ("direction", Value::from("left")),
                ]),
            ),
        ]),
    );

    let value = ok_value(&outcome.messages[0]);
    assert_eq!(get(value, "pane_id").and_then(Value::as_u64), Some(2));
    assert_eq!(
        get(value, "document_id").and_then(Value::as_u64),
        Some(document_id)
    );
}

#[test]
fn focuses_and_closes_panes_through_the_layout_tree() {
    let mut session = Session::new();
    initialize(&mut session);
    let (_, first) = open_untitled(&mut session);
    let outcome = request(
        &mut session,
        3,
        "pane/open",
        object([
            ("uri", Value::from("vy:untitled")),
            (
                "placement",
                object([
                    ("target", Value::from(first)),
                    ("direction", Value::from("below")),
                ]),
            ),
        ]),
    );
    let second = get(ok_value(&outcome.messages[0]), "pane_id")
        .and_then(Value::as_u64)
        .expect("pane_id");

    let outcome = request(
        &mut session,
        4,
        "pane/focus",
        object([("pane_id", Value::from(first))]),
    );
    let (params, _) = layout_notification(&outcome);
    assert_eq!(
        get(params, "active_pane").and_then(Value::as_u64),
        Some(first)
    );

    let outcome = request(
        &mut session,
        5,
        "pane/close",
        object([("pane_id", Value::from(first))]),
    );
    let (params, layout) = layout_notification(&outcome);
    assert_eq!(get(params, "reason").and_then(Value::as_str), Some("close"));
    assert_eq!(
        decode_params::<vy_core::LayoutNode>(layout),
        Some(vy_core::LayoutNode::Leaf { pane_id: second })
    );

    let outcome = request(
        &mut session,
        6,
        "pane/close",
        object([("pane_id", Value::from(second))]),
    );
    assert!(is_error(&outcome.messages[0], ErrorCode::InvalidRequest));
}

#[test]
fn rejects_closing_unsaved_document_without_force() {
    let mut session = Session::new();
    initialize(&mut session);
    let (document_id, first) = open_untitled(&mut session);

    // Make the buffer dirty by applying a change
    session.handle(insert_change(document_id, 0, "hello"));

    // Open a second pane with a different document
    let outcome = request(
        &mut session,
        4,
        "pane/open",
        object([
            ("uri", Value::from("vy:untitled-2")),
            (
                "placement",
                object([
                    ("target", Value::from(first)),
                    ("direction", Value::from("below")),
                ]),
            ),
        ]),
    );
    let _second = get(ok_value(&outcome.messages[0]), "pane_id")
        .and_then(Value::as_u64)
        .expect("pane_id");

    // Attempting to close the dirty pane without force fails
    let outcome = request(
        &mut session,
        5,
        "pane/close",
        object([
            ("pane_id", Value::from(first)),
            ("force", Value::from(false)),
        ]),
    );
    assert!(is_error(&outcome.messages[0], ErrorCode::InvalidRequest));
    let msg = match &outcome.messages[0] {
        RpcMessage::Response {
            result: RpcResult::Err { message, .. },
            ..
        } => message.as_str(),
        _ => "",
    };
    assert!(msg.contains("No write since last change"));

    // Closing with force succeeds
    let outcome = request(
        &mut session,
        6,
        "pane/close",
        object([
            ("pane_id", Value::from(first)),
            ("force", Value::from(true)),
        ]),
    );
    assert_eq!(
        get(ok_value(&outcome.messages[0]), "pane_id").and_then(Value::as_u64),
        Some(first)
    );
}

#[test]
fn allows_closing_shared_dirty_document_without_force() {
    let mut session = Session::new();
    initialize(&mut session);
    let (document_id, first) = open_untitled(&mut session);

    // Make the buffer dirty
    session.handle(insert_change(document_id, 0, "dirty"));

    // Open another pane referencing the SAME document
    let outcome = request(
        &mut session,
        3,
        "pane/open",
        object([
            ("uri", Value::from("vy:untitled")),
            (
                "placement",
                object([
                    ("target", Value::from(first)),
                    ("direction", Value::from("right")),
                ]),
            ),
        ]),
    );
    let second = get(ok_value(&outcome.messages[0]), "pane_id")
        .and_then(Value::as_u64)
        .expect("pane_id");

    // Closing the first pane without force succeeds because the document is still open in second
    let outcome = request(
        &mut session,
        4,
        "pane/close",
        object([
            ("pane_id", Value::from(first)),
            ("force", Value::from(false)),
        ]),
    );
    assert_eq!(
        get(ok_value(&outcome.messages[0]), "pane_id").and_then(Value::as_u64),
        Some(first)
    );

    // But closing the last remaining pane without force now fails
    let outcome = request(
        &mut session,
        5,
        "pane/close",
        object([
            ("pane_id", Value::from(second)),
            ("force", Value::from(false)),
        ]),
    );
    assert!(is_error(&outcome.messages[0], ErrorCode::InvalidRequest));
}

#[test]
fn returns_the_layout_with_its_panes() {
    let mut session = Session::new();
    initialize(&mut session);
    let (document_id, pane_id) = open_untitled(&mut session);

    let outcome = request(&mut session, 3, "layout/get", Value::Map(Vec::new()));
    let value = ok_value(&outcome.messages[0]);
    assert_eq!(get(value, "revision").and_then(Value::as_u64), Some(1));
    assert_eq!(
        get(value, "active_pane").and_then(Value::as_u64),
        Some(pane_id)
    );

    let panes = match get(value, "panes") {
        Some(Value::Array(panes)) => panes,
        other => panic!("expected panes: {other:?}"),
    };
    assert_eq!(panes.len(), 1);
    assert_eq!(
        get(&panes[0], "document_id").and_then(Value::as_u64),
        Some(document_id)
    );
    assert_eq!(
        get(&panes[0], "view").and_then(Value::as_str),
        Some("editor")
    );
}

#[test]
fn editor_open_replaces_the_document_in_the_active_pane() {
    let mut session = Session::new();
    initialize(&mut session);
    let (first_document, pane_id) = open_untitled(&mut session);

    let outcome = request(
        &mut session,
        3,
        "editor/open",
        object([("uri", Value::from("vy:second"))]),
    );
    let value = ok_value(&outcome.messages[0]);
    let second_document = get(value, "document_id")
        .and_then(Value::as_u64)
        .expect("document_id");
    assert_ne!(first_document, second_document);
    assert_eq!(get(value, "pane_id").and_then(Value::as_u64), Some(pane_id));
    assert_eq!(get(value, "replaced").and_then(Value::as_bool), Some(true));

    let outcome = request(
        &mut session,
        4,
        "editor/open",
        object([("uri", Value::from("vy:untitled"))]),
    );
    let value = ok_value(&outcome.messages[0]);
    assert_eq!(
        get(value, "document_id").and_then(Value::as_u64),
        Some(first_document)
    );
}

#[test]
fn saves_a_file_back_to_disk() {
    let mut session = Session::new();
    initialize(&mut session);

    let path = std::env::temp_dir().join(format!("vy-save-{}-{}.txt", std::process::id(), line!()));
    std::fs::write(&path, "old").expect("write fixture");
    let uri = format!("file://{}", path.display());

    let outcome = request(
        &mut session,
        2,
        "pane/open",
        object([("uri", Value::from(uri))]),
    );
    let document_id = get(ok_value(&outcome.messages[0]), "document_id")
        .and_then(Value::as_u64)
        .expect("document_id");

    session.handle(insert_change(document_id, 0, "new"));

    let outcome = request(
        &mut session,
        3,
        "document/save",
        object([("document_id", Value::from(document_id))]),
    );
    let value = ok_value(&outcome.messages[0]);
    assert_eq!(
        get(value, "document_id").and_then(Value::as_u64),
        Some(document_id)
    );
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "newold");

    std::fs::remove_file(path).ok();
}

#[test]
fn fs_and_process_operations() {
    let mut session = Session::new();
    initialize(&mut session);

    let temp_dir = std::env::temp_dir().join(format!("vy-fs-test-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let file_path = temp_dir.join("hello.txt");
    let file_uri = format!("file://{}", file_path.display());
    let dir_uri = format!("file://{}", temp_dir.display());

    // 1. fs/write_file
    let outcome = request(
        &mut session,
        10,
        "fs/write_file",
        object([
            ("uri", Value::from(file_uri.clone())),
            ("text", Value::from("Hello, World!")),
        ]),
    );
    let _ = ok_value(&outcome.messages[0]);

    // 2. fs/exists
    let outcome = request(
        &mut session,
        11,
        "fs/exists",
        object([("uri", Value::from(file_uri.clone()))]),
    );
    let val = ok_value(&outcome.messages[0]);
    assert_eq!(get(val, "exists").and_then(Value::as_bool), Some(true));

    // 3. fs/read_text_file
    let outcome = request(
        &mut session,
        12,
        "fs/read_text_file",
        object([("uri", Value::from(file_uri.clone()))]),
    );
    let val = ok_value(&outcome.messages[0]);
    assert_eq!(
        get(val, "text").and_then(Value::as_str),
        Some("Hello, World!")
    );

    // 4. fs/stat
    let outcome = request(
        &mut session,
        13,
        "fs/stat",
        object([("uri", Value::from(file_uri.clone()))]),
    );
    let val = ok_value(&outcome.messages[0]);
    let stat = get(val, "stat").expect("stat");
    assert_eq!(get(stat, "is_file").and_then(Value::as_bool), Some(true));
    assert_eq!(get(stat, "size").and_then(Value::as_u64), Some(13));

    // 5. fs/read_dir
    let outcome = request(
        &mut session,
        14,
        "fs/read_dir",
        object([("uri", Value::from(dir_uri.clone()))]),
    );
    let val = ok_value(&outcome.messages[0]);
    let entries = get(val, "entries")
        .and_then(Value::as_array)
        .expect("entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        get(&entries[0], "name").and_then(Value::as_str),
        Some("hello.txt")
    );

    // 6. process/exec
    let outcome = request(
        &mut session,
        15,
        "process/exec",
        object([
            ("command", Value::from("echo")),
            ("args", Value::Array(vec![Value::from("vy-process-test")])),
        ]),
    );
    let val = ok_value(&outcome.messages[0]);
    assert_eq!(get(val, "exit_code").and_then(Value::as_i64), Some(0));
    assert!(
        get(val, "stdout")
            .and_then(Value::as_str)
            .unwrap()
            .contains("vy-process-test")
    );

    // Clean up
    std::fs::remove_dir_all(&temp_dir).ok();
}

fn temp_session(name: &str, workspace: &std::path::Path) -> (std::path::PathBuf, Session) {
    let data = std::env::temp_dir().join(format!(
        "vy-session-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&data);
    std::fs::create_dir_all(&data).expect("data dir");
    let mut session = Session::new();
    session.set_data_dir(data.clone());
    let workspace_uri = format!("file://{}", workspace.display());
    session.handle(RpcMessage::request(
        1,
        0,
        "initialize",
        to_value(&InitializeParams {
            protocol_version: PROTOCOL_VERSION,
            client_version: "0.1.0".into(),
            client_capabilities: Vec::new(),
            workspace: Some(workspace_uri),
            resume_session: None,
        }),
    ));
    (data, session)
}

#[test]
fn restores_the_layout_and_open_documents_for_a_workspace() {
    let workspace =
        std::env::temp_dir().join(format!("vy-workspace-restore-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&workspace);
    std::fs::create_dir_all(&workspace).expect("workspace");
    let file = workspace.join("note.txt");
    std::fs::write(&file, "hello").expect("write file");
    let file_uri = format!("file://{}", file.display());

    let (data, mut first) = temp_session("restore-first", &workspace);
    let outcome = request(
        &mut first,
        2,
        "pane/open",
        to_value(&PaneOpenParams {
            view: None,
            uri: file_uri.clone(),
            placement: None,
        }),
    );
    let opened: PaneOpenResult = decode_params(ok_value(&outcome.messages[0])).expect("open");
    assert!(!outcome.close);

    let mut second = Session::new();
    second.set_data_dir(data.clone());
    second.handle(RpcMessage::request(
        1,
        0,
        "initialize",
        to_value(&InitializeParams {
            protocol_version: PROTOCOL_VERSION,
            client_version: "0.1.0".into(),
            client_capabilities: Vec::new(),
            workspace: Some(format!("file://{}", workspace.display())),
            resume_session: None,
        }),
    ));

    let outcome = request(&mut second, 2, "layout/get", Value::Nil);
    let layout: LayoutGetResult = decode_params(ok_value(&outcome.messages[0])).expect("layout");
    assert_eq!(layout.active_pane, Some(opened.pane_id));
    assert_eq!(layout.panes.len(), 1);
    assert_eq!(layout.panes[0].pane_id, opened.pane_id);
    assert_eq!(layout.panes[0].uri.as_deref(), Some(file_uri.as_str()));
    assert!(layout.panes[0].document_id.is_some());

    let _ = std::fs::remove_dir_all(&data);
    let _ = std::fs::remove_dir_all(&workspace);
}

#[test]
fn sync_request_replaces_unsaved_text_and_keeps_saved_version() {
    let workspace = std::env::temp_dir().join(format!("vy-workspace-sync-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&workspace);
    std::fs::create_dir_all(&workspace).expect("workspace");
    let file = workspace.join("note.txt");
    std::fs::write(&file, "from disk").expect("write file");
    let file_uri = format!("file://{}", file.display());

    let (data, mut first) = temp_session("sync-first", &workspace);
    let outcome = request(
        &mut first,
        2,
        "pane/open",
        to_value(&PaneOpenParams {
            view: None,
            uri: file_uri.clone(),
            placement: None,
        }),
    );
    let opened: PaneOpenResult = decode_params(ok_value(&outcome.messages[0])).expect("open");

    let mut second = Session::new();
    second.set_data_dir(data.clone());
    second.handle(RpcMessage::request(
        1,
        0,
        "initialize",
        to_value(&InitializeParams {
            protocol_version: PROTOCOL_VERSION,
            client_version: "0.1.0".into(),
            client_capabilities: Vec::new(),
            workspace: Some(format!("file://{}", workspace.display())),
            resume_session: None,
        }),
    ));

    let outcome = request(
        &mut second,
        2,
        "sync/request",
        to_value(&SyncRequestParams {
            documents: vec![SyncDocument {
                uri: file_uri.clone(),
                version: 7,
                saved_version: 5,
                text: Some("unsaved text".into()),
            }],
        }),
    );
    let synced: SyncRequestResult = decode_params(ok_value(&outcome.messages[0])).expect("sync");
    assert_eq!(synced.documents.len(), 1);
    assert_eq!(synced.documents[0].text, "unsaved text");
    assert_eq!(synced.documents[0].version, 7);
    assert_eq!(synced.documents[0].saved_version, 5);
    assert_eq!(synced.documents[0].document_id, opened.document_id);
    let _ = std::fs::remove_dir_all(&data);
    let _ = std::fs::remove_dir_all(&workspace);
}

#[test]
fn resizes_pane_and_notifies_layout_change() {
    let mut session = Session::new();
    initialize(&mut session);
    open_untitled(&mut session);

    // Split to create a second pane
    let outcome = request(
        &mut session,
        3,
        "pane/open",
        to_value(&PaneOpenParams {
            view: None,
            uri: "vy:untitled".into(),
            placement: Some(vy_proto::PanePlacement {
                target: Some(1),
                direction: Some("right".into()),
                ratio: None,
            }),
        }),
    );
    assert_eq!(outcome.messages.len(), 2);

    // Resize pane 1
    let outcome = request(
        &mut session,
        4,
        "pane/resize",
        to_value(&vy_proto::PaneResizeParams {
            pane_id: Some(1),
            direction: Some("horizontal".into()),
            delta: Some(0.1),
            ..Default::default()
        }),
    );
    assert_eq!(outcome.messages.len(), 2);
    let res: vy_proto::PaneResizeResult =
        decode_params(ok_value(&outcome.messages[0])).expect("resize result");
    assert_eq!(res.revision, 3);

    // Verify layout did_change notification was sent
    let notification = &outcome.messages[1];
    let RpcMessage::Notification { method, params } = notification else {
        panic!("expected notification");
    };
    assert_eq!(method, "layout/did_change");
    let layout_notif: vy_proto::LayoutDidChangeNotification =
        decode_params(params).expect("layout did change");
    assert_eq!(layout_notif.reason.as_deref(), Some("resize"));
    let Some(vy_core::LayoutNode::Split { ratio, .. }) = layout_notif.layout else {
        panic!("expected split node");
    };
    assert!((ratio - 0.6).abs() < 1e-4);
}

#[test]
fn test_save_document_atomic_and_external_modification() {
    let mut session = Session::new();
    initialize(&mut session);

    let dir = std::env::temp_dir().join(format!("vy-atomic-save-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file_path = dir.join("test_save.txt");
    std::fs::write(&file_path, "initial line\n").unwrap();
    let file_uri = format!("file://{}", file_path.display());

    let (_untitled_doc, _pane_id) = open_untitled(&mut session);

    // Open the document in the active pane
    let outcome = request(
        &mut session,
        3,
        "editor/open",
        to_value(&vy_proto::EditorOpenParams {
            uri: file_uri.clone(),
            pane_id: None,
        }),
    );
    assert_eq!(outcome.messages.len(), 1);
    let open_res: vy_proto::PaneOpenResult =
        decode_params(ok_value(&outcome.messages[0])).expect("open result");
    let document_id = open_res.document_id;

    // Make an edit
    session.handle(RpcMessage::notification(
        "buffer/did_change",
        to_value(&vy_proto::DidChangeParams {
            document_id,
            base_version: 0,
            version: 1,
            transaction_id: 1,
            edits: vec![TextEdit::insert(
                Position {
                    line: 1,
                    character: 0,
                },
                "appended line\n",
            )],
            selection_before: vec![Selection::caret(Position {
                line: 0,
                character: 0,
            })],
            selection_after: vec![Selection::caret(Position {
                line: 1,
                character: 0,
            })],
        }),
    ));

    // Save document atomically
    let outcome = request(
        &mut session,
        4,
        "document/save",
        to_value(&vy_proto::DocumentSaveParams {
            document_id,
            force: false,
        }),
    );
    assert_eq!(outcome.messages.len(), 1);
    assert!(!outcome.close);
    let saved_content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(saved_content, "initial line\nappended line\n");

    // Modify file externally on disk
    std::thread::sleep(std::time::Duration::from_millis(15));
    std::fs::write(&file_path, "externally modified\n").unwrap();

    // Trying to save without force should fail
    let outcome = request(
        &mut session,
        5,
        "document/save",
        to_value(&vy_proto::DocumentSaveParams {
            document_id,
            force: false,
        }),
    );
    assert_eq!(outcome.messages.len(), 1);
    assert!(is_error(&outcome.messages[0], ErrorCode::InvalidRequest));

    // Saving with force = true should succeed
    let outcome = request(
        &mut session,
        6,
        "document/save",
        to_value(&vy_proto::DocumentSaveParams {
            document_id,
            force: true,
        }),
    );
    assert_eq!(outcome.messages.len(), 1);
    assert!(!is_error(&outcome.messages[0], ErrorCode::InvalidRequest));
    let overwritten_content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(overwritten_content, "initial line\nappended line\n");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_sandbox_enforcement_and_symlinks() {
    let mut session = Session::new();
    let temp_root = std::env::temp_dir().join(format!("vy-sandbox-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_root);
    let ws_dir = temp_root.join("workspace");
    let outside_dir = temp_root.join("outside");
    std::fs::create_dir_all(&ws_dir).expect("ws dir");
    std::fs::create_dir_all(&outside_dir).expect("outside dir");

    let outside_file = outside_dir.join("secret.txt");
    std::fs::write(&outside_file, "super-secret").expect("outside file");

    let inside_sub = ws_dir.join("sub");
    std::fs::create_dir_all(&inside_sub).expect("inside sub dir");
    let inside_file = inside_sub.join("note.txt");
    std::fs::write(&inside_file, "inside note").expect("inside file");

    // Symlink pointing outside
    let link_to_outside = ws_dir.join("link_to_outside");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside_dir, &link_to_outside).expect("symlink outside");

    // Symlink pointing inside
    let link_to_inside = ws_dir.join("link_to_inside");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&inside_sub, &link_to_inside).expect("symlink inside");

    let ws_uri = format!("file://{}", ws_dir.display());
    request(
        &mut session,
        1,
        "initialize",
        object([
            ("protocol_version", Value::from(PROTOCOL_VERSION)),
            ("client_capabilities", Value::Array(Vec::new())),
            ("workspace", Value::from(ws_uri)),
        ]),
    );

    // 1. Reading file inside workspace succeeds
    let outcome = request(
        &mut session,
        2,
        "fs/read_text_file",
        object([(
            "uri",
            Value::from(format!("file://{}", inside_file.display())),
        )]),
    );
    let val = ok_value(&outcome.messages[0]);
    assert_eq!(
        get(val, "text").and_then(Value::as_str),
        Some("inside note")
    );

    // 2. Traversal using `..` is denied
    let traversal_uri = format!("file://{}/../outside/secret.txt", ws_dir.display());
    let outcome = request(
        &mut session,
        3,
        "fs/read_text_file",
        object([("uri", Value::from(traversal_uri))]),
    );
    assert!(is_error(&outcome.messages[0], ErrorCode::CapabilityDenied));

    // 3. Traversal using percent-encoded `%2e%2e` is denied
    let encoded_traversal_uri = format!("file://{}/%2e%2e/outside/secret.txt", ws_dir.display());
    let outcome = request(
        &mut session,
        4,
        "fs/read_text_file",
        object([("uri", Value::from(encoded_traversal_uri))]),
    );
    assert!(is_error(&outcome.messages[0], ErrorCode::CapabilityDenied));

    // 4. Reading outside file directly is denied
    let direct_outside_uri = format!("file://{}", outside_file.display());
    let outcome = request(
        &mut session,
        5,
        "fs/read_text_file",
        object([("uri", Value::from(direct_outside_uri))]),
    );
    assert!(is_error(&outcome.messages[0], ErrorCode::CapabilityDenied));

    #[cfg(unix)]
    {
        // 5. Reading outside file through symlink is denied
        let symlink_outside_file_uri = format!("file://{}/secret.txt", link_to_outside.display());
        let outcome = request(
            &mut session,
            6,
            "fs/read_text_file",
            object([("uri", Value::from(symlink_outside_file_uri))]),
        );
        assert!(is_error(&outcome.messages[0], ErrorCode::CapabilityDenied));

        // 6. Writing through symlink to outside is denied
        let symlink_write_uri = format!("file://{}/new_evil.txt", link_to_outside.display());
        let outcome = request(
            &mut session,
            7,
            "fs/write_file",
            object([
                ("uri", Value::from(symlink_write_uri)),
                ("text", Value::from("evil")),
            ]),
        );
        assert!(is_error(&outcome.messages[0], ErrorCode::CapabilityDenied));

        // 7. Reading inside file through symlink pointing inside succeeds
        let symlink_inside_file_uri = format!("file://{}/note.txt", link_to_inside.display());
        let outcome = request(
            &mut session,
            8,
            "fs/read_text_file",
            object([("uri", Value::from(symlink_inside_file_uri))]),
        );
        let val = ok_value(&outcome.messages[0]);
        assert_eq!(
            get(val, "text").and_then(Value::as_str),
            Some("inside note")
        );
    }

    // 8. Writing outside workspace is denied
    let evil_write_uri = format!("file://{}/../outside/evil.txt", ws_dir.display());
    let outcome = request(
        &mut session,
        9,
        "fs/write_file",
        object([
            ("uri", Value::from(evil_write_uri)),
            ("text", Value::from("evil")),
        ]),
    );
    assert!(is_error(&outcome.messages[0], ErrorCode::CapabilityDenied));

    // 9. Renaming to outside workspace is denied
    let outcome = request(
        &mut session,
        10,
        "fs/rename",
        object([
            (
                "from",
                Value::from(format!("file://{}", inside_file.display())),
            ),
            (
                "to",
                Value::from(format!("file://{}/escaped.txt", outside_dir.display())),
            ),
        ]),
    );
    assert!(is_error(&outcome.messages[0], ErrorCode::CapabilityDenied));

    // 10. Copying from outside workspace is denied
    let outcome = request(
        &mut session,
        11,
        "fs/copy",
        object([
            (
                "from",
                Value::from(format!("file://{}", outside_file.display())),
            ),
            (
                "to",
                Value::from(format!("file://{}/copied.txt", ws_dir.display())),
            ),
        ]),
    );
    assert!(is_error(&outcome.messages[0], ErrorCode::CapabilityDenied));

    // 11. Checking existence outside workspace is denied
    let outcome = request(
        &mut session,
        12,
        "fs/exists",
        object([(
            "uri",
            Value::from(format!("file://{}", outside_file.display())),
        )]),
    );
    assert!(is_error(&outcome.messages[0], ErrorCode::CapabilityDenied));

    let _ = std::fs::remove_dir_all(&temp_root);
}
