//! Client implementation of the `commands` host trait.

use vy_client_api::namespaces::commands::{
    CommandArgInfo, CommandInfo, CommandsApi, CommandsExecuteParams, CommandsRegisterParams,
};
use vy_host_api::{CallError, Reply};

use super::ClientHost;
use crate::commands::{
    BuiltinCommand, CommandArgs, CommandDescriptor, DocumentCommand, PaneCommand, UiCommand,
};
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

    fn get_commands(&mut self) -> Result<Reply<Vec<CommandInfo>>, CallError> {
        let mut descriptors = self.call.context.commands.descriptors();
        descriptors.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(Reply::Value(descriptors.iter().map(command_info).collect()))
    }
}

fn command_info(command: &CommandDescriptor) -> CommandInfo {
    CommandInfo {
        id: command.id.clone(),
        title: command.title.clone(),
        category: command.category,
        palette: command.palette,
        ex_names: command.ex_names.clone(),
        args: command
            .args
            .args()
            .iter()
            .map(|arg| CommandArgInfo {
                name: arg.name.to_string(),
                kind: arg.kind,
                choices: arg
                    .choices
                    .iter()
                    .map(|choice| (*choice).to_string())
                    .collect(),
                required: arg.required,
                rest: arg.rest,
            })
            .collect(),
        range: command.range,
        bang: command.bang,
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
