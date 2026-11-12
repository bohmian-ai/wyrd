//! Borrowed validation projections of the wire request contracts.
//!
//! [`wire_contract!`](crate::wire_contract) declares a contract once and emits
//! the owned type together with its borrowed projection, carrying the same
//! fields and Serde rules. Validating a decoded JSON document through the
//! projection applies exactly the owned rules while strings borrow from the
//! document and unconstrained JSON is only checked, so validation copies no
//! payload text. A projection is validation data, never a serializable
//! request.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;

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
/// checks its members, keeping only its first key, the member a rejection of
/// unmodeled members names.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IgnoredObject<'a> {
    /// The object's first key, borrowed unless it needed unescaping; `None`
    /// for an empty object or `null`.
    pub first_key: Option<Cow<'a, str>>,
}

/// A map key borrowed from the input when it holds no escapes.
#[derive(serde::Deserialize)]
#[serde(transparent)]
struct Key<'a>(#[serde(borrow)] Cow<'a, str>);

impl<'de: 'a, 'a> Deserialize<'de> for IgnoredObject<'a> {
    /// Accepts what an owned `Map<String, Value>` accepts, retaining only the
    /// first key.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// Visits an object, skipping each member after reading the first key.
        struct Visitor<'a>(PhantomData<IgnoredObject<'a>>);

        impl<'de: 'a, 'a> de::Visitor<'de> for Visitor<'a> {
            type Value = IgnoredObject<'a>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a map")
            }

            fn visit_unit<E: de::Error>(self) -> Result<IgnoredObject<'a>, E> {
                Ok(IgnoredObject::default())
            }

            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<IgnoredObject<'a>, A::Error> {
                let first_key = map.next_key::<Key<'a>>()?.map(|Key(key)| key);
                if first_key.is_some() {
                    map.next_value::<IgnoredAny>()?;
                    while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                }
                Ok(IgnoredObject { first_key })
            }
        }

        deserializer.deserialize_map(Visitor(PhantomData))
    }
}

impl<'a> Borrowed<'a> for String {
    type View = &'a str;
}

impl Borrowed<'_> for Value {
    type View = IgnoredAny;
}

impl<'a> Borrowed<'a> for Map<String, Value> {
    type View = IgnoredObject<'a>;
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
