//! Encode a typed value straight into a Variant builder.
//!
//! [`EncodedVariant::from_serialize`](super::EncodedVariant::from_serialize)
//! drives a [`serde::Serializer`] whose structs, maps and sequences are the
//! `parquet-variant` object and list builders, so a typed value is encoded in
//! one pass with no JSON text, re-parse or key map in between. It stores what
//! [`EncodedVariant::from_json_text`](super::EncodedVariant::from_json_text)
//! stores for the value's `serde_json` text: the same numbers, nulls, strings
//! and object fields under the module's rules.
//!
//! The encoder only takes values it can encode exactly. A container past
//! [`VARIANT_MAX_DEPTH`], an integer outside the 64-bit ranges, a map key that
//! does not serialize to a string, or a `Serialize` error stops it with
//! [`Fallback`], and the caller encodes the value through the JSON walk, the
//! one authority for every refusal and its pointer. An externally tagged enum
//! variant with fields cannot hold its wrapper and inner builders at once, so
//! its fields are collected as `serde_json` values and appended by
//! [`append_json`].

use std::fmt;

use parquet_variant::{ListBuilder, ObjectBuilder, ObjectFieldBuilder, Variant, VariantBuilderExt};
use serde::ser::{self, Serialize};
use serde_json::Value;
use wyrd_spec::vala::api::VARIANT_MAX_DEPTH;

use super::narrow_integer;

/// The encoder cannot store this value exactly; the JSON walk decides it.
#[derive(Debug)]
pub(super) struct Fallback;

impl fmt::Display for Fallback {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("value needs the JSON Variant walk")
    }
}

impl std::error::Error for Fallback {}

impl ser::Error for Fallback {
    fn custom<T: fmt::Display>(_: T) -> Self {
        Self
    }
}

/// Opens one container below `depth` others, refusing one past the limit.
///
/// # Errors
///
/// Returns [`Fallback`] for a container past [`VARIANT_MAX_DEPTH`].
fn enter(depth: u32) -> Result<u32, Fallback> {
    let depth = depth + 1;
    if depth > VARIANT_MAX_DEPTH {
        return Err(Fallback);
    }
    Ok(depth)
}

/// An unsigned integer as the JSON walk stores its token: the narrowest
/// integer within `i64`, else a scale-zero decimal.
fn unsigned(integer: u64) -> Variant<'static, 'static> {
    i64::try_from(integer).map_or_else(|_| Variant::from(integer), narrow_integer)
}

/// A double as `serde_json` writes it: a finite value is a Variant double and
/// a non-finite one is written as `null`.
fn double(double: f64) -> Variant<'static, 'static> {
    if double.is_finite() {
        Variant::from(double)
    } else {
        Variant::Null
    }
}

/// Appends one `serde_json` value below `depth` open containers.
///
/// The value's numbers are already exact 64-bit integers or finite doubles,
/// so only depth can refuse it. An object field whose value is null is
/// inserted as a Variant null, since a field builder would drop it.
///
/// # Errors
///
/// Returns [`Fallback`] for a container past [`VARIANT_MAX_DEPTH`] or a
/// builder refusal.
fn append_json(
    builder: &mut impl VariantBuilderExt,
    value: &Value,
    depth: u32,
) -> Result<(), Fallback> {
    match value {
        Value::Null => builder.append_value(Variant::Null),
        Value::Bool(flag) => builder.append_value(*flag),
        Value::Number(number) => builder.append_value(match (number.as_i64(), number.as_u64()) {
            (Some(integer), _) => narrow_integer(integer),
            (None, Some(integer)) => unsigned(integer),
            (None, None) => double(number.as_f64().ok_or(Fallback)?),
        }),
        Value::String(text) => builder.append_value(text.as_str()),
        Value::Array(items) => {
            let depth = enter(depth)?;
            let mut list = builder.try_new_list().map_err(|_| Fallback)?;
            for item in items {
                append_json(&mut list, item, depth)?;
            }
            list.finish();
        }
        Value::Object(entries) => {
            let depth = enter(depth)?;
            let mut object = builder.try_new_object().map_err(|_| Fallback)?;
            for (key, child) in entries {
                append_json(&mut ObjectFieldBuilder::new(key, &mut object), child, depth)?;
            }
            object.finish();
        }
    }
    Ok(())
}

/// Appends `variant: inner` as a one-field object, the external enum tag.
///
/// # Errors
///
/// Returns [`Fallback`] past the depth limit or on a builder refusal.
fn tagged(
    builder: &mut impl VariantBuilderExt,
    variant: &str,
    inner: &Value,
    depth: u32,
) -> Result<(), Fallback> {
    let depth = enter(depth)?;
    let mut object = builder.try_new_object().map_err(|_| Fallback)?;
    append_json(
        &mut ObjectFieldBuilder::new(variant, &mut object),
        inner,
        depth,
    )?;
    object.finish();
    Ok(())
}

/// A [`serde::Serializer`] writing one value into `builder` below `depth`
/// open containers.
pub(super) struct Encoder<'b, B> {
    /// Where the value is appended.
    pub(super) builder: &'b mut B,
    /// Containers already open above the value.
    pub(super) depth: u32,
}

impl<B: VariantBuilderExt> Encoder<'_, B> {
    /// Appends one scalar.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "every serializer method returns a Result"
    )]
    fn scalar<'m, 'v>(self, value: impl Into<Variant<'m, 'v>>) -> Result<(), Fallback> {
        self.builder.append_value(value);
        Ok(())
    }
}

impl<'b, B: VariantBuilderExt + 'b> ser::Serializer for Encoder<'b, B> {
    type Ok = ();
    type Error = Fallback;
    type SerializeSeq = List<'b, B::State<'b>>;
    type SerializeTuple = List<'b, B::State<'b>>;
    type SerializeTupleStruct = List<'b, B::State<'b>>;
    type SerializeTupleVariant = TaggedList<'b, B>;
    type SerializeMap = Map<'b, B::State<'b>>;
    type SerializeStruct = Object<'b, B::State<'b>>;
    type SerializeStructVariant = TaggedObject<'b, B>;

    fn serialize_bool(self, value: bool) -> Result<(), Fallback> {
        self.scalar(value)
    }
    fn serialize_i8(self, value: i8) -> Result<(), Fallback> {
        self.scalar(narrow_integer(value.into()))
    }
    fn serialize_i16(self, value: i16) -> Result<(), Fallback> {
        self.scalar(narrow_integer(value.into()))
    }
    fn serialize_i32(self, value: i32) -> Result<(), Fallback> {
        self.scalar(narrow_integer(value.into()))
    }
    fn serialize_i64(self, value: i64) -> Result<(), Fallback> {
        self.scalar(narrow_integer(value))
    }
    fn serialize_i128(self, value: i128) -> Result<(), Fallback> {
        if let Ok(integer) = i64::try_from(value) {
            return self.scalar(narrow_integer(integer));
        }
        self.scalar(unsigned(u64::try_from(value).map_err(|_| Fallback)?))
    }
    fn serialize_u8(self, value: u8) -> Result<(), Fallback> {
        self.scalar(narrow_integer(value.into()))
    }
    fn serialize_u16(self, value: u16) -> Result<(), Fallback> {
        self.scalar(narrow_integer(value.into()))
    }
    fn serialize_u32(self, value: u32) -> Result<(), Fallback> {
        self.scalar(narrow_integer(value.into()))
    }
    fn serialize_u64(self, value: u64) -> Result<(), Fallback> {
        self.scalar(unsigned(value))
    }
    fn serialize_u128(self, value: u128) -> Result<(), Fallback> {
        self.scalar(unsigned(u64::try_from(value).map_err(|_| Fallback)?))
    }
    fn serialize_f32(self, value: f32) -> Result<(), Fallback> {
        // `serde_json` writes the shortest `f32` text, which reads back as
        // that decimal's nearest double, not the widened `f32`.
        if !value.is_finite() {
            return self.scalar(Variant::Null);
        }
        self.scalar(double(value.to_string().parse().map_err(|_| Fallback)?))
    }
    fn serialize_f64(self, value: f64) -> Result<(), Fallback> {
        self.scalar(double(value))
    }
    fn serialize_char(self, value: char) -> Result<(), Fallback> {
        self.scalar(value.encode_utf8(&mut [0; 4]) as &str)
    }
    fn serialize_str(self, value: &str) -> Result<(), Fallback> {
        self.scalar(value)
    }
    fn serialize_bytes(self, value: &[u8]) -> Result<(), Fallback> {
        enter(self.depth)?;
        let mut list = self.builder.try_new_list().map_err(|_| Fallback)?;
        for byte in value {
            list.append_value(narrow_integer((*byte).into()));
        }
        list.finish();
        Ok(())
    }
    fn serialize_none(self) -> Result<(), Fallback> {
        self.scalar(Variant::Null)
    }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<(), Fallback> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), Fallback> {
        self.scalar(Variant::Null)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Fallback> {
        self.scalar(Variant::Null)
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<(), Fallback> {
        self.scalar(variant)
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), Fallback> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), Fallback> {
        let depth = enter(self.depth)?;
        let mut object = self.builder.try_new_object().map_err(|_| Fallback)?;
        value.serialize(Encoder {
            builder: &mut ObjectFieldBuilder::new(variant, &mut object),
            depth,
        })?;
        object.finish();
        Ok(())
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Fallback> {
        Ok(List {
            depth: enter(self.depth)?,
            list: self.builder.try_new_list().map_err(|_| Fallback)?,
        })
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Fallback> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Fallback> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Fallback> {
        Ok(TaggedList {
            encoder: self,
            variant,
            items: Vec::with_capacity(len),
        })
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Fallback> {
        Ok(Map {
            depth: enter(self.depth)?,
            object: self.builder.try_new_object().map_err(|_| Fallback)?,
            key: None,
        })
    }
    fn serialize_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, Fallback> {
        Ok(Object {
            depth: enter(self.depth)?,
            object: self.builder.try_new_object().map_err(|_| Fallback)?,
        })
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Fallback> {
        Ok(TaggedObject {
            encoder: self,
            variant,
            fields: serde_json::Map::new(),
        })
    }
}

/// A sequence being appended to its list builder.
pub(super) struct List<'b, S: parquet_variant::BuilderSpecificState> {
    /// The open list.
    list: ListBuilder<'b, S>,
    /// The list's own depth.
    depth: u32,
}

impl<S: parquet_variant::BuilderSpecificState> List<'_, S> {
    /// Appends one item.
    fn item<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Fallback> {
        value.serialize(Encoder {
            builder: &mut self.list,
            depth: self.depth,
        })
    }
}

impl<S: parquet_variant::BuilderSpecificState> ser::SerializeSeq for List<'_, S> {
    type Ok = ();
    type Error = Fallback;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Fallback> {
        self.item(value)
    }
    fn end(self) -> Result<(), Fallback> {
        self.list.finish();
        Ok(())
    }
}

impl<S: parquet_variant::BuilderSpecificState> ser::SerializeTuple for List<'_, S> {
    type Ok = ();
    type Error = Fallback;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Fallback> {
        self.item(value)
    }
    fn end(self) -> Result<(), Fallback> {
        self.list.finish();
        Ok(())
    }
}

impl<S: parquet_variant::BuilderSpecificState> ser::SerializeTupleStruct for List<'_, S> {
    type Ok = ();
    type Error = Fallback;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Fallback> {
        self.item(value)
    }
    fn end(self) -> Result<(), Fallback> {
        self.list.finish();
        Ok(())
    }
}

/// A struct being appended to its object builder.
pub(super) struct Object<'b, S: parquet_variant::BuilderSpecificState> {
    /// The open object.
    object: ObjectBuilder<'b, S>,
    /// The object's own depth.
    depth: u32,
}

impl<S: parquet_variant::BuilderSpecificState> ser::SerializeStruct for Object<'_, S> {
    type Ok = ();
    type Error = Fallback;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Fallback> {
        value.serialize(Encoder {
            builder: &mut ObjectFieldBuilder::new(key, &mut self.object),
            depth: self.depth,
        })
    }
    fn end(self) -> Result<(), Fallback> {
        self.object.finish();
        Ok(())
    }
}

/// A map being appended to its object builder; keys must serialize to
/// strings, as `serde_json` text keys are.
pub(super) struct Map<'b, S: parquet_variant::BuilderSpecificState> {
    /// The open object.
    object: ObjectBuilder<'b, S>,
    /// The object's own depth.
    depth: u32,
    /// The key whose value comes next.
    key: Option<String>,
}

impl<S: parquet_variant::BuilderSpecificState> ser::SerializeMap for Map<'_, S> {
    type Ok = ();
    type Error = Fallback;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), Fallback> {
        match key.serialize(serde_json::value::Serializer) {
            Ok(Value::String(key)) => {
                self.key = Some(key);
                Ok(())
            }
            _ => Err(Fallback),
        }
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Fallback> {
        let key = self.key.take().ok_or(Fallback)?;
        value.serialize(Encoder {
            builder: &mut ObjectFieldBuilder::new(&key, &mut self.object),
            depth: self.depth,
        })
    }
    fn end(self) -> Result<(), Fallback> {
        self.object.finish();
        Ok(())
    }
}

/// An externally tagged tuple variant: its items collected, then appended as
/// `{variant: [items]}`.
pub(super) struct TaggedList<'b, B> {
    /// Where the tagged value is appended.
    encoder: Encoder<'b, B>,
    /// The variant name, the wrapper's one key.
    variant: &'static str,
    /// The collected items.
    items: Vec<Value>,
}

impl<B: VariantBuilderExt> ser::SerializeTupleVariant for TaggedList<'_, B> {
    type Ok = ();
    type Error = Fallback;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Fallback> {
        self.items
            .push(serde_json::to_value(value).map_err(|_| Fallback)?);
        Ok(())
    }
    fn end(self) -> Result<(), Fallback> {
        let Self {
            encoder,
            variant,
            items,
        } = self;
        tagged(
            encoder.builder,
            variant,
            &Value::Array(items),
            encoder.depth,
        )
    }
}

/// An externally tagged struct variant: its fields collected, then appended
/// as `{variant: {fields}}`.
pub(super) struct TaggedObject<'b, B> {
    /// Where the tagged value is appended.
    encoder: Encoder<'b, B>,
    /// The variant name, the wrapper's one key.
    variant: &'static str,
    /// The collected fields.
    fields: serde_json::Map<String, Value>,
}

impl<B: VariantBuilderExt> ser::SerializeStructVariant for TaggedObject<'_, B> {
    type Ok = ();
    type Error = Fallback;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Fallback> {
        self.fields.insert(
            key.to_owned(),
            serde_json::to_value(value).map_err(|_| Fallback)?,
        );
        Ok(())
    }
    fn end(self) -> Result<(), Fallback> {
        let Self {
            encoder,
            variant,
            fields,
        } = self;
        tagged(
            encoder.builder,
            variant,
            &Value::Object(fields),
            encoder.depth,
        )
    }
}
