//! `commands` declarations.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use vy_host_api::{CallError, Reply};

use crate::command::{ArgKind, CommandCategory, PaletteVisibility, RangeSpec};

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

/// One positional argument of a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommandArgInfo {
    pub name: String,
    pub kind: ArgKind,
    pub choices: Vec<String>,
    pub required: bool,
    pub rest: bool,
}

/// Metadata for a registered command, used by the palette and the Ex resolver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
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
