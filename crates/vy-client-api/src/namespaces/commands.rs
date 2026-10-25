//! `commands` declarations.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use vy_host_api::{CallError, Reply};

use crate::command::{ArgKind, CommandCategory, PaletteVisibility, RangeSpec};

/// `commands.registerCommand` / `commands.register` parameters.
///
/// Metadata is optional so `registerCommand(id, handler)` can stay a thin sugar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CommandsRegisterParams {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<CommandCategory>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ex_names: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<CommandArgInfo>>,
}

/// `commands.execute` parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct CommandsExecuteParams {
    pub id: String,
    #[serde(default)]
    pub args: Value,
}

/// One positional argument of a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CommandArgInfo {
    pub name: String,
    pub kind: ArgKind,
    pub choices: Vec<String>,
    pub required: bool,
    pub rest: bool,
}

/// Metadata for a registered command, used by the palette and the Ex resolver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CommandInfo {
    pub id: String,
    pub title: String,
    pub category: CommandCategory,
    pub palette: PaletteVisibility,
    pub ex_names: Vec<String>,
    pub args: Vec<CommandArgInfo>,
    pub range: RangeSpec,
    pub bang: bool,
}

/// Registers command handlers and runs commands by id.
#[vy_api_schema::vy_api_trait("commands")]
pub trait CommandsApi {
    /// Registers a command handler and returns a disposable registration.
    #[vy_api_schema::vy_api_method(manual)]
    fn register_command(&mut self, params: CommandsRegisterParams) -> Result<Reply<()>, CallError>;

    /// Runs a command by id with optional arguments.
    #[vy_api_schema::vy_api_method(manual)]
    fn execute(&mut self, params: CommandsExecuteParams) -> Result<Reply<()>, CallError>;

    /// Lists the metadata of every registered command.
    #[vy_api_schema::vy_api_method]
    fn get_commands(&mut self) -> Result<Reply<Vec<CommandInfo>>, CallError>;
}
