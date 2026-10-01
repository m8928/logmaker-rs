use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::PluginError;

/// User-supplied arguments of a maker or sender (a JSON object).
pub type Args = serde_json::Map<String, Value>;

/// JSON type accepted for an argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArgType {
    String,
    /// Whole number within the 32-bit signed range.
    Integer,
    /// Any JSON number.
    Number,
    Boolean,
    /// JSON array.
    List,
}

impl ArgType {
    fn accepts(self, value: &Value) -> bool {
        match self {
            Self::String => value.is_string(),
            Self::Integer => value.as_i64().is_some_and(|n| i32::try_from(n).is_ok()),
            Self::Number => value.is_number(),
            Self::Boolean => value.is_boolean(),
            Self::List => value.is_array(),
        }
    }
}

/// Declaration of one argument.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArgSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub arg_type: ArgType,
    pub description: String,
    pub required: bool,
}

impl ArgSpec {
    pub fn required(name: &str, arg_type: ArgType, description: &str) -> Self {
        Self::new(name, arg_type, description, true)
    }

    pub fn optional(name: &str, arg_type: ArgType, description: &str) -> Self {
        Self::new(name, arg_type, description, false)
    }

    fn new(name: &str, arg_type: ArgType, description: &str, required: bool) -> Self {
        Self {
            name: name.to_owned(),
            arg_type,
            description: description.to_owned(),
            required,
        }
    }
}

/// Validates `args` against `specs`:
///
/// * every required argument is present;
/// * every declared argument that is present is non-null and of the declared type;
/// * required list arguments are non-empty.
///
/// Undeclared arguments are allowed and kept as-is.
pub fn check_args(specs: &[ArgSpec], args: &Args) -> Result<(), PluginError> {
    for spec in specs {
        let Some(value) = args.get(&spec.name) else {
            if spec.required {
                return Err(PluginError::invalid(&spec.name));
            }
            continue;
        };
        if !spec.arg_type.accepts(value) {
            return Err(PluginError::invalid(&spec.name));
        }
        if spec.required && value.as_array().is_some_and(Vec::is_empty) {
            return Err(PluginError::invalid(&spec.name));
        }
    }
    Ok(())
}

/// String argument, or `None` when absent or not a string.
pub fn arg_str<'a>(args: &'a Args, name: &str) -> Option<&'a str> {
    args.get(name).and_then(Value::as_str)
}

/// Whole-number argument. Accepts JSON integers, integral floats (`10.0`) and
/// numeric strings; returns `Some(Err)` for any other present value.
pub fn arg_i64(args: &Args, name: &str) -> Option<Result<i64, PluginError>> {
    let value = args.get(name)?;
    let parsed = match value {
        Value::Number(n) => n.as_i64().or_else(|| {
            n.as_f64()
                .filter(|f| f.fract() == 0.0 && *f >= i64::MIN as f64 && *f <= i64::MAX as f64)
                .map(|f| f as i64)
        }),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    };
    Some(parsed.ok_or_else(|| PluginError::invalid(name)))
}

/// Boolean argument, or `None` when absent or not a boolean.
pub fn arg_bool(args: &Args, name: &str) -> Option<bool> {
    args.get(name).and_then(Value::as_bool)
}

/// List argument as strings (non-string items are converted with their JSON
/// text), or `None` when absent or not a list.
pub fn arg_string_list(args: &Args, name: &str) -> Option<Vec<String>> {
    let items = args.get(name)?.as_array()?;
    Some(
        items
            .iter()
            .map(|item| match item {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn args(value: Value) -> Args {
        value.as_object().cloned().unwrap()
    }

    fn specs() -> Vec<ArgSpec> {
        vec![
            ArgSpec::required("ip", ArgType::String, ""),
            ArgSpec::required("port", ArgType::Integer, ""),
            ArgSpec::required("host", ArgType::List, ""),
            ArgSpec::optional("random", ArgType::Boolean, ""),
            ArgSpec::optional("ratio", ArgType::Number, ""),
        ]
    }

    #[test]
    fn accepts_valid_arguments_and_extra_keys() {
        let a = args(json!({"ip": "127.0.0.1", "port": 514, "host": ["a"], "ratio": 0.5, "extra": 1}));
        assert_eq!(check_args(&specs(), &a), Ok(()));
    }

    #[test]
    fn rejects_missing_required_argument() {
        let a = args(json!({"ip": "127.0.0.1", "host": ["a"]}));
        assert_eq!(check_args(&specs(), &a), Err(PluginError::invalid("port")));
    }

    #[test]
    fn rejects_wrong_types_and_nulls() {
        for (key, bad) in [
            ("ip", json!(1)),
            ("port", json!("514")),
            ("port", json!(1.5)),
            ("port", json!(4_294_967_296_i64)),
            ("random", json!("true")),
            ("ratio", Value::Null),
        ] {
            let mut a = args(json!({"ip": "x", "port": 1, "host": ["a"]}));
            a.insert(key.to_owned(), bad);
            assert_eq!(check_args(&specs(), &a), Err(PluginError::invalid(key)), "{key}");
        }
    }

    #[test]
    fn rejects_empty_required_list() {
        let a = args(json!({"ip": "x", "port": 1, "host": []}));
        assert_eq!(check_args(&specs(), &a), Err(PluginError::invalid("host")));
    }

    #[test]
    fn reads_whole_numbers_leniently() {
        let a = args(json!({"a": 10, "b": 10.0, "c": "42", "d": 1.5, "e": true}));
        assert_eq!(arg_i64(&a, "a"), Some(Ok(10)));
        assert_eq!(arg_i64(&a, "b"), Some(Ok(10)));
        assert_eq!(arg_i64(&a, "c"), Some(Ok(42)));
        assert_eq!(arg_i64(&a, "d"), Some(Err(PluginError::invalid("d"))));
        assert_eq!(arg_i64(&a, "e"), Some(Err(PluginError::invalid("e"))));
        assert_eq!(arg_i64(&a, "missing"), None);
    }

    #[test]
    fn converts_list_items_to_strings() {
        let a = args(json!({"l": ["x", 1, true]}));
        assert_eq!(
            arg_string_list(&a, "l"),
            Some(vec!["x".into(), "1".into(), "true".into()])
        );
    }
}
