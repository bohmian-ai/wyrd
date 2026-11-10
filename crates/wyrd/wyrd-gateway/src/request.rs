//! The operation request body a call carries from ingress to every attempt.

use std::borrow::Cow;
use std::ops::Range;
use std::sync::OnceLock;

use bytes::Bytes;
use serde::Deserialize;
use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use skald_providers::RequestBody;

/// Most members one object may hold for its text to be kept: duplicate names
/// are found by comparing each name with the object's earlier ones, so larger
/// objects decode instead.
// ponytail: quadratic duplicate check, a hash set if large canonical objects matter.
const MAX_CANONICAL_MEMBERS: usize = 64;

/// An operation request body: a decoded JSON value, or the compact text it
/// arrived as when that text is exactly the value's compact serialization.
///
/// Kept text is a JSON object without whitespace, duplicate member names, or
/// a string or number written differently from how `serde_json` writes the
/// decoded value, so its length is the compact serialization's length and
/// forwarding it sends the same bytes a decoded body would. Readers borrow
/// that text, and the decoded [`Value`] is built only when one asks for it
/// with [`Self::tree`], as translation, capture, multipart encoding, and
/// views that cannot borrow escaped text do. Any other body is decoded once
/// on arrival.
#[derive(Debug, Clone, Default)]
pub struct RequestJson {
    /// The kept compact text and its top-level members, in order.
    text: Option<(Bytes, Vec<Member>)>,
    /// The decoded body, built on first use when text is kept.
    tree: OnceLock<Value>,
}

/// Byte ranges of one top-level member of kept text.
#[derive(Debug, Clone)]
struct Member {
    /// The name as written, without its quotes.
    name: Range<usize>,
    /// The value as written.
    value: Range<usize>,
}

impl From<Value> for RequestJson {
    /// Carries an already-decoded body.
    fn from(value: Value) -> Self {
        Self {
            text: None,
            tree: OnceLock::from(value),
        }
    }
}

impl PartialEq for RequestJson {
    /// Bodies are equal when their decoded values are.
    fn eq(&self, other: &Self) -> bool {
        self.text() == other.text() && self.text().is_some() || self.tree() == other.tree()
    }
}

impl RequestJson {
    /// Takes a JSON request body as it arrived.
    ///
    /// The body is read once as a [`Value`] decode reads it, with the same
    /// syntax, number, nesting, and trailing-input rules, while each decoded
    /// part is compared with its compact serialization; text that matches
    /// throughout is kept without decoding, and any other body is decoded.
    ///
    /// # Errors
    ///
    /// Returns the JSON error of a body that does not decode as a [`Value`].
    pub fn parse(text: Bytes) -> serde_json::Result<Self> {
        if let Some(members) = canonical(&text) {
            return Ok(Self {
                text: Some((text, members)),
                tree: OnceLock::new(),
            });
        }
        serde_json::from_slice::<Value>(&text).map(Self::from)
    }

    /// The kept compact text, when the body arrived as it.
    #[must_use]
    pub fn text(&self) -> Option<&[u8]> {
        self.text.as_ref().map(|(text, _)| &text[..])
    }

    /// The decoded body, decoding kept text on first use.
    ///
    /// # Panics
    ///
    /// Never in practice: kept text decoded as a [`Value`] when it arrived.
    pub fn tree(&self) -> &Value {
        self.tree.get_or_init(|| match &self.text {
            Some((text, _)) => serde_json::from_slice(text).expect("kept text decoded on arrival"),
            None => Value::Null,
        })
    }

    /// Moves the decoded body out, decoding kept text.
    #[must_use]
    pub fn into_tree(self) -> Value {
        self.tree();
        self.tree.into_inner().unwrap_or_default()
    }

    /// Whether the body is a JSON object.
    #[must_use]
    pub fn is_object(&self) -> bool {
        self.text.is_some() || self.tree().is_object()
    }

    /// The top-level member `name` of an object body, decoded from kept text
    /// or borrowed from the decoded body.
    ///
    /// # Panics
    ///
    /// Never in practice: every member of kept text decoded on arrival.
    #[must_use]
    pub fn member(&self, name: &str) -> Option<Cow<'_, Value>> {
        let Some((text, members)) = &self.text else {
            return self.tree().get(name).map(Cow::Borrowed);
        };
        members
            .iter()
            .find(|member| &text[member.name.clone()] == name.as_bytes())
            .map(|member| {
                Cow::Owned(
                    serde_json::from_slice(&text[member.value.clone()])
                        .expect("kept member decoded on arrival"),
                )
            })
    }

    /// Decodes the body as `T`, borrowing kept text when `T` can.
    ///
    /// Kept text has no duplicate member and writes each value as its decode
    /// would, so `T` reads it as it reads the decoded body; when it cannot,
    /// as a borrowed string cannot hold escaped text, the decoded body is
    /// read instead, so the result and its error are the decoded body's.
    /// Once the decoded body exists, views read it directly.
    ///
    /// # Errors
    ///
    /// Returns the error of decoding the body as `T`.
    pub fn view<'a, T: Deserialize<'a>>(&'a self) -> serde_json::Result<T> {
        if let Some((text, _)) = &self.text
            && self.tree.get().is_none()
            && let Ok(view) = serde_json::from_slice(text)
        {
            return Ok(view);
        }
        T::deserialize(self.tree())
    }

    /// Compact JSON bytes of an object body with `model` applied as
    /// [`RequestBody::Object`] applies it: a present `model` member keeps its
    /// position with the new value and an absent one follows the others.
    ///
    /// Kept text is copied member by member, so the bytes equal those of the
    /// decoded body.
    ///
    /// # Errors
    ///
    /// Returns the serialization error of a member serde cannot write, or of
    /// a decoded body that is not an object.
    pub fn to_json(&self, model: Option<&str>) -> serde_json::Result<Vec<u8>> {
        let Some((text, members)) = &self.text else {
            let Value::Object(members) = self.tree() else {
                return Err(de::Error::custom("request body is not a JSON object"));
            };
            return RequestBody::Object { members, model }.to_json();
        };
        let mut json = Vec::with_capacity(text.len() + model.map_or(0, |model| model.len() + 11));
        let mut replaced = false;
        json.push(b'{');
        for member in members {
            if json.len() > 1 {
                json.push(b',');
            }
            json.extend_from_slice(&text[member.name.start - 1..member.value.start]);
            match model {
                Some(model) if &text[member.name.clone()] == b"model" => {
                    replaced = true;
                    serde_json::to_writer(&mut json, model)?;
                }
                _ => json.extend_from_slice(&text[member.value.clone()]),
            }
        }
        if let Some(model) = model.filter(|_| !replaced) {
            if json.len() > 1 {
                json.push(b',');
            }
            json.extend_from_slice(br#""model":"#);
            serde_json::to_writer(&mut json, model)?;
        }
        json.push(b'}');
        Ok(json)
    }
}

/// The top-level members of `text` when it is a JSON object written exactly
/// as its decode serializes compactly, or `None`.
fn canonical(text: &[u8]) -> Option<Vec<Member>> {
    let mut state = Canonical {
        text,
        at: 0,
        names: Vec::new(),
        members: Vec::new(),
    };
    let mut deserializer = serde_json::Deserializer::from_slice(text);
    Part {
        state: &mut state,
        after: b"",
        top: true,
    }
    .deserialize(&mut deserializer)
    .ok()?;
    deserializer.end().ok()?;
    (state.at == text.len()).then_some(state.members)
}

/// Progress of [`canonical`]: the text, how much of it the compact
/// serialization of what was decoded so far matched, the member names of the
/// objects being read, and the top-level members found.
struct Canonical<'de> {
    text: &'de [u8],
    at: usize,
    names: Vec<Cow<'de, str>>,
    members: Vec<Member>,
}

impl Canonical<'_> {
    /// Matches `bytes` against the text at the current position.
    fn literal<E: de::Error>(&mut self, bytes: &[u8]) -> Result<(), E> {
        let end = self.at + bytes.len();
        if self.text.get(self.at..end) != Some(bytes) {
            return Err(E::custom("not compact"));
        }
        self.at = end;
        Ok(())
    }

    /// Matches the compact serialization of `value` at the current position,
    /// comparing it as it is written.
    fn written<E: de::Error, T: serde::Serialize + ?Sized>(&mut self, value: &T) -> Result<(), E> {
        serde_json::to_writer(&mut *self, value).map_err(|_| E::custom("not compact"))
    }

    /// Matches a string `serde_json` borrowed from the text without
    /// formatting it: it borrows only a string written without escapes, and
    /// rejects raw control characters, so its compact serialization is the
    /// string itself between quotes.
    fn borrowed<E: de::Error>(&mut self, value: &str) -> Result<(), E> {
        self.literal(b"\"")?;
        self.literal(value.as_bytes())?;
        self.literal(b"\"")
    }
}

impl std::io::Write for Canonical<'_> {
    /// Accepts `bytes` only when the text continues with them.
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.literal::<serde_json::Error>(bytes)
            .map(|()| bytes.len())
            .map_err(std::io::Error::other)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// One value of [`canonical`]'s text, read through `deserialize_any` so
/// `serde_json` applies a [`Value`] decode's rules, after the separator
/// `after` that precedes it; `top` marks the body.
struct Part<'s, 'de> {
    state: &'s mut Canonical<'de>,
    after: &'static [u8],
    top: bool,
}

impl<'de> DeserializeSeed<'de> for Part<'_, 'de> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        self.state.literal(self.after)?;
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Part<'_, 'de> {
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON object")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<(), E> {
        self.scalar(&value)
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<(), E> {
        self.scalar(&value)
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<(), E> {
        self.scalar(&value)
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<(), E> {
        self.scalar(&value)
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<(), E> {
        self.scalar(value)
    }

    fn visit_borrowed_str<E: de::Error>(self, value: &'de str) -> Result<(), E> {
        if self.top {
            return Err(E::custom("not an object"));
        }
        self.state.borrowed(value)
    }

    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        self.scalar(&())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        if self.top {
            return Err(de::Error::custom("not an object"));
        }
        self.state.literal(b"[")?;
        let mut after: &[u8] = b"";
        while seq
            .next_element_seed(Part {
                state: &mut *self.state,
                after,
                top: false,
            })?
            .is_some()
        {
            after = b",";
        }
        self.state.literal(b"]")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        self.state.literal(b"{")?;
        let earlier = self.state.names.len();
        while let Some(Name(name)) = map.next_key()? {
            if self.state.names.len() > earlier {
                self.state.literal(b",")?;
            }
            if self.state.names.len() - earlier >= MAX_CANONICAL_MEMBERS
                || self.state.names[earlier..].contains(&name)
            {
                return Err(de::Error::custom("not unique"));
            }
            let start = self.state.at;
            match &name {
                Cow::Borrowed(name) => self.state.borrowed(name)?,
                Cow::Owned(name) => self.state.written(name)?,
            }
            let named = start + 1..self.state.at - 1;
            self.state.names.push(name);
            self.state.literal(b":")?;
            let value = self.state.at;
            map.next_value_seed(Part {
                state: &mut *self.state,
                after: b"",
                top: false,
            })?;
            if self.top {
                let end = self.state.at;
                self.state.members.push(Member {
                    name: named,
                    value: value..end,
                });
            }
        }
        self.state.names.truncate(earlier);
        self.state.literal(b"}")
    }
}

impl Part<'_, '_> {
    /// Matches a scalar's compact serialization, which a body never is.
    fn scalar<E: de::Error, T: serde::Serialize + ?Sized>(self, value: &T) -> Result<(), E> {
        if self.top {
            return Err(E::custom("not an object"));
        }
        self.state.written(value)
    }
}

/// An object member name, borrowed from the text unless it holds escapes.
#[derive(Deserialize)]
#[serde(transparent)]
struct Name<'a>(#[serde(borrow)] Cow<'a, str>);

#[cfg(test)]
mod tests {
    use super::*;

    /// Text is kept exactly when it is its decode's compact serialization,
    /// whether its strings borrow from the text or hold escapes, and the kept
    /// members are the top-level names and values as written.
    #[test]
    fn canonical_borrowed_strings_match_compact_serialization() {
        let deep = format!(r#"{{"a":{}1{}}}"#, "[".repeat(200), "]".repeat(200));
        let members = |count: usize| {
            let members: Vec<String> = (0..count).map(|i| format!(r#""m{i}":{i}"#)).collect();
            format!("{{{}}}", members.join(","))
        };
        let cases = [
            (
                r#"{"model":"gpt-4o","messages":[{"role":"user","content":"hi"}]}"#,
                true,
            ),
            ("{\"name\":\"café ☕ 𝄞\",\"del\":\"\u{7f}\"}", true),
            (r#"{"name":"caf\u00e9"}"#, false),
            (r#"{"q":"say \"hi\"\\ \n\t\r\b\f"}"#, true),
            (r#"{"c":"\u0001"}"#, true),
            (r#"{"c":"\u001F"}"#, false),
            (r#"{"s":"a\/b"}"#, false),
            ("{\"c\":\"\u{1}\"}", false),
            (r#"{"say \"hi\"":1,"say":2}"#, true),
            (
                r#"{"n":1.5,"i":-7,"u":18446744073709551615,"f":0.1,"z":0}"#,
                true,
            ),
            (r#"{"n":1.0}"#, true),
            (r#"{"n":1.50}"#, false),
            (r#"{"n":1e3}"#, false),
            (r#"{"n":-0}"#, false),
            (r#"{"a":1,"a":2}"#, false),
            (r#"{"ab":1,"ab":2}"#, false),
            (r#"{"a":{"b":1,"b":2}}"#, false),
            (r#"{"a":{"b":1},"c":{"b":2}}"#, true),
            (r#"{"a":[true,false,null,"x",{},[]]}"#, true),
            (r#"{ "a":1}"#, false),
            (r#"{"a":1} "#, false),
            (r#"["a"]"#, false),
            (r#""a""#, false),
            (&deep, false),
            (&members(MAX_CANONICAL_MEMBERS), true),
            (&members(MAX_CANONICAL_MEMBERS + 1), false),
        ];
        for (text, kept) in cases {
            assert_eq!(canonical(text.as_bytes()).is_some(), kept, "{text}");
            if kept {
                let compact =
                    serde_json::to_vec(&serde_json::from_str::<Value>(text).expect("decodes"))
                        .expect("serializes");
                assert_eq!(compact.len(), text.len(), "{text}: compact bound");
            }
        }

        let text = br#"{"say \"hi\"":"caf\u00e9x","model":"gpt-4o","n":[1,{"k":"v"}]}"#;
        assert!(canonical(text).is_none(), "an escape compact writes raw");
        let text = br#"{"say \"hi\"":"caf\"x","model":"gpt-4o","n":[1,{"k":"v"}]}"#;
        let written: Vec<(&[u8], &[u8])> = canonical(text)
            .expect("kept")
            .into_iter()
            .map(|member| (&text[member.name], &text[member.value]))
            .collect();
        assert_eq!(
            written,
            [
                (&br#"say \"hi\""#[..], &br#""caf\"x""#[..]),
                (b"model", br#""gpt-4o""#),
                (b"n", br#"[1,{"k":"v"}]"#),
            ]
        );
    }
}
