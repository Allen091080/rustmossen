// Translated from utils/mcp/dateTimeParser.ts and utils/mcp/elicitationValidation.ts

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// dateTimeParser.ts
// ============================================================================

/// Result of parsing a natural language date/time input.
#[derive(Debug, Clone)]
pub enum DateTimeParseResult {
    Success { value: String },
    Failure { error: String },
}

/// Parse natural language date/time input into ISO 8601 format.
///
/// Examples:
/// - "tomorrow at 3pm" → "2025-10-15T15:00:00-07:00"
/// - "next Monday" → "2025-10-20"
/// - "in 2 hours" → "2025-10-14T12:30:00-07:00"
pub async fn parse_natural_language_date_time(
    input: &str,
    format: DateTimeFormat,
    _abort: Option<()>,
) -> DateTimeParseResult {
    // Get current datetime with timezone for context
    let now = chrono::Local::now();
    let current_date_time = now.to_rfc3339();
    let timezone = now.format("%:z").to_string();
    let day_of_week = now.format("%A").to_string();

    let format_description = match format {
        DateTimeFormat::Date => "YYYY-MM-DD (date only, no time)".to_string(),
        DateTimeFormat::DateTime => {
            format!(
                "YYYY-MM-DDTHH:MM:SS{} (full date-time with timezone)",
                timezone
            )
        }
    };

    let _user_prompt = format!(
        "Current context:\n\
        - Current date and time: {} (UTC)\n\
        - Local timezone: {}\n\
        - Day of week: {}\n\n\
        User input: \"{}\"\n\n\
        Output format: {}\n\n\
        Parse the user's input into ISO 8601 format. Return ONLY the formatted string, or \"INVALID\" if the input is incomplete or unparseable.",
        current_date_time, timezone, day_of_week, input, format_description
    );

    // In a real implementation, this would call the Fast LLM
    // For now, attempt basic ISO parsing
    let trimmed = input.trim();
    if looks_like_iso8601(trimmed) {
        return DateTimeParseResult::Success {
            value: trimmed.to_string(),
        };
    }

    DateTimeParseResult::Failure {
        error: "Unable to parse date/time. Please enter in ISO 8601 format manually.".to_string(),
    }
}

/// Date/time format for parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateTimeFormat {
    Date,
    DateTime,
}

/// Check if a string looks like it might be an ISO 8601 date/time.
/// Used to decide whether to attempt NL parsing.
pub fn looks_like_iso8601(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.len() < 10 {
        return false;
    }
    let bytes = trimmed.as_bytes();
    // Check YYYY-MM-DD pattern
    bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[4] == b'-'
        && bytes[5].is_ascii_digit()
        && bytes[6].is_ascii_digit()
        && bytes[7] == b'-'
        && bytes[8].is_ascii_digit()
        && bytes[9].is_ascii_digit()
        && (trimmed.len() == 10 || bytes[10] == b'T')
}

// ============================================================================
// elicitationValidation.ts
// ============================================================================

/// Result of validating user input against a schema.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub value: Option<ValidationValue>,
    pub is_valid: bool,
    pub error: Option<String>,
}

/// A validated value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ValidationValue {
    String(String),
    Number(f64),
    Boolean(bool),
}

/// Primitive schema types for elicitation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PrimitiveSchema {
    #[serde(rename = "string")]
    String {
        #[serde(default)]
        format: Option<String>,
        #[serde(rename = "minLength")]
        min_length: Option<usize>,
        #[serde(rename = "maxLength")]
        max_length: Option<usize>,
        #[serde(rename = "enum")]
        enum_values: Option<Vec<String>>,
        #[serde(rename = "oneOf")]
        one_of: Option<Vec<EnumItem>>,
    },
    #[serde(rename = "number")]
    Number {
        minimum: Option<f64>,
        maximum: Option<f64>,
    },
    #[serde(rename = "integer")]
    Integer {
        minimum: Option<i64>,
        maximum: Option<i64>,
    },
    #[serde(rename = "boolean")]
    Boolean {},
    #[serde(rename = "array")]
    Array { items: Option<Box<ArrayItems>> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumItem {
    #[serde(rename = "const")]
    pub const_value: String,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrayItems {
    #[serde(rename = "enum")]
    pub enum_values: Option<Vec<String>>,
    #[serde(rename = "anyOf")]
    pub any_of: Option<Vec<EnumItem>>,
}

/// String format info for hints.
struct StringFormatInfo {
    description: &'static str,
    example: &'static str,
}

fn get_string_formats() -> HashMap<&'static str, StringFormatInfo> {
    let mut m = HashMap::new();
    m.insert(
        "email",
        StringFormatInfo {
            description: "email address",
            example: "user@example.com",
        },
    );
    m.insert(
        "uri",
        StringFormatInfo {
            description: "URI",
            example: "https://example.com",
        },
    );
    m.insert(
        "date",
        StringFormatInfo {
            description: "date",
            example: "2024-03-15",
        },
    );
    m.insert(
        "date-time",
        StringFormatInfo {
            description: "date-time",
            example: "2024-03-15T14:30:00Z",
        },
    );
    m
}

/// Check if schema is a single-select enum.
pub fn is_enum_schema(schema: &PrimitiveSchema) -> bool {
    matches!(
        schema,
        PrimitiveSchema::String {
            enum_values: Some(_),
            ..
        } | PrimitiveSchema::String {
            one_of: Some(_),
            ..
        }
    )
}

/// Check if schema is a multi-select enum (type: "array" with items.enum or items.anyOf).
pub fn is_multi_select_enum_schema(schema: &PrimitiveSchema) -> bool {
    matches!(schema, PrimitiveSchema::Array { items: Some(items) } if items.enum_values.is_some() || items.any_of.is_some())
}

/// Get values from a multi-select enum schema.
pub fn get_multi_select_values(schema: &PrimitiveSchema) -> Vec<String> {
    if let PrimitiveSchema::Array { items: Some(items) } = schema {
        if let Some(any_of) = &items.any_of {
            return any_of.iter().map(|i| i.const_value.clone()).collect();
        }
        if let Some(enum_values) = &items.enum_values {
            return enum_values.clone();
        }
    }
    Vec::new()
}

/// Get display labels from a multi-select enum schema.
pub fn get_multi_select_labels(schema: &PrimitiveSchema) -> Vec<String> {
    if let PrimitiveSchema::Array { items: Some(items) } = schema {
        if let Some(any_of) = &items.any_of {
            return any_of
                .iter()
                .map(|i| i.title.clone().unwrap_or_else(|| i.const_value.clone()))
                .collect();
        }
        if let Some(enum_values) = &items.enum_values {
            return enum_values.clone();
        }
    }
    Vec::new()
}

/// Get label for a specific value in a multi-select enum.
pub fn get_multi_select_label(schema: &PrimitiveSchema, value: &str) -> String {
    let values = get_multi_select_values(schema);
    let labels = get_multi_select_labels(schema);
    if let Some(index) = values.iter().position(|v| v == value) {
        labels
            .get(index)
            .cloned()
            .unwrap_or_else(|| value.to_string())
    } else {
        value.to_string()
    }
}

/// Get enum values from EnumSchema.
pub fn get_enum_values(schema: &PrimitiveSchema) -> Vec<String> {
    if let PrimitiveSchema::String {
        one_of: Some(one_of),
        ..
    } = schema
    {
        return one_of.iter().map(|i| i.const_value.clone()).collect();
    }
    if let PrimitiveSchema::String {
        enum_values: Some(values),
        ..
    } = schema
    {
        return values.clone();
    }
    Vec::new()
}

/// Get enum display labels.
pub fn get_enum_labels(schema: &PrimitiveSchema) -> Vec<String> {
    if let PrimitiveSchema::String {
        one_of: Some(one_of),
        ..
    } = schema
    {
        return one_of
            .iter()
            .map(|i| i.title.clone().unwrap_or_else(|| i.const_value.clone()))
            .collect();
    }
    if let PrimitiveSchema::String {
        enum_values: Some(values),
        ..
    } = schema
    {
        return values.clone();
    }
    Vec::new()
}

/// Get label for a specific enum value.
pub fn get_enum_label(schema: &PrimitiveSchema, value: &str) -> String {
    let values = get_enum_values(schema);
    let labels = get_enum_labels(schema);
    if let Some(index) = values.iter().position(|v| v == value) {
        labels
            .get(index)
            .cloned()
            .unwrap_or_else(|| value.to_string())
    } else {
        value.to_string()
    }
}

/// Validate user input against a schema.
pub fn validate_elicitation_input(
    string_value: &str,
    schema: &PrimitiveSchema,
) -> ValidationResult {
    match schema {
        PrimitiveSchema::String {
            format,
            min_length,
            max_length,
            enum_values,
            one_of,
        } => {
            // Check enum constraint
            if let Some(values) = enum_values {
                if values.contains(&string_value.to_string()) {
                    return ValidationResult {
                        value: Some(ValidationValue::String(string_value.to_string())),
                        is_valid: true,
                        error: None,
                    };
                } else {
                    return ValidationResult {
                        value: None,
                        is_valid: false,
                        error: Some(format!("Must be one of: {}", values.join(", "))),
                    };
                }
            }
            if let Some(one_of_items) = one_of {
                let valid_values: Vec<&str> = one_of_items
                    .iter()
                    .map(|i| i.const_value.as_str())
                    .collect();
                if valid_values.contains(&string_value) {
                    return ValidationResult {
                        value: Some(ValidationValue::String(string_value.to_string())),
                        is_valid: true,
                        error: None,
                    };
                } else {
                    return ValidationResult {
                        value: None,
                        is_valid: false,
                        error: Some(format!("Must be one of: {}", valid_values.join(", "))),
                    };
                }
            }
            // Check min/max length
            if let Some(min) = min_length {
                if string_value.len() < *min {
                    return ValidationResult {
                        value: None,
                        is_valid: false,
                        error: Some(format!("Must be at least {} character(s)", min)),
                    };
                }
            }
            if let Some(max) = max_length {
                if string_value.len() > *max {
                    return ValidationResult {
                        value: None,
                        is_valid: false,
                        error: Some(format!("Must be at most {} character(s)", max)),
                    };
                }
            }
            // Check format
            if let Some(fmt) = format {
                match fmt.as_str() {
                    "email" if (!string_value.contains('@') || !string_value.contains('.')) => {
                        return ValidationResult {
                            value: None,
                            is_valid: false,
                            error: Some(
                                "Must be a valid email address, e.g. user@example.com".to_string(),
                            ),
                        };
                    }
                    "uri"
                        if !string_value.starts_with("http://")
                            && !string_value.starts_with("https://") =>
                    {
                        return ValidationResult {
                            value: None,
                            is_valid: false,
                            error: Some(
                                "Must be a valid URI, e.g. https://example.com".to_string(),
                            ),
                        };
                    }
                    "date" if (!looks_like_iso8601(string_value) || string_value.len() != 10) => {
                        return ValidationResult {
                            value: None,
                            is_valid: false,
                            error: Some("Must be a valid date, e.g. 2024-03-15".to_string()),
                        };
                    }
                    "date-time"
                        if (!looks_like_iso8601(string_value) || !string_value.contains('T')) =>
                    {
                        return ValidationResult {
                            value: None,
                            is_valid: false,
                            error: Some(
                                "Must be a valid date-time, e.g. 2024-03-15T14:30:00Z".to_string(),
                            ),
                        };
                    }
                    _ => {}
                }
            }
            ValidationResult {
                value: Some(ValidationValue::String(string_value.to_string())),
                is_valid: true,
                error: None,
            }
        }
        PrimitiveSchema::Number { minimum, maximum } => match string_value.parse::<f64>() {
            Ok(num) => {
                if let Some(min) = minimum {
                    if num < *min {
                        return ValidationResult {
                            value: None,
                            is_valid: false,
                            error: Some(format!("Must be a number >= {}", min)),
                        };
                    }
                }
                if let Some(max) = maximum {
                    if num > *max {
                        return ValidationResult {
                            value: None,
                            is_valid: false,
                            error: Some(format!("Must be a number <= {}", max)),
                        };
                    }
                }
                ValidationResult {
                    value: Some(ValidationValue::Number(num)),
                    is_valid: true,
                    error: None,
                }
            }
            Err(_) => ValidationResult {
                value: None,
                is_valid: false,
                error: Some("Must be a number".to_string()),
            },
        },
        PrimitiveSchema::Integer { minimum, maximum } => match string_value.parse::<i64>() {
            Ok(num) => {
                if let Some(min) = minimum {
                    if num < *min {
                        return ValidationResult {
                            value: None,
                            is_valid: false,
                            error: Some(format!("Must be an integer >= {}", min)),
                        };
                    }
                }
                if let Some(max) = maximum {
                    if num > *max {
                        return ValidationResult {
                            value: None,
                            is_valid: false,
                            error: Some(format!("Must be an integer <= {}", max)),
                        };
                    }
                }
                ValidationResult {
                    value: Some(ValidationValue::Number(num as f64)),
                    is_valid: true,
                    error: None,
                }
            }
            Err(_) => ValidationResult {
                value: None,
                is_valid: false,
                error: Some("Must be an integer".to_string()),
            },
        },
        PrimitiveSchema::Boolean {} => {
            let val = match string_value.to_lowercase().as_str() {
                "true" | "1" | "yes" => true,
                "false" | "0" | "no" => false,
                _ => {
                    return ValidationResult {
                        value: None,
                        is_valid: false,
                        error: Some("Must be a boolean (true/false)".to_string()),
                    };
                }
            };
            ValidationResult {
                value: Some(ValidationValue::Boolean(val)),
                is_valid: true,
                error: None,
            }
        }
        PrimitiveSchema::Array { .. } => {
            // Array validation is handled differently (multi-select)
            ValidationResult {
                value: Some(ValidationValue::String(string_value.to_string())),
                is_valid: true,
                error: None,
            }
        }
    }
}

/// Returns a helpful placeholder/hint for a given format.
pub fn get_format_hint(schema: &PrimitiveSchema) -> Option<String> {
    match schema {
        PrimitiveSchema::String {
            format: Some(fmt), ..
        } => {
            let formats = get_string_formats();
            formats
                .get(fmt.as_str())
                .map(|info| format!("{}, e.g. {}", info.description, info.example))
        }
        PrimitiveSchema::Number { minimum, maximum } => match (minimum, maximum) {
            (Some(min), Some(max)) => Some(format!("(number between {} and {})", min, max)),
            (Some(min), None) => Some(format!("(number >= {})", min)),
            (None, Some(max)) => Some(format!("(number <= {})", max)),
            (None, None) => Some("(number, e.g. 3.14)".to_string()),
        },
        PrimitiveSchema::Integer { minimum, maximum } => match (minimum, maximum) {
            (Some(min), Some(max)) => Some(format!("(integer between {} and {})", min, max)),
            (Some(min), None) => Some(format!("(integer >= {})", min)),
            (None, Some(max)) => Some(format!("(integer <= {})", max)),
            (None, None) => Some("(integer, e.g. 42)".to_string()),
        },
        _ => None,
    }
}

/// Check if a schema is a date or date-time format that supports NL parsing.
pub fn is_date_time_schema(schema: &PrimitiveSchema) -> bool {
    matches!(
        schema,
        PrimitiveSchema::String { format: Some(f), .. } if f == "date" || f == "date-time"
    )
}

/// Async validation that attempts NL date/time parsing when the input doesn't look like ISO 8601.
pub async fn validate_elicitation_input_async(
    string_value: &str,
    schema: &PrimitiveSchema,
    _abort: Option<()>,
) -> ValidationResult {
    let sync_result = validate_elicitation_input(string_value, schema);
    if sync_result.is_valid {
        return sync_result;
    }

    if is_date_time_schema(schema) && !looks_like_iso8601(string_value) {
        let format = match schema {
            PrimitiveSchema::String {
                format: Some(f), ..
            } if f == "date" => DateTimeFormat::Date,
            _ => DateTimeFormat::DateTime,
        };
        let parse_result = parse_natural_language_date_time(string_value, format, None).await;

        if let DateTimeParseResult::Success { value } = parse_result {
            let validated = validate_elicitation_input(&value, schema);
            if validated.is_valid {
                return validated;
            }
        }
    }

    sync_result
}
