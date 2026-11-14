//! Converts protobuf Value/Struct to JSON.
//!
//! Protobuf numbers are f64. JSON schema constraints such as `minItems` must
//! be integers, so exact integer-valued floats are emitted as JSON integers
//! when they fit in `u64` / `i64` without saturating.

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
        if value >= 0.0 {
            // `u64::MAX as f64` rounds to 2^64. Casting at that bound saturates
            // to `u64::MAX`, which then rounds back to 2^64 and would pass a
            // round-trip check while emitting 2^64 − 1.
            if value < u64::MAX as f64 {
                let int_value = value as u64;
                if int_value as f64 == value {
                    return Value::from(int_value);
                }
            }
        } else if value >= i64::MIN as f64 {
            let int_value = value as i64;
            if int_value as f64 == value {
                return Value::from(int_value);
            }
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

    #[test]
    fn exact_integers_within_u64_stay_json_integers() {
        for (value, expected) in [
            (0.0, "0"),
            (-0.0, "0"),
            ((1u64 << 53) as f64, "9007199254740992"),
            ((1u64 << 62) as f64, "4611686018427387904"),
            ((1u64 << 63) as f64, "9223372036854775808"),
            (((1u64 << 63) + 2048) as f64, "9223372036854777856"),
        ] {
            let json = json_number(value);
            assert_eq!(serde_json::to_string(&json).unwrap(), expected, "{value}");
            assert!(json.as_u64().is_some(), "{value}");
        }
    }

    #[test]
    fn i64_min_stays_a_json_integer() {
        let value = json_number(i64::MIN as f64);
        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            "-9223372036854775808"
        );
        assert_eq!(value.as_i64(), Some(i64::MIN));
    }

    #[test]
    fn values_outside_integer_range_stay_json_floats() {
        for value in [2.0_f64.powi(64), 1e300] {
            let json = json_number(value);
            assert!(json.as_u64().is_none(), "{value}");
            assert!(json.as_i64().is_none(), "{value}");
            assert_eq!(json.as_f64(), Some(value), "{value}");
        }
    }

    #[test]
    fn two_pow_64_is_not_silently_truncated_to_u64_max() {
        let json = json_number(2.0_f64.powi(64));
        assert_ne!(json.as_u64(), Some(u64::MAX));
        assert_eq!(json.as_f64(), Some(2.0_f64.powi(64)));
    }

    #[test]
    fn two_pow_63_is_not_silently_truncated_to_i64_max() {
        let json = json_number((1u64 << 63) as f64);
        assert_ne!(json.as_i64(), Some(i64::MAX));
        assert_eq!(json.as_u64(), Some(1u64 << 63));
    }
}
