use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::{PulseError, PulseResult};

pub fn to_canonical_value<T: Serialize>(value: &T) -> PulseResult<Value> {
    let value = serde_json::to_value(value)
        .map_err(|e| PulseError::validation("json_serialize_error", e.to_string()))?;
    Ok(sort_value(value))
}

pub fn sort_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut sorted = Map::new();
            for key in keys {
                let value = map.get(&key).expect("key from map").clone();
                sorted.insert(key, sort_value(value));
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_value).collect()),
        other => other,
    }
}

pub fn to_canonical_bytes<T: Serialize>(value: &T) -> PulseResult<Vec<u8>> {
    let value = to_canonical_value(value)?;
    canonical_value_bytes(&value)
}

pub fn canonical_value_bytes(value: &Value) -> PulseResult<Vec<u8>> {
    reject_float(value)?;
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| PulseError::validation("json_serialize_error", e.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("sha256:{}", hex::encode(digest))
}

pub fn hash_value<T: Serialize>(value: &T) -> PulseResult<String> {
    Ok(hash_bytes(&to_canonical_bytes(value)?))
}

fn reject_float(value: &Value) -> PulseResult<()> {
    match value {
        Value::Number(n) if n.is_f64() => Err(PulseError::validation(
            "non_canonical_number",
            "floating point numbers are not allowed in canonical JSON",
        )),
        Value::Array(items) => {
            for item in items {
                reject_float(item)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for item in map.values() {
                reject_float(item)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
