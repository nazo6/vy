//! Client implementation of the `commands` host trait.

use vy_client_api::namespaces::commands::{
    CommandsApi, CommandsExecuteParams, CommandsRegisterParams,
};
use vy_host_api::{CallError, Reply};

use super::ClientHost;
use crate::commands::{BuiltinCommand, CommandArgs, DocumentCommand, PaneCommand, UiCommand};
use crate::plugin_host::HostAction;

impl CommandsApi for ClientHost<'_, '_, '_> {
    fn register_command(&mut self, params: CommandsRegisterParams) -> Result<Reply<()>, CallError> {
        let owner = self.call.context.owner.clone();
        self.call
            .context
            .commands
            .register(params.id, owner)
            .map_err(|error| CallError::invalid_state(error.to_string()))?;
        Ok(Reply::Value(()))
    }

    fn execute(&mut self, params: CommandsExecuteParams) -> Result<Reply<()>, CallError> {
        let id = params.id;
        let arguments = if params.args.is_null() {
            None
        } else {
            Some(params.args.clone())
        };

        if let Some(command) = UiCommand::parse(&id) {
            self.call.outcome.action = HostAction::Ui {
                command,
                args: arguments,
            };
        } else if let Some(command) = PaneCommand::parse(&id) {
            self.call.outcome.action = HostAction::Pane(command);
        } else if let Some(command) = DocumentCommand::parse(&id) {
            self.call.outcome.action = HostAction::Document {
                command,
                args: arguments,
            };
        } else if let Some(command) = BuiltinCommand::parse(&id) {
            let args = command_args(&params.args);
            self.call.outcome.action = self.call.context.editor.execute(command, args).into();
        } else if self.call.context.commands.is_registered(&id) {
            self.call.outcome.action = HostAction::Execute {
                id,
                args: params.args.clone(),
            };
        } else {
            return Err(CallError::invalid_state(format!("unknown command: {id}")));
        }
        Ok(Reply::Value(()))
    }

    fn get_commands(&mut self) -> Result<Reply<Vec<String>>, CallError> {
        let mut ids: Vec<String> = BuiltinCommand::all()
            .iter()
            .map(|command| command.id().to_string())
            .chain(
                PaneCommand::all()
                    .iter()
                    .map(|command| command.id().to_string()),
            )
            .chain(
                DocumentCommand::all()
                    .iter()
                    .map(|command| command.id().to_string()),
            )
            .chain(
                UiCommand::all()
                    .iter()
                    .map(|command| command.id().to_string()),
            )
            .chain(self.call.context.commands.script_commands())
            .collect();
        ids.sort();
        Ok(Reply::Value(ids))
    }
}

fn command_args(value: &serde_json::Value) -> CommandArgs {
    CommandArgs {
        count: value
            .get("count")
            .and_then(serde_json::Value::as_u64)
            .map(|count| count as u32),
        register: value
            .get("register")
            .and_then(serde_json::Value::as_str)
            .and_then(|register| register.chars().next()),
    }
}
