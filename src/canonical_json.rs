use crate::error::{PulseError, Result};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

pub const SHA256_PREFIX: &str = "sha256:";

pub fn to_value<T: Serialize>(value: &T) -> Result<Value> {
    serde_json::to_value(value).map_err(PulseError::from)
}

pub fn to_canonical_value(value: &Value) -> Result<Value> {
    canonicalize(value, "$")
}

pub fn to_canonical_value_from<T: Serialize>(value: &T) -> Result<Value> {
    to_canonical_value(&to_value(value)?)
}

pub fn sort_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut sorted = Map::new();
            let mut entries: Vec<_> = map.into_iter().collect();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            for (key, child) in entries {
                sorted.insert(key, sort_value(child));
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_value).collect()),
        other => other,
    }
}

pub fn to_canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let json = to_value(value)?;
    canonical_value_bytes(&to_canonical_value(&json)?)
}

pub fn to_canonical_bytes_from<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    to_canonical_bytes(value)
}

pub fn canonical_value_bytes(value: &Value) -> Result<Vec<u8>> {
    reject_float(value, "$")?;
    let mut bytes = serde_json::to_vec_pretty(value)?;
    while bytes.last() == Some(&b'\n') || bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn to_canonical_string(value: &Value) -> Result<String> {
    let bytes = to_canonical_bytes(value)?;
    String::from_utf8(bytes).map_err(|error| {
        PulseError::validation(
            "utf8_error",
            format!("canonical JSON was not UTF-8: {error}"),
        )
    })
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{SHA256_PREFIX}{}", hex::encode(digest))
}

pub fn hash_value<T: Serialize>(value: &T) -> Result<String> {
    Ok(hash_bytes(&to_canonical_bytes(value)?))
}

pub fn hash_serializable<T: Serialize>(value: &T) -> Result<String> {
    hash_value(value)
}

fn canonicalize(value: &Value, path: &str) -> Result<Value> {
    match value {
        Value::Object(object) => {
            let mut sorted = Map::new();
            let mut entries: Vec<_> = object.iter().collect();
            entries.sort_by_key(|(left, _)| *left);
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

fn reject_float(value: &Value, path: &str) -> Result<()> {
    match value {
        Value::Number(n) if n.is_f64() => Err(PulseError::FloatRejected {
            path: path.to_string(),
        }),
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                reject_float(item, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for (key, item) in map {
                reject_float(item, &format!("{path}.{key}"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
