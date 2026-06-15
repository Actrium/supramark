/// A parsed JSON value (recursive tree structure).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

/// Top-level JSON diagram: the entire parsed JSON document.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct JsonDiagram {
    pub root: JsonValue,
}

impl JsonValue {
    /// Returns true if this value is a container (object or array).
    pub fn is_container(&self) -> bool {
        matches!(self, JsonValue::Object(_) | JsonValue::Array(_))
    }

    /// Returns a human-readable type label for display.
    pub fn type_label(&self) -> &str {
        match self {
            JsonValue::Null => "null",
            JsonValue::Bool(_) => "bool",
            JsonValue::Number(_) => "number",
            JsonValue::Str(_) => "string",
            JsonValue::Array(_) => "array",
            JsonValue::Object(_) => "object",
        }
    }

    /// Format this value as a display string (for leaf nodes).
    pub fn display_value(&self) -> String {
        match self {
            JsonValue::Null => "null".to_string(),
            JsonValue::Bool(b) => b.to_string(),
            JsonValue::Number(n) => {
                if *n == (*n as i64) as f64 && n.is_finite() {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            JsonValue::Str(s) => format!("\"{s}\""),
            JsonValue::Array(_) => "[ ... ]".to_string(),
            JsonValue::Object(_) => "{ ... }".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_container() {
        assert!(!JsonValue::Null.is_container());
        assert!(!JsonValue::Bool(true).is_container());
        assert!(!JsonValue::Number(42.0).is_container());
        assert!(!JsonValue::Str("hello".into()).is_container());
        assert!(JsonValue::Array(vec![]).is_container());
        assert!(JsonValue::Object(vec![]).is_container());
    }

    #[test]
    fn test_type_label() {
        assert_eq!(JsonValue::Null.type_label(), "null");
        assert_eq!(JsonValue::Bool(false).type_label(), "bool");
        assert_eq!(JsonValue::Number(1.0).type_label(), "number");
        assert_eq!(JsonValue::Str("x".into()).type_label(), "string");
        assert_eq!(JsonValue::Array(vec![]).type_label(), "array");
        assert_eq!(JsonValue::Object(vec![]).type_label(), "object");
    }

    #[test]
    fn test_display_value_null() {
        assert_eq!(JsonValue::Null.display_value(), "null");
    }

    #[test]
    fn test_display_value_bool() {
        assert_eq!(JsonValue::Bool(true).display_value(), "true");
        assert_eq!(JsonValue::Bool(false).display_value(), "false");
    }

    #[test]
    fn test_display_value_number_integer() {
        assert_eq!(JsonValue::Number(42.0).display_value(), "42");
        assert_eq!(JsonValue::Number(-7.0).display_value(), "-7");
    }

    #[test]
    fn test_display_value_number_float() {
        assert_eq!(JsonValue::Number(3.15).display_value(), "3.15");
    }

    #[test]
    fn test_display_value_string() {
        assert_eq!(JsonValue::Str("hello".into()).display_value(), "\"hello\"");
    }

    #[test]
    fn test_display_value_array_summary() {
        let arr = JsonValue::Array(vec![JsonValue::Number(1.0)]);
        assert_eq!(arr.display_value(), "[ ... ]");
    }

    #[test]
    fn test_display_value_object_summary() {
        let obj = JsonValue::Object(vec![("k".into(), JsonValue::Null)]);
        assert_eq!(obj.display_value(), "{ ... }");
    }

    #[test]
    fn test_json_diagram_creation() {
        let jd = JsonDiagram {
            root: JsonValue::Object(vec![("key".into(), JsonValue::Str("value".into()))]),
        };
        assert!(jd.root.is_container());
    }

    #[test]
    fn test_clone_and_eq() {
        let v = JsonValue::Array(vec![JsonValue::Number(1.0), JsonValue::Str("two".into())]);
        let v2 = v.clone();
        assert_eq!(v, v2);
    }
}
