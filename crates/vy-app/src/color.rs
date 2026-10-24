//! Semantic color tokens scripts can use for slots and widgets.

pub use vy_client_api::color::ThemeColor;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_token_round_trips() {
        for token in [
            ThemeColor::Default,
            ThemeColor::Muted,
            ThemeColor::Primary,
            ThemeColor::Warning,
            ThemeColor::Secondary,
        ] {
            let json = serde_json::to_string(&token).expect("serialize");
            assert_eq!(json, format!("\"{}\"", token.name()));
            let parsed: ThemeColor = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(parsed, token);
        }
        assert!(ThemeColor::parse("chartreuse").is_none());
        assert!(serde_json::from_str::<ThemeColor>("\"chartreuse\"").is_err());
    }
}
