//! `commands` namespace.

/// Registers command handlers and runs commands by id.
#[vy_api_schema::vy_api_namespace("commands", sugar = CommandsSugar)]
mod api {
    use serde_json::Value;
    use vy_api_schema::{CommandsExecuteParams, CommandsRegisterParams};

    use crate::api::{Call, CallError, Reply};
    use crate::commands::{
        BuiltinCommand, CommandArgs, DocumentCommand, Owner, PaneCommand, UiCommand,
    };
    use crate::plugin_host::HostAction;

    /// Registers a command handler and returns a disposable registration.
    #[vy_api_schema::vy_api_method(manual)]
    fn register_command(
        call: &mut Call<'_, '_>,
        params: CommandsRegisterParams,
    ) -> Result<Reply<()>, CallError> {
        call.context
            .commands
            .register(params.id, Owner::Init)
            .map_err(|error| CallError::invalid_state(error.to_string()))?;
        Ok(Reply::Value(()))
    }

    /// Runs a command by id with optional arguments.
    #[vy_api_schema::vy_api_method(manual)]
    fn execute(
        call: &mut Call<'_, '_>,
        params: CommandsExecuteParams,
    ) -> Result<Reply<()>, CallError> {
        let id = params.id;
        let arguments = if params.args.is_null() {
            None
        } else {
            Some(params.args.clone())
        };

        if let Some(command) = UiCommand::parse(&id) {
            call.outcome.action = HostAction::Ui {
                command,
                args: arguments,
            };
        } else if let Some(command) = PaneCommand::parse(&id) {
            call.outcome.action = HostAction::Pane(command);
        } else if let Some(command) = DocumentCommand::parse(&id) {
            call.outcome.action = HostAction::Document {
                command,
                args: arguments,
            };
        } else if let Some(command) = BuiltinCommand::parse(&id) {
            let args = command_args(&params.args);
            call.outcome.action = call.context.editor.execute(command, args).into();
        } else if call.context.commands.is_registered(&id) {
            if let Some(script) = call.context.script {
                script.execute(id.clone(), params.args);
            }
        } else {
            return Err(CallError::invalid_state(format!("unknown command: {id}")));
        }
        Ok(Reply::Value(()))
    }

    /// Lists the ids of every registered command.
    #[vy_api_schema::vy_api_method]
    fn get_commands(call: &mut Call<'_, '_>) -> Result<Reply<Vec<String>>, CallError> {
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
            .chain(call.context.commands.script_commands())
            .collect();
        ids.sort();
        Ok(Reply::Value(ids))
    }

    fn command_args(args: &Value) -> CommandArgs {
        CommandArgs {
            count: args
                .get("count")
                .and_then(Value::as_u64)
                .map(|count| count as u32),
            register: args
                .get("register")
                .and_then(Value::as_str)
                .and_then(|value| value.chars().next()),
        }
    }
}

pub use self::api::{METHODS, NAMESPACE, dispatch};
