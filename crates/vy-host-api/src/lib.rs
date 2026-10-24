//! The host API surface shared by the client and the core.
//!
//! This crate owns the wire types and method declarations that both
//! `vy-app` and `vy-server` expose to plugins. Handlers stay in the host
//! crates; the declarations live here so the TypeScript surface and the
//! backend implementation cannot drift. See ADR 0012.

// Lets macro-generated code use `::vy_host_api` even inside this crate.
extern crate self as vy_host_api;

pub mod config;
pub mod context;
pub mod document;
pub mod fs;
pub mod log;
pub mod process;
pub mod runtime;
pub mod workspace;

pub use document::DocumentInfoReply;
pub use runtime::{CallError, Reply, erase_reply, parse_params};
pub use vy_api_schema::Json;

/// The core-side host surface shared by the client and the server.
///
/// Implementors provide the namespace traits; `dispatch` is the single entry
/// point used by the host loop.
pub trait HostApi: config::ConfigReadApi + fs::FsApi + workspace::WorkspaceApi {
    fn dispatch(
        &mut self,
        method: &str,
        params: &serde_json::Value,
    ) -> Option<Result<Reply<serde_json::Value>, CallError>> {
        if let Some(result) = config::dispatch(self, method, params) {
            return Some(result);
        }
        if let Some(result) = fs::dispatch(self, method, params) {
            return Some(result);
        }
        if let Some(result) = workspace::dispatch(self, method, params) {
            return Some(result);
        }
        None
    }
}

/// Declares a host method without a handler.
///
/// The client and server crates attach their own handler to the same
/// declaration, so the wire name and the TypeScript types have one source.
#[macro_export]
macro_rules! method_decl {
    ($name:ident, $wire:literal, $result:ty, $doc:literal) => {
        $crate::method_decl!(@build $name, $wire, None, $result, $doc, false, None, None);
    };
    ($name:ident, $wire:literal, $params:ty, $result:ty, $doc:literal) => {
        $crate::method_decl!(
            @build
            $name,
            $wire,
            Some(::vy_api_schema::TypeRef {
                name: ::vy_api_schema::ts_name::<$params> as fn() -> String,
                collect: ::vy_api_schema::collect::<$params>
                    as fn(&mut ::vy_api_schema::TypeCollector),
            }),
            $result,
            $doc,
            false,
            None,
            None
        );
    };
    ($name:ident, $wire:literal, $params:ty, $result:ty, $doc:literal, manual) => {
        $crate::method_decl!(
            @build
            $name,
            $wire,
            Some(::vy_api_schema::TypeRef {
                name: ::vy_api_schema::ts_name::<$params> as fn() -> String,
                collect: ::vy_api_schema::collect::<$params>
                    as fn(&mut ::vy_api_schema::TypeCollector),
            }),
            $result,
            $doc,
            true,
            None,
            None
        );
    };
    ($name:ident, $wire:literal, $params:ty, $result:ty, $doc:literal, typed_key = $key:literal) => {
        $crate::method_decl!(
            @build
            $name,
            $wire,
            Some(::vy_api_schema::TypeRef {
                name: ::vy_api_schema::ts_name::<$params> as fn() -> String,
                collect: ::vy_api_schema::collect::<$params>
                    as fn(&mut ::vy_api_schema::TypeCollector),
            }),
            $result,
            $doc,
            false,
            Some($key),
            None
        );
    };
    ($name:ident, $wire:literal, $params:ty, $result:ty, $doc:literal, typed_args = $key:literal) => {
        $crate::method_decl!(
            @build
            $name,
            $wire,
            Some(::vy_api_schema::TypeRef {
                name: ::vy_api_schema::ts_name::<$params> as fn() -> String,
                collect: ::vy_api_schema::collect::<$params>
                    as fn(&mut ::vy_api_schema::TypeCollector),
            }),
            $result,
            $doc,
            false,
            None,
            Some($key)
        );
    };
    (
        @build
        $name:ident,
        $wire:literal,
        $params:expr,
        $result:ty,
        $doc:literal,
        $manual:expr,
        $typed_key:expr,
        $typed_args:expr
    ) => {
        pub static $name: ::vy_api_schema::MethodDecl = ::vy_api_schema::MethodDecl {
            method: $wire,
            params: $params,
            result: ::vy_api_schema::TypeRef {
                name: ::vy_api_schema::ts_name::<$result> as fn() -> String,
                collect: ::vy_api_schema::collect::<$result>
                    as fn(&mut ::vy_api_schema::TypeCollector),
            },
            doc: $doc,
            manual: $manual,
            typed_key: $typed_key,
            typed_args: $typed_args,
        };
    };
}

/// Declares an event without a handler.
#[macro_export]
macro_rules! event_decl {
    ($name:ident, $topic:literal, $namespace:literal, $event:literal, $payload:ty, $doc:literal) => {
        pub static $name: ::vy_api_schema::EventDecl = ::vy_api_schema::EventDecl {
            topic: $topic,
            name: $event,
            namespace: $namespace,
            payload: ::vy_api_schema::TypeRef {
                name: ::vy_api_schema::ts_name::<$payload> as fn() -> String,
                collect: ::vy_api_schema::collect::<$payload>
                    as fn(&mut ::vy_api_schema::TypeCollector),
            },
            doc: $doc,
        };
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use vy_api_schema::MethodDecl;

    fn names(methods: &[MethodDecl]) -> Vec<&'static str> {
        methods.iter().map(|method| method.method).collect()
    }

    #[test]
    fn fs_declares_every_method() {
        assert_eq!(
            names(fs::METHODS),
            vec![
                "fs.readFile",
                "fs.readTextFile",
                "fs.writeFile",
                "fs.readDir",
                "fs.stat",
                "fs.exists",
                "fs.createDir",
                "fs.remove",
                "fs.rename",
                "fs.copy",
            ]
        );
        assert_eq!(fs::NAMESPACE.path, "fs");
    }

    #[test]
    fn workspace_declares_its_methods_and_event() {
        assert_eq!(
            names(workspace::METHODS),
            vec!["workspace.info", "workspace.textDocuments"]
        );
        assert_eq!(workspace::EVENTS.len(), 1);
        assert_eq!(
            workspace::EVENTS[0].topic,
            "workspace.onDidChangeWorkspaceFolders"
        );
    }

    #[test]
    fn process_declares_methods_and_events() {
        assert_eq!(
            names(process::METHODS),
            vec![
                "process.exec",
                "process.spawn",
                "process.kill",
                "process.writeStdin",
            ]
        );
        assert_eq!(process::EVENTS.len(), 3);
    }

    #[test]
    fn document_config_log_and_context_are_shared() {
        assert_eq!(document::METHODS.len(), 4);
        assert_eq!(config::METHODS.len(), 2);
        assert_eq!(config::EVENTS.len(), 1);
        assert_eq!(log::METHODS.len(), 1);
        assert_eq!(context::METHODS.len(), 1);
    }
}
