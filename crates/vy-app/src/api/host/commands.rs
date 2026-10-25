//! Client implementation of the `commands` host trait.

use vy_client_api::namespaces::commands::{
    CommandArgInfo, CommandInfo, CommandsApi, CommandsExecuteParams, CommandsRegisterParams,
};
use vy_host_api::{CallError, Reply};

use super::ClientHost;
use crate::commands::{
    ArgList, BuiltinCommand, CommandArg, CommandDescriptor, CommandError, CommandSource,
    DocumentCommand, KeymapArgs, PaneCommand, SettingCommand, UiCommand, argument_error,
    validate_json_args,
};
use crate::plugin_host::HostAction;

impl CommandsApi for ClientHost<'_, '_, '_> {
    fn register_command(&mut self, params: CommandsRegisterParams) -> Result<Reply<()>, CallError> {
        let owner = self.call.context.owner.clone();
        let args = ArgList::new(
            params
                .args
                .unwrap_or_default()
                .into_iter()
                .map(|arg| CommandArg {
                    name: arg.name,
                    kind: arg.kind,
                    choices: arg.choices,
                    required: arg.required,
                    rest: arg.rest,
                })
                .collect(),
        );
        self.call
            .context
            .commands
            .register_described(
                params.id,
                owner,
                params.title,
                params.category,
                params.ex_names.unwrap_or_default(),
                args,
            )
            .map_err(|error| match error {
                CommandError::InvalidId(_)
                | CommandError::Reserved(_)
                | CommandError::InvalidExName(_)
                | CommandError::DuplicateExName(_) => CallError::invalid_params(error.to_string()),
                _ => CallError::invalid_state(error.to_string()),
            })?;
        Ok(Reply::Value(()))
    }

    fn execute(&mut self, params: CommandsExecuteParams) -> Result<Reply<()>, CallError> {
        let id = params.id;
        let descriptor = self
            .call
            .context
            .commands
            .descriptors()
            .into_iter()
            .find(|command| command.id == id);

        if let Some(descriptor) = &descriptor
            && (!descriptor.args.is_empty()
                || !matches!(
                    descriptor.source,
                    CommandSource::Init | CommandSource::Plugin(_)
                ))
            && let Err(error) = validate_json_args(&descriptor.args, &params.args)
        {
            return Err(CallError::invalid_params(argument_error(
                &id,
                &descriptor.args,
                &error,
            )));
        }

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
        } else if let Some(command) = SettingCommand::parse(&id) {
            self.call.outcome.action = HostAction::Setting {
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
                name: arg.name.clone(),
                kind: arg.kind,
                choices: arg.choices.clone(),
                required: arg.required,
                rest: arg.rest,
            })
            .collect(),
        range: command.range,
        bang: command.bang,
    }
}

fn command_args(value: &serde_json::Value) -> KeymapArgs {
    KeymapArgs {
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
