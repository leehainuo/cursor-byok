//! Converts protobuf Value/Struct to JSON.
//!
//! Protobuf numbers are f64. JSON schema constraints such as `minItems` must
//! be integers, so exact integer values are emitted as JSON integers.

use prost_types::{value::Kind, Struct, Value as ProtoValue};
use serde_json::Value;

pub(crate) fn prost_json(value: &ProtoValue) -> Value {
    match value.kind.as_ref() {
        None | Some(Kind::NullValue(_)) => Value::Null,
        Some(Kind::NumberValue(value)) => json_number(*value),
        Some(Kind::StringValue(value)) => Value::String(value.clone()),
        Some(Kind::BoolValue(value)) => Value::Bool(*value),
        Some(Kind::StructValue(value)) => prost_struct(value),
        Some(Kind::ListValue(value)) => Value::Array(value.values.iter().map(prost_json).collect()),
    }
}

pub(crate) fn prost_struct(value: &Struct) -> Value {
    Value::Object(
        value
            .fields
            .iter()
            .map(|(key, value)| (key.clone(), prost_json(value)))
            .collect(),
    )
}

fn json_number(value: f64) -> Value {
    if value.is_finite() && value.fract() == 0.0 {
        let int_value = value as i64;
        if int_value as f64 == value {
            return Value::from(int_value);
        }
    }
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_protobuf_integers_serialize_as_json_integers() {
        let value = prost_json(&ProtoValue {
            kind: Some(Kind::NumberValue(2.0)),
        });
        assert_eq!(value, serde_json::json!(2));
        assert_eq!(serde_json::to_string(&value).unwrap(), "2");
    }

    #[test]
    fn non_integer_protobuf_numbers_remain_json_floats() {
        let value = prost_json(&ProtoValue {
            kind: Some(Kind::NumberValue(2.5)),
        });
        assert_eq!(value, serde_json::json!(2.5));
        assert_eq!(serde_json::to_string(&value).unwrap(), "2.5");
    }

    #[test]
    fn non_finite_protobuf_numbers_become_null() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                prost_json(&ProtoValue {
                    kind: Some(Kind::NumberValue(value)),
                }),
                Value::Null
            );
        }
    }

    #[test]
    fn struct_integer_fields_serialize_as_json_integers() {
        let value = prost_struct(&Struct {
            fields: [(
                "minItems".into(),
                ProtoValue {
                    kind: Some(Kind::NumberValue(2.0)),
                },
            )]
            .into_iter()
            .collect(),
        });
        assert_eq!(value.get("minItems"), Some(&serde_json::json!(2)));
        let serialized = serde_json::to_string(&value).unwrap();
        assert!(serialized.contains("\"minItems\":2"));
        assert!(!serialized.contains("2.0"));
    }
}
