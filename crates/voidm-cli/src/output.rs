use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

pub fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

pub fn print_result<T: Serialize>(value: &T) -> Result<()> {
    print_json(&serde_json::json!({ "result": value }))
}

pub fn print_error(msg: &str) {
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "error": msg }))
        .unwrap_or_else(|_| "{\"error\":\"unknown error\"}".to_string()));
}

pub fn redact_secret_values(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if key.eq_ignore_ascii_case("password")
                    || key.eq_ignore_ascii_case("token")
                    || key.eq_ignore_ascii_case("secret")
                    || key.eq_ignore_ascii_case("api_key")
                    || key.eq_ignore_ascii_case("apikey")
                {
                    *val = Value::String("[REDACTED]".to_string());
                } else {
                    redact_secret_values(val);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_secret_values(item);
            }
        }
        _ => {}
    }
}
