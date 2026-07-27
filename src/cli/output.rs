use serde::Serialize;
use serde_json::json;

use crate::PulseError;

pub(super) fn render<T: Serialize>(
    json_output: bool,
    value: &T,
    human: String,
) -> Result<(), PulseError> {
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(value)
                .map_err(|e| PulseError::validation("json_serialize_error", e.to_string()))?
        );
    } else {
        println!("{human}");
    }
    Ok(())
}

pub fn print_error(err: &PulseError) {
    let value = match err {
        PulseError::CasConflict {
            subject,
            expected_revision,
            current_revision,
        } => json!({
            "schema_version": 1,
            "code": err.code(),
            "subject": subject,
            "expected_revision": expected_revision,
            "current_revision": current_revision,
            "message": err.to_string(),
        }),
        _ => json!({
            "schema_version": 1,
            "code": err.code(),
            "message": err.to_string(),
        }),
    };
    eprintln!(
        "{}",
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| err.to_string())
    );
}
