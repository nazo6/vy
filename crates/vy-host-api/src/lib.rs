//! The host API surface shared by the client and the core.
//!
//! This crate owns the wire types and method declarations that both
//! `vy-app` and `vy-server` expose to plugins. Handlers stay in the host
//! crates; the declarations live here so the TypeScript surface and the
//! backend implementation cannot drift. See ADR 0012.

pub mod document;
pub mod fs;
pub mod workspace;

pub use document::DocumentInfoReply;

/// Declares a host method without a handler.
///
/// The client and server crates attach their own handler to the same
/// declaration, so the wire name and the TypeScript types have one source.
#[macro_export]
macro_rules! method_decl {
    ($name:ident, $wire:literal, $result:ty, $doc:literal) => {
        $crate::method_decl!(
            @build $name,
            $wire,
            None,
            $result,
            $doc
        );
    };
    ($name:ident, $wire:literal, $params:ty, $result:ty, $doc:literal) => {
        $crate::method_decl!(
            @build $name,
            $wire,
            Some(::vy_api_schema::TypeRef {
                name: ::vy_api_schema::ts_name::<$params> as fn() -> String,
                collect: ::vy_api_schema::collect::<$params>
                    as fn(&mut ::vy_api_schema::TypeCollector),
            }),
            $result,
            $doc
        );
    };
    (@build $name:ident, $wire:literal, $params:expr, $result:ty, $doc:literal) => {
        pub static $name: ::vy_api_schema::MethodDecl = ::vy_api_schema::MethodDecl {
            method: $wire,
            params: $params,
            result: ::vy_api_schema::TypeRef {
                name: ::vy_api_schema::ts_name::<$result> as fn() -> String,
                collect: ::vy_api_schema::collect::<$result>
                    as fn(&mut ::vy_api_schema::TypeCollector),
            },
            doc: $doc,
            manual: false,
            typed_key: None,
            typed_args: None,
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
}
