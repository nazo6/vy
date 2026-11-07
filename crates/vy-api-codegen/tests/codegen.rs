use vy_api_codegen::check_client;
use vy_api_schema::codegen::Output;

#[test]
fn client_declarations_are_up_to_date() {
    let output = Output::packages();
    if let Err(error) = check_client(&output) {
        panic!(
            "Generated API declarations are out of date: {error}\nRun `pnpm generate:api` to update."
        );
    }
}
