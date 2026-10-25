//! `ui.hover` declarations.

use serde::{Deserialize, Serialize};
use vy_host_api::{CallError, Reply};

/// `ui.hover.show` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct HoverShowParams {
    pub line: usize,
    pub character: usize,
    pub text: String,
}

/// Shows and hides hover popups.
#[vy_api_schema::vy_api_trait("ui.hover")]
pub trait UiHoverApi {
    /// Shows a hover popup at a position.
    #[vy_api_schema::vy_api_method]
    fn show(&mut self, params: HoverShowParams) -> Result<Reply<()>, CallError>;

    /// Hides the hover popup.
    #[vy_api_schema::vy_api_method]
    fn hide(&mut self) -> Result<Reply<()>, CallError>;
}
