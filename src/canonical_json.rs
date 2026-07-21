use crate::error::{PulseError, Result};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

pub const SHA256_PREFIX: &str = "sha256:";

pub fn to_canonical_value(value: &Value) -> Result<Value> {
    canonicalize(value, "$")
}

pub fn to_canonical_bytes(value: &Value) -> Result<Vec<u8>> {
    let canonical = to_canonical_value(value)?;
    let mut bytes = serde_json::to_vec_pretty(&canonical)?;
    while bytes.last() == Some(&b'\n') || bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn to_canonical_string(value: &Value) -> Result<String> {
    let bytes = to_canonical_bytes(value)?;
    String::from_utf8(bytes).map_err(|error| PulseError::Validation {
        message: format!("canonical JSON was not UTF-8: {error}"),
    })
}

pub fn to_value<T: Serialize>(value: &T) -> Result<Value> {
    serde_json::to_value(value).map_err(PulseError::from)
}

pub fn to_canonical_bytes_from<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let json = to_value(value)?;
    to_canonical_bytes(&json)
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{SHA256_PREFIX}{}", hex::encode(digest))
}

pub fn hash_value(value: &Value) -> Result<String> {
    Ok(hash_bytes(&to_canonical_bytes(value)?))
}

pub fn hash_serializable<T: Serialize>(value: &T) -> Result<String> {
    Ok(hash_bytes(&to_canonical_bytes_from(value)?))
}

fn canonicalize(value: &Value, path: &str) -> Result<Value> {
    match value {
        Value::Object(object) => {
            let mut sorted = Map::new();
            let mut entries: Vec<_> = object.iter().collect();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            for (key, child) in entries {
                sorted.insert(key.clone(), canonicalize(child, &format!("{path}.{key}"))?);
            }
            Ok(Value::Object(sorted))
        }
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, child)| canonicalize(child, &format!("{path}[{index}]")))
            .collect::<Result<Vec<_>>>()
            .map(Value::Array),
        Value::Number(number) => {
            if number.is_f64() {
                return Err(PulseError::FloatRejected {
                    path: path.to_string(),
                });
            }
            Ok(Value::Number(number.clone()))
        }
        Value::Null | Value::Bool(_) | Value::String(_) => Ok(value.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_json_sorts_keys_and_preserves_arrays() {
        let left = json!({"b": 1, "a": {"d": 4, "c": [ {"z": 1, "y": 2} ]}});
        let right = json!({"a": {"c": [ {"y": 2, "z": 1} ], "d": 4}, "b": 1});

        let left_bytes = to_canonical_bytes(&left).unwrap();
        let right_bytes = to_canonical_bytes(&right).unwrap();

        assert_eq!(left_bytes, right_bytes);
        assert_eq!(left_bytes.last(), Some(&b'\n'));
        assert_eq!(String::from_utf8(left_bytes).unwrap().matches('\n').count(), 8);
    }

    #[test]
    fn canonical_json_rejects_floats() {
        let value = json!({"n": 1.25});
        assert!(matches!(
            to_canonical_bytes(&value),
            Err(PulseError::FloatRejected { .. })
        ));
    }
}
