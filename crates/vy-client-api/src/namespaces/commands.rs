//! `commands` declarations.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl};

use crate::method_decl;

/// `commands.registerCommand` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CommandsRegisterParams {
    pub id: String,
}

/// `commands.execute` parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CommandsExecuteParams {
    pub id: String,
    #[serde(default)]
    #[ts(type = "unknown")]
    pub args: Value,
}

method_decl!(
    REGISTER_COMMAND,
    "commands.registerCommand",
    CommandsRegisterParams,
    (),
    "Registers a command handler and returns a disposable registration.",
    manual
);
method_decl!(
    EXECUTE,
    "commands.execute",
    CommandsExecuteParams,
    (),
    "Runs a command by id with optional arguments.",
    typed_args = "CommandMap"
);
method_decl!(
    GET_COMMANDS,
    "commands.getCommands",
    Vec<String>,
    "Lists the ids of every registered command."
);

/// Every method in this namespace.
pub static METHODS: &[MethodDecl] = &[REGISTER_COMMAND, EXECUTE, GET_COMMANDS];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "commands",
    doc: "Registers command handlers and runs commands by id.",
    sugar: Some("CommandsSugar"),
};
