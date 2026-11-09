//! Borrowed validation projections of the wire request contracts.
//!
//! [`wire_contract!`](crate::wire_contract) declares a contract once and emits
//! the owned type together with its borrowed projection, carrying the same
//! fields and Serde rules. Validating a decoded JSON document through the
//! projection applies exactly the owned rules while strings borrow from the
//! document and unconstrained JSON is only checked, so validation copies no
//! payload text. A projection is validation data, never a serializable
//! request.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, Deserialize, Deserializer, IgnoredAny, MapAccess};
use serde_json::{Map, Value};

/// A contract type's borrowed validation projection.
///
/// `View` decodes with the same Serde rules as the implementing type while
/// borrowing its strings from data that outlives `'a`. Implemented by
/// [`wire_contract!`](crate::wire_contract) for declared contracts, as `Self`
/// for payload-free enums and scalars, and structurally for containers.
pub trait Borrowed<'a> {
    /// The projection decoded in place of the owned type.
    type View: Deserialize<'a>;
}

/// The projection of an arbitrary JSON object member: requires an object and
/// checks its members without retaining them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IgnoredObject;

impl<'de> Deserialize<'de> for IgnoredObject {
    /// Accepts what an owned `Map<String, Value>` accepts, retaining nothing.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// Visits an object, skipping each member.
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = IgnoredObject;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a map")
            }

            fn visit_unit<E: de::Error>(self) -> Result<IgnoredObject, E> {
                Ok(IgnoredObject)
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<IgnoredObject, A::Error> {
                while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(IgnoredObject)
            }
        }

        deserializer.deserialize_map(Visitor)
    }
}

impl<'a> Borrowed<'a> for String {
    type View = &'a str;
}

impl Borrowed<'_> for Value {
    type View = IgnoredAny;
}

impl Borrowed<'_> for Map<String, Value> {
    type View = IgnoredObject;
}

impl<'a, T: Borrowed<'a>> Borrowed<'a> for Option<T> {
    type View = Option<T::View>;
}

impl<'a, T: Borrowed<'a>> Borrowed<'a> for Vec<T> {
    type View = Vec<T::View>;
}

impl<'a, K: Borrowed<'a>, V: Borrowed<'a>> Borrowed<'a> for BTreeMap<K, V>
where
    K::View: Ord,
{
    type View = BTreeMap<K::View, V::View>;
}

/// Projects types as themselves, so validation decodes them owned: a
/// payload-free type copies nothing, and any other type keeps its owned
/// decode.
#[macro_export]
macro_rules! borrowed_as_self {
    ($($name:ty),* $(,)?) => {
        $(impl $crate::wire::borrowed::Borrowed<'_> for $name {
            type View = Self;
        })*
    };
}

borrowed_as_self!(bool, u32, u64, i64, f32, f64);

/// Declares a wire contract and its borrowed validation projection.
///
/// Each declaration names its projection after `as` and carries, in order,
/// doc comments, one `derive`, any `cfg_attr`, then `serde` attributes; fields
/// and variants carry doc comments then `serde` attributes. The owned type is
/// emitted unchanged. The projection keeps the docs and `serde` attributes,
/// derives only `Deserialize`, and projects every field through
/// [`Borrowed`](crate::wire::borrowed::Borrowed), so a field type without a
/// projection, or any other attribute, fails to compile instead of
/// validating differently.
#[macro_export]
macro_rules! wire_contract {
    (
        $(#[doc = $doc:expr])*
        #[derive($($derive:path),* $(,)?)]
        $(#[cfg_attr($($cfg:tt)*)])*
        $(#[serde($($serde:tt)*)])*
        $vis:vis struct $name:ident as $view:ident {
            $(
                $(#[doc = $fdoc:expr])*
                $(#[serde($($fserde:tt)*)])*
                $fvis:vis $field:ident : $fty:ty
            ),* $(,)?
        }
    ) => {
        $(#[doc = $doc])*
        #[derive($($derive),*)]
        $(#[cfg_attr($($cfg)*)])*
        $(#[serde($($serde)*)])*
        $vis struct $name {
            $(
                $(#[doc = $fdoc])*
                $(#[serde($($fserde)*)])*
                $fvis $field: $fty,
            )*
        }

        #[doc = concat!("Borrowed validation projection of [`", stringify!($name), "`].")]
        #[derive(::serde::Deserialize)]
        $(#[serde($($serde)*)])*
        $vis struct $view<'a> {
            $(
                $(#[doc = $fdoc])*
                $(#[serde($($fserde)*)])*
                #[serde(borrow)]
                $fvis $field: <$fty as $crate::wire::borrowed::Borrowed<'a>>::View,
            )*
        }

        impl<'a> $crate::wire::borrowed::Borrowed<'a> for $name {
            type View = $view<'a>;
        }
    };
    (
        $(#[doc = $doc:expr])*
        #[derive($($derive:path),* $(,)?)]
        $(#[cfg_attr($($cfg:tt)*)])*
        $(#[serde($($serde:tt)*)])*
        $vis:vis enum $name:ident as $view:ident {
            $(
                $(#[doc = $vdoc:expr])*
                $(#[serde($($vserde:tt)*)])*
                $variant:ident
                $(( $tty:ty ))?
                $({
                    $(
                        $(#[doc = $fdoc:expr])*
                        $(#[serde($($fserde:tt)*)])*
                        $field:ident : $fty:ty
                    ),* $(,)?
                })?
            ),* $(,)?
        }
    ) => {
        $(#[doc = $doc])*
        #[derive($($derive),*)]
        $(#[cfg_attr($($cfg)*)])*
        $(#[serde($($serde)*)])*
        $vis enum $name {
            $(
                $(#[doc = $vdoc])*
                $(#[serde($($vserde)*)])*
                $variant
                $(($tty))?
                $({
                    $(
                        $(#[doc = $fdoc])*
                        $(#[serde($($fserde)*)])*
                        $field: $fty,
                    )*
                })?,
            )*
        }

        #[doc = concat!("Borrowed validation projection of [`", stringify!($name), "`].")]
        #[derive(::serde::Deserialize)]
        $(#[serde($($serde)*)])*
        $vis enum $view<'a> {
            $(
                $(#[doc = $vdoc])*
                $(#[serde($($vserde)*)])*
                $variant
                $((
                    #[serde(borrow)]
                    <$tty as $crate::wire::borrowed::Borrowed<'a>>::View
                ))?
                $({
                    $(
                        $(#[doc = $fdoc])*
                        $(#[serde($($fserde)*)])*
                        #[serde(borrow)]
                        $field: <$fty as $crate::wire::borrowed::Borrowed<'a>>::View,
                    )*
                })?,
            )*
        }

        impl<'a> $crate::wire::borrowed::Borrowed<'a> for $name {
            type View = $view<'a>;
        }
    };
}
