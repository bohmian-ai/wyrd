//! The three Bifrost timestamp types.
//!
//! A user column declares one of three timestamp types, named as Snowflake
//! names them:
//!
//! - `TIMESTAMP_NTZ` is a wall-clock reading with no instant, stored as an
//!   Iceberg `timestamp`.
//! - `TIMESTAMP_LTZ` is one instant, stored as an Iceberg `timestamptz`.
//! - `TIMESTAMP_TZ` is one instant plus the writer's wall-clock reading of it,
//!   stored as `struct<utc: timestamptz, local: timestamp>` so a reader can
//!   group by the writer's local hour without knowing the writer's zone.
//!
//! A model declares the type through its JSON Schema `format`. The Wyrd
//! formats are `timestamp-ntz` and `timestamp-tz`; the standard `date-time`
//! format is an RFC 3339 instant and so declares `TIMESTAMP_LTZ`, and the
//! `partial-date-time` format `schemars` gives a `chrono::NaiveDateTime`
//! declares `TIMESTAMP_NTZ`. System-owned timestamps are always
//! `TIMESTAMP_LTZ`.

use std::borrow::Cow;
use std::collections::BTreeMap;

use arrow_schema::{DataType, TimeUnit as ArrowTimeUnit};
use chrono::{DateTime, FixedOffset, NaiveDateTime, Utc};
use schemars::JsonSchema;
use schemars::r#gen::SchemaGenerator;
use schemars::schema::{InstanceType, Schema, SchemaObject};
use serde::{Deserialize, Serialize};
use wyrd_spec::vala::api::{DataTypeSpec, FieldSpec, TimeUnit, UTC_TIME_ZONE};

/// Name of a `TIMESTAMP_TZ` column's instant field.
pub const TIMESTAMP_TZ_UTC: &str = "utc";
/// Name of a `TIMESTAMP_TZ` column's writer wall-clock field.
pub const TIMESTAMP_TZ_LOCAL: &str = "local";

/// One of the three Bifrost timestamp types a column declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampKind {
    /// `TIMESTAMP_NTZ`: a wall-clock reading with no instant.
    Ntz,
    /// `TIMESTAMP_LTZ`: one instant.
    Ltz,
    /// `TIMESTAMP_TZ`: one instant plus the writer's wall-clock reading.
    Tz,
}

impl TimestampKind {
    /// The type's name: `TIMESTAMP_NTZ`, `TIMESTAMP_LTZ`, or `TIMESTAMP_TZ`.
    ///
    /// SDKs receive this name to pick their own Wyrd timestamp type.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Ntz => "TIMESTAMP_NTZ",
            Self::Ltz => "TIMESTAMP_LTZ",
            Self::Tz => "TIMESTAMP_TZ",
        }
    }

    /// The type a [`TimestampKind::name`] names, if any.
    ///
    /// SDKs hand the name back to render or check a value of that type.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Ntz, Self::Ltz, Self::Tz]
            .into_iter()
            .find(|kind| kind.name() == name)
    }

    /// The JSON Schema `format` a Wyrd timestamp type declares itself with.
    #[must_use]
    pub fn json_format(self) -> &'static str {
        match self {
            Self::Ntz => "timestamp-ntz",
            Self::Ltz => "date-time",
            Self::Tz => "timestamp-tz",
        }
    }

    /// The timestamp type a JSON Schema string `format` declares, if any.
    ///
    /// This is the one table from model formats to column types; every SDK's
    /// model path reaches it through the JSON Schema mapping.
    #[must_use]
    pub fn from_json_format(format: &str) -> Option<Self> {
        match format {
            "timestamp-ntz" | "partial-date-time" => Some(Self::Ntz),
            "date-time" => Some(Self::Ltz),
            "timestamp-tz" => Some(Self::Tz),
            _ => None,
        }
    }

    /// The timestamp type an Arrow column type holds, if any.
    ///
    /// A zoned Arrow timestamp is `TIMESTAMP_LTZ` whatever its label, since
    /// the label never changes the instant. `TIMESTAMP_TZ` is recognized by
    /// its struct shape: exactly a zoned `utc` and a naive `local`
    /// microsecond timestamp.
    #[must_use]
    pub fn of(data_type: &DataType) -> Option<Self> {
        match data_type {
            DataType::Timestamp(_, None) => Some(Self::Ntz),
            DataType::Timestamp(_, Some(_)) => Some(Self::Ltz),
            DataType::Struct(fields) => {
                let [utc, local] = fields.iter().collect::<Vec<_>>()[..] else {
                    return None;
                };
                (utc.name() == TIMESTAMP_TZ_UTC
                    && local.name() == TIMESTAMP_TZ_LOCAL
                    && matches!(
                        utc.data_type(),
                        DataType::Timestamp(ArrowTimeUnit::Microsecond, Some(_))
                    )
                    && matches!(
                        local.data_type(),
                        DataType::Timestamp(ArrowTimeUnit::Microsecond, None)
                    ))
                .then_some(Self::Tz)
            }
            _ => None,
        }
    }

    /// The wire column type this timestamp type is declared as.
    ///
    /// Every type stores microseconds; `TIMESTAMP_LTZ` carries the canonical
    /// UTC label, and `TIMESTAMP_TZ` is a struct of the other two with both
    /// fields required.
    #[must_use]
    pub fn data_type_spec(self) -> DataTypeSpec {
        let timestamp = |tz: Option<&str>| DataTypeSpec::Timestamp {
            unit: TimeUnit::Microsecond,
            tz: tz.map(str::to_owned),
        };
        let required = |name: &str, data_type| FieldSpec {
            name: name.to_owned(),
            data_type,
            nullable: false,
            metadata: BTreeMap::new(),
        };
        match self {
            Self::Ntz => timestamp(None),
            Self::Ltz => timestamp(Some(UTC_TIME_ZONE)),
            Self::Tz => DataTypeSpec::Struct(vec![
                required(TIMESTAMP_TZ_UTC, timestamp(Some(UTC_TIME_ZONE))),
                required(TIMESTAMP_TZ_LOCAL, timestamp(None)),
            ]),
        }
    }

    /// Render one stored value as this type's RFC 3339 text.
    ///
    /// `stored` is the column's microseconds: the wall-clock reading for
    /// `TIMESTAMP_NTZ`, the instant otherwise. A `TIMESTAMP_TZ` value also
    /// needs its `local` writer reading. The text is the Wyrd value's own
    /// JSON text, so every SDK reads the same text Rust writes. Returns `None`
    /// when the value is out of range or a `TIMESTAMP_TZ` has no valid
    /// `local`.
    #[must_use]
    pub fn render(self, stored: i64, local: Option<i64>) -> Option<String> {
        let at = DateTime::from_timestamp_micros(stored)?;
        let rendered = match self {
            Self::Ntz => serde_json::to_value(TimestampNtz(at.naive_utc())),
            Self::Ltz => serde_json::to_value(TimestampLtz(at)),
            Self::Tz => serde_json::to_value(TimestampTz::from_stored(stored, local?)?),
        };
        match rendered.ok()? {
            serde_json::Value::String(text) => Some(text),
            _ => None,
        }
    }

    /// Check text against this type's zone rule and return its canonical text.
    ///
    /// The text is parsed exactly as the write path parses it, then rendered
    /// with [`TimestampKind::render`]. Returns `None` when the write path
    /// would refuse it.
    #[must_use]
    pub fn canonical(self, text: &str) -> Option<String> {
        match self {
            Self::Ntz => self.render(parse_ntz(text)?, None),
            Self::Ltz => self.render(parse_ltz(text)?, None),
            Self::Tz => {
                let (utc, local) = parse_tz(text)?;
                self.render(utc, Some(local))
            }
        }
    }
}

/// Parse a `TIMESTAMP_NTZ` value: RFC 3339 date and time with no offset.
///
/// Returns microseconds of the wall-clock reading, or `None` when the text
/// names an offset (it is an instant, not a wall-clock reading) or does not
/// parse.
#[must_use]
pub fn parse_ntz(text: &str) -> Option<i64> {
    text.parse::<NaiveDateTime>()
        .ok()
        .map(|at| at.and_utc().timestamp_micros())
}

/// Parse a `TIMESTAMP_LTZ` value: an RFC 3339 instant with its offset.
///
/// Returns microseconds since the Unix epoch, or `None` when the text has no
/// offset (a wall-clock reading has no instant and is never guessed as UTC)
/// or does not parse.
#[must_use]
pub fn parse_ltz(text: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|at| at.timestamp_micros())
}

/// Parse a `TIMESTAMP_TZ` value: an RFC 3339 instant with its offset.
///
/// Returns `(utc, local)` microseconds: the instant since the Unix epoch and
/// the writer's wall-clock reading. `None` when the text has no offset or
/// does not parse.
#[must_use]
pub fn parse_tz(text: &str) -> Option<(i64, i64)> {
    DateTime::parse_from_rfc3339(text).ok().map(|at| {
        (
            at.timestamp_micros(),
            at.naive_local().and_utc().timestamp_micros(),
        )
    })
}

/// A `TIMESTAMP_NTZ` value: a wall-clock reading with no instant.
///
/// It writes and reads as RFC 3339 text with no offset. Its JSON Schema is a
/// string with `format: timestamp-ntz`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimestampNtz(pub NaiveDateTime);

/// A `TIMESTAMP_LTZ` value: one instant.
///
/// It writes as RFC 3339 UTC text and reads text in any offset as the same
/// instant. Its JSON Schema is a string with `format: date-time`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimestampLtz(pub DateTime<Utc>);

/// A `TIMESTAMP_TZ` value: one instant in the writer's offset.
///
/// It writes and reads as RFC 3339 text with the writer's offset; a read
/// renders the stored `{utc, local}` struct as that text. Its JSON Schema is
/// a string with `format: timestamp-tz`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimestampTz(pub DateTime<FixedOffset>);

impl TimestampTz {
    /// Rebuild the value from its stored `utc` and `local` microseconds.
    ///
    /// The writer's offset is `local - utc`. Returns `None` when `utc` is out
    /// of range or the offset is not under a day in whole seconds.
    #[must_use]
    pub fn from_stored(utc: i64, local: i64) -> Option<Self> {
        let offset = i32::try_from((local - utc) / 1_000_000).ok()?;
        let offset = FixedOffset::east_opt(offset)?;
        DateTime::from_timestamp_micros(utc).map(|at| Self(at.with_timezone(&offset)))
    }
}

/// Give a Wyrd timestamp type the JSON Schema that declares its column type.
///
/// The schema is an inline string whose `format` is the kind's
/// [`TimestampKind::json_format`], which
/// [`TimestampKind::from_json_format`] maps back to that kind.
macro_rules! timestamp_json_schema {
    ($type:ident, $kind:expr) => {
        impl JsonSchema for $type {
            /// Inline the schema so the format sits on the field itself.
            fn is_referenceable() -> bool {
                false
            }

            /// The schema's name.
            fn schema_name() -> String {
                stringify!($type).to_owned()
            }

            /// The schema's stable identity.
            fn schema_id() -> Cow<'static, str> {
                Cow::Borrowed(concat!(module_path!(), "::", stringify!($type)))
            }

            /// A string with this type's `format`.
            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                SchemaObject {
                    instance_type: Some(InstanceType::String.into()),
                    format: Some($kind.json_format().to_owned()),
                    ..SchemaObject::default()
                }
                .into()
            }
        }
    };
}

timestamp_json_schema!(TimestampNtz, TimestampKind::Ntz);
timestamp_json_schema!(TimestampLtz, TimestampKind::Ltz);
timestamp_json_schema!(TimestampTz, TimestampKind::Tz);

#[cfg(test)]
mod tests {
    //! The timestamp types' format table, Arrow recognition, and parsing.

    use arrow_schema::{DataType, Field, Fields};
    use chrono::{FixedOffset, TimeZone};

    use super::*;

    /// 2026-10-06T17:00:00Z in microseconds.
    const MOMENT: i64 = 1_791_306_000_000_000;
    /// The same moment's wall-clock reading in UTC-05:00.
    const CHICAGO_LOCAL: i64 = MOMENT - 5 * 3_600 * 1_000_000;

    /// Every format names its type, and other formats name none.
    ///
    /// # Panics
    ///
    /// Panics when a format maps to the wrong type.
    #[test]
    fn formats_name_their_timestamp_type() {
        assert_eq!(
            TimestampKind::from_json_format("timestamp-ntz"),
            Some(TimestampKind::Ntz)
        );
        assert_eq!(
            TimestampKind::from_json_format("partial-date-time"),
            Some(TimestampKind::Ntz)
        );
        assert_eq!(
            TimestampKind::from_json_format("date-time"),
            Some(TimestampKind::Ltz)
        );
        assert_eq!(
            TimestampKind::from_json_format("timestamp-tz"),
            Some(TimestampKind::Tz)
        );
        assert_eq!(TimestampKind::from_json_format("date"), None);
    }

    /// Each type's declared Arrow form is recognized as that type, any zone
    /// label is `TIMESTAMP_LTZ`, and a look-alike struct is not
    /// `TIMESTAMP_TZ`.
    ///
    /// # Panics
    ///
    /// Panics when a type is misrecognized.
    #[test]
    fn arrow_forms_are_recognized() {
        let arrow = |kind: TimestampKind| {
            let field = FieldSpec {
                name: "at".to_owned(),
                data_type: kind.data_type_spec(),
                nullable: true,
                metadata: BTreeMap::new(),
            };
            crate::schema::spec_to_field(&field, false)
                .data_type()
                .clone()
        };
        for kind in [TimestampKind::Ntz, TimestampKind::Ltz, TimestampKind::Tz] {
            assert_eq!(TimestampKind::of(&arrow(kind)), Some(kind));
        }
        assert_eq!(
            TimestampKind::of(&DataType::Timestamp(
                ArrowTimeUnit::Microsecond,
                Some("America/Chicago".into())
            )),
            Some(TimestampKind::Ltz)
        );
        let look_alike = DataType::Struct(Fields::from(vec![
            Field::new(TIMESTAMP_TZ_UTC, DataType::Utf8, false),
            Field::new(TIMESTAMP_TZ_LOCAL, DataType::Utf8, false),
        ]));
        assert_eq!(TimestampKind::of(&look_alike), None);
    }

    /// NTZ keeps the reading and refuses an offset; LTZ and TZ keep one
    /// instant from any offset and refuse text with none; TZ also keeps the
    /// writer's reading.
    ///
    /// # Panics
    ///
    /// Panics when a value parses to the wrong reading or instant.
    #[test]
    fn each_type_parses_its_own_text() {
        assert_eq!(parse_ntz("2026-10-06T12:00:00"), Some(CHICAGO_LOCAL));
        assert_eq!(parse_ntz("2026-10-06T12:00:00.000000"), Some(CHICAGO_LOCAL));
        assert_eq!(parse_ntz("2026-10-06T12:00:00-05:00"), None);
        assert_eq!(parse_ntz("2026-10-06T17:00:00Z"), None);

        for text in [
            "2026-10-06T17:00:00Z",
            "2026-10-06T17:00:00+00:00",
            "2026-10-06T12:00:00-05:00",
            "2026-10-07T02:00:00+09:00",
        ] {
            assert_eq!(parse_ltz(text), Some(MOMENT), "{text}");
        }
        assert_eq!(parse_ltz("2026-10-06T17:00:00"), None);

        assert_eq!(
            parse_tz("2026-10-06T12:00:00-05:00"),
            Some((MOMENT, CHICAGO_LOCAL))
        );
        assert_eq!(parse_tz("2026-10-06T12:00:00"), None);
    }

    /// Each Wyrd timestamp type writes and reads its own text, and a
    /// `TimestampTz` rebuilds the writer's offset from its stored struct.
    ///
    /// # Panics
    ///
    /// Panics when a value loses its reading, instant, or offset.
    #[test]
    fn wyrd_values_round_trip_their_text() {
        let chicago = FixedOffset::west_opt(5 * 3_600).expect("a valid offset");
        let at = chicago
            .with_ymd_and_hms(2026, 10, 6, 12, 0, 0)
            .single()
            .expect("one instant");
        let tz = TimestampTz(at);
        let ltz = TimestampLtz(at.to_utc());
        let ntz = TimestampNtz(at.naive_local());

        assert_eq!(
            serde_json::to_value(tz).expect("serializes"),
            "2026-10-06T12:00:00-05:00"
        );
        assert_eq!(
            serde_json::to_value(ltz).expect("serializes"),
            "2026-10-06T17:00:00Z"
        );
        assert_eq!(
            serde_json::to_value(ntz).expect("serializes"),
            "2026-10-06T12:00:00"
        );
        let read: TimestampTz =
            serde_json::from_value("2026-10-06T12:00:00-05:00".into()).expect("deserializes");
        let read_ltz: TimestampLtz =
            serde_json::from_value("2026-10-06T12:00:00-05:00".into()).expect("deserializes");
        let stored = TimestampTz::from_stored(MOMENT, CHICAGO_LOCAL).expect("rebuilds");

        assert_eq!(read.0.offset(), &chicago);
        assert_eq!(read_ltz, ltz);
        assert_eq!(stored, tz);
        assert_eq!(stored.0.offset(), &chicago);
    }

    /// Each type names itself, renders its stored form, and canonicalizes only
    /// text its write path accepts.
    ///
    /// # Panics
    ///
    /// Panics when a name, rendering, or canonical text is wrong.
    #[test]
    fn each_type_renders_and_checks_its_text() {
        for kind in [TimestampKind::Ntz, TimestampKind::Ltz, TimestampKind::Tz] {
            assert_eq!(TimestampKind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(
            TimestampKind::Ntz.render(CHICAGO_LOCAL, None).as_deref(),
            Some("2026-10-06T12:00:00")
        );
        assert_eq!(
            TimestampKind::Ltz.render(MOMENT + 1, None).as_deref(),
            Some("2026-10-06T17:00:00.000001Z")
        );
        assert_eq!(
            TimestampKind::Tz
                .render(MOMENT, Some(CHICAGO_LOCAL))
                .as_deref(),
            Some("2026-10-06T12:00:00-05:00")
        );
        assert_eq!(TimestampKind::Tz.render(MOMENT, None), None);
        assert_eq!(
            TimestampKind::Ltz
                .canonical("2026-10-06T12:00:00-05:00")
                .as_deref(),
            Some("2026-10-06T17:00:00Z")
        );
        assert_eq!(
            TimestampKind::Ntz.canonical("2026-10-06T12:00:00-05:00"),
            None
        );
        assert_eq!(TimestampKind::Tz.canonical("2026-10-06T12:00:00"), None);
    }

    /// Each Rust timestamp type's JSON Schema declares that type.
    ///
    /// # Panics
    ///
    /// Panics when a type's schema names another format.
    #[test]
    fn rust_types_declare_their_format() {
        /// The timestamp type a Rust type's JSON Schema declares.
        fn kind<T: JsonSchema>() -> Option<TimestampKind> {
            let schema = serde_json::to_value(schemars::schema_for!(T)).expect("serializes");
            schema["format"]
                .as_str()
                .and_then(TimestampKind::from_json_format)
        }

        assert_eq!(kind::<TimestampNtz>(), Some(TimestampKind::Ntz));
        assert_eq!(kind::<TimestampLtz>(), Some(TimestampKind::Ltz));
        assert_eq!(kind::<TimestampTz>(), Some(TimestampKind::Tz));
    }
}
