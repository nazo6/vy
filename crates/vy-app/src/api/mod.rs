//! Declarations for the host methods the plugin API exposes.
//!
//! Each namespace lives in `namespaces/<name>.rs` inside an inline module
//! annotated with `#[vy_api_namespace]`. The registry macro joins the modules,
//! so adding a method needs no central table. See
//! `docs/adr/0008-declare-plugin-api-with-macros.md`.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use vy_api_schema::MethodDecl;
use vy_js::HostRequest;
use vy_proto::ErrorCode;

use crate::plugin_host::{HostOutcome, RequestContext};

/// Mutable client state one host call may touch.
pub struct Call<'a, 'b> {
    pub context: &'a mut RequestContext<'b>,
    pub request: &'a HostRequest,
    pub outcome: &'a mut HostOutcome,
}

/// The value a handler returns now, or a reply the view sends later.
pub enum Reply<T> {
    Value(T),
    Deferred,
}

impl<T> Reply<T> {
    pub fn value(value: T) -> Self {
        Self::Value(value)
    }
}

/// A failure the shim receives as an exception with a stable code.
pub struct CallError {
    pub(crate) code: ErrorCode,
    pub(crate) message: String,
}

impl CallError {
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidParams,
            message: message.into(),
        }
    }

    pub fn invalid_state(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidState,
            message: message.into(),
        }
    }
}

impl From<serde_json::Error> for CallError {
    fn from(error: serde_json::Error) -> Self {
        Self::invalid_params(error.to_string())
    }
}

/// Deserializes the parameters of one method.
///
/// Missing parameters arrive as `null`; treating that as an empty object keeps
/// methods without parameters from needing a special case.
pub fn parse_params<T: DeserializeOwned>(params: &Value) -> Result<T, CallError> {
    let empty = Value::Object(serde_json::Map::new());
    let value = if params.is_null() { &empty } else { params };
    Ok(serde_json::from_value(value.clone())?)
}

/// Serializes a handler's reply for the script boundary.
pub fn erase_reply<T: Serialize>(reply: Reply<T>) -> Result<Reply<Value>, CallError> {
    match reply {
        Reply::Value(value) => Ok(Reply::Value(serde_json::to_value(value)?)),
        Reply::Deferred => Ok(Reply::Deferred),
    }
}

/// The erased signature of a handler stored in a namespace table.
pub type HandlerFn =
    for<'x, 'y, 'a, 'b> fn(&'x mut Call<'a, 'b>, &'y Value) -> Result<Reply<Value>, CallError>;

/// One method and the function that answers it.
pub struct HandlerDecl {
    pub decl: &'static MethodDecl,
    pub handler: HandlerFn,
}

#[vy_api_schema::vy_api_registry]
mod namespaces {
    mod editor;
}

pub use namespaces::{dispatch, methods, namespace_decls};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declares_the_poc_namespace() {
        let methods = methods();
        let names: Vec<&str> = methods.iter().map(|entry| entry.decl.method).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"editor.mode"));
        assert!(names.contains(&"editor.setMode"));

        let mode = methods
            .iter()
            .find(|entry| entry.decl.method == "editor.mode")
            .expect("editor.mode");
        assert_eq!(
            mode.decl.doc,
            "Returns the editor mode of the focused pane."
        );
        assert!(mode.decl.params.is_none());
        assert_eq!((mode.decl.result)(), "Mode");
        assert!(!mode.decl.manual);

        let set_mode = methods
            .iter()
            .find(|entry| entry.decl.method == "editor.setMode")
            .expect("editor.setMode");
        assert_eq!(set_mode.decl.doc, "Switches the focused pane to a mode.");
        assert_eq!(
            (set_mode.decl.params.expect("params"))(),
            "EditorSetModeParams"
        );
        assert_eq!((set_mode.decl.result)(), "null");

        let namespaces = namespace_decls();
        assert_eq!(namespaces.len(), 1);
        assert_eq!(namespaces[0].path, "editor");
        assert_eq!(namespaces[0].doc, "Reads and edits the active document.");
        assert!(namespaces[0].sugar.is_none());
    }
}
