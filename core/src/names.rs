//! Rules for maker, sender, log and scenario names given at creation.
//!
//! Names appear in URL paths (`/api/v1/maker/{name}`), so they are limited to
//! characters that need no escaping and pass through proxies unchanged. Maker
//! names must also be usable as `<name>` tokens in log formats. Names already
//! in storage are loaded as they are, whatever they contain.

use crate::api_result::ApiResult;
use crate::template;

pub const MAX_NAME_LEN: usize = 64;

const REQUIRED: &str = "Name field value is required";
const CHARACTERS: &str = "Name may only contain letters, digits, '_' and '-' (at most 64 characters)";
const MAKER_START: &str = "Maker name must start with a letter or '_' to be usable as <name> in log formats";

/// Validation error for a sender, log or scenario name.
pub fn check_name(name: &str) -> Option<ApiResult> {
    let message = if name.is_empty() {
        REQUIRED
    } else if name.len() > MAX_NAME_LEN
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        CHARACTERS
    } else {
        return None;
    };
    Some(ApiResult::validation("name", message))
}

/// Validation error for a maker name.
pub fn check_maker_name(name: &str) -> Option<ApiResult> {
    check_name(name).or_else(|| (!template::is_token_name(name)).then(|| ApiResult::validation("name", MAKER_START)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(result: Option<ApiResult>) -> Option<String> {
        result.map(|r| r.data.unwrap()["name"].as_str().unwrap().to_owned())
    }

    #[test]
    fn accepts_url_safe_names() {
        for name in ["web", "Src_IP-2", "404-code", "_x", &"a".repeat(MAX_NAME_LEN)] {
            assert_eq!(message(check_name(name)), None, "{name}");
        }
        for name in ["web", "Src_IP-2", "_x"] {
            assert_eq!(message(check_maker_name(name)), None, "{name}");
        }
    }

    #[test]
    fn rejects_names_that_break_urls_or_templates() {
        assert_eq!(message(check_name("")).as_deref(), Some(REQUIRED));
        for name in [
            "a b",
            " a",
            "a#b",
            "a?b",
            "a/b",
            "a\\b",
            "a%20b",
            ".",
            "..",
            "a.b",
            "한글",
            "tab\t",
            &"a".repeat(65),
        ] {
            assert_eq!(message(check_name(name)).as_deref(), Some(CHARACTERS), "{name:?}");
        }
        for name in ["404-code", "-x"] {
            assert_eq!(message(check_maker_name(name)).as_deref(), Some(MAKER_START), "{name}");
        }
    }
}
