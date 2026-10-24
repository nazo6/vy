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

/// Marker base for the shared host facade. It has no metadata of its own.
pub trait HostRoot {
    fn dispatch(
        &mut self,
        _method: &str,
        _params: &serde_json::Value,
    ) -> Option<Result<Reply<serde_json::Value>, CallError>> {
        None
    }
}

pub fn host_root_methods() -> Vec<&'static vy_api_schema::MethodDecl> {
    Vec::new()
}

pub fn host_root_namespaces() -> Vec<&'static vy_api_schema::NamespaceDecl> {
    Vec::new()
}

pub fn host_root_events() -> Vec<&'static vy_api_schema::EventDecl> {
    Vec::new()
}

crate::host_api! {
    trait HostApi: HostRoot {
        base_methods_fn = host_root_methods;
        base_namespaces_fn = host_root_namespaces;
        base_events_fn = host_root_events;
        traits = [
            config => ConfigReadApi,
            context => ContextApi,
            document => DocumentApi,
            fs => FsApi,
            log => LogApi,
            process => ProcessApi,
            workspace => WorkspaceApi,
        ];
        methods_fn = host_api_methods;
        namespaces_fn = host_api_namespaces;
        events_fn = host_api_events;
    }
}

/// Builds a facade trait and its codegen surface from namespace traits.
///
/// Each entry is `module => Trait`; the module must expose `dispatch`,
/// `METHODS`, `NAMESPACE`, and `EVENTS`, as `#[vy_api_trait]` does.
#[macro_export]
macro_rules! host_api {
    (
        trait $trait:ident : $base:path {
            base_methods_fn = $base_methods:path;
            base_namespaces_fn = $base_namespaces:path;
            base_events_fn = $base_events:path;
            traits = [ $($module:ident => $api:ident),* $(,)? ];
            methods_fn = $methods:ident;
            namespaces_fn = $namespaces:ident;
            events_fn = $events:ident;
        }
    ) => {
        pub trait $trait: $base $(+ $module::$api)* {
            fn dispatch(
                &mut self,
                method: &str,
                params: &::serde_json::Value,
            ) -> Option<
                ::core::result::Result<
                    ::vy_host_api::Reply<::serde_json::Value>,
                    ::vy_host_api::CallError,
                >,
            > {
                if let Some(result) = <Self as $base>::dispatch(self, method, params) {
                    return Some(result);
                }
                $(
                    if let Some(result) = $module::dispatch(self, method, params) {
                        return Some(result);
                    }
                )*
                None
            }
        }

        pub fn $methods() -> Vec<&'static ::vy_api_schema::MethodDecl> {
            let mut methods = $base_methods();
            $(
                methods.extend($module::METHODS.iter());
            )*
            let mut seen = ::std::collections::BTreeSet::new();
            methods.retain(|decl| seen.insert(decl.method));
            methods
        }

        pub fn $namespaces() -> Vec<&'static ::vy_api_schema::NamespaceDecl> {
            let mut namespaces = $base_namespaces();
            $(
                namespaces.push(&$module::NAMESPACE);
            )*
            let mut seen = ::std::collections::BTreeSet::new();
            namespaces.retain(|decl| seen.insert(decl.path));
            namespaces
        }

        pub fn $events() -> Vec<&'static ::vy_api_schema::EventDecl> {
            let mut events = $base_events();
            $(
                events.extend($module::EVENTS.iter());
            )*
            let mut seen = ::std::collections::BTreeSet::new();
            events.retain(|decl| seen.insert(decl.topic));
            events
        }
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
