use rmpv::Value;

/// Builds a map with string keys.
pub fn object<'a>(entries: impl IntoIterator<Item = (&'a str, Value)>) -> Value {
    Value::Map(
        entries
            .into_iter()
            .map(|(key, value)| (Value::from(key), value))
            .collect(),
    )
}

/// Looks up a value by a string key.
pub fn get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Map(entries) => {
            entries
                .iter()
                .find_map(|(entry_key, entry_value)| match entry_key {
                    Value::String(name) if name.as_str() == Some(key) => Some(entry_value),
                    _ => None,
                })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_and_reads_an_object() {
        let value = object([("method", "test".into()), ("count", 3.into())]);
        assert_eq!(get(&value, "method").and_then(Value::as_str), Some("test"));
        assert_eq!(get(&value, "count").and_then(Value::as_u64), Some(3));
        assert_eq!(get(&value, "missing"), None);
    }
}
