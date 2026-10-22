use rmpv::Value;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Serializes any serde type into an [`rmpv::Value`].
pub fn to_value<T: Serialize + ?Sized>(value: &T) -> Value {
    rmp_serde::to_vec_named(value)
        .ok()
        .and_then(|bytes| rmpv::decode::read_value(&mut bytes.as_slice()).ok())
        .unwrap_or(Value::Nil)
}

/// Alias for [`to_value`].
pub fn value_of<T: Serialize + ?Sized>(value: &T) -> Value {
    to_value(value)
}

/// Deserializes a serde type from an [`rmpv::Value`].
pub fn decode_value<T: DeserializeOwned>(value: &Value) -> Option<T> {
    let mut bytes = Vec::new();
    rmpv::encode::write_value(&mut bytes, value).ok()?;
    rmp_serde::from_slice(&bytes).ok()
}

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
