//! `commands` declarations.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use vy_host_api::{CallError, Reply};

/// `commands.registerCommand` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CommandsRegisterParams {
    pub id: String,
}

/// `commands.execute` parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct CommandsExecuteParams {
    pub id: String,
    #[serde(default)]
    pub args: Value,
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

    /// Lists the ids of every registered command.
    #[vy_api_schema::vy_api_method]
    fn get_commands(&mut self) -> Result<Reply<Vec<String>>, CallError>;
}
