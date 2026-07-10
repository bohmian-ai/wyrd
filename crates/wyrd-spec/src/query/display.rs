//! Canonical printer for metadata queries.

use std::fmt;

use crate::query::parse::escape_string;
use crate::query::{FieldRef, MetadataQuery, Operator, Value};

impl fmt::Display for MetadataQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_prec(f, 0)
    }
}

impl MetadataQuery {
    fn fmt_prec(&self, f: &mut fmt::Formatter<'_>, parent_prec: u8) -> fmt::Result {
        match self {
            Self::Predicate(predicate) => {
                write!(f, "{} {}", predicate.field, predicate.op)
            }
            Self::Not(inner) => {
                let needs_parens = matches!(inner.as_ref(), Self::And(_) | Self::Or(_));
                if 3 < parent_prec {
                    write!(f, "(")?;
                }
                write!(f, "not ")?;
                if needs_parens {
                    write!(f, "(")?;
                    inner.fmt_prec(f, 0)?;
                    write!(f, ")")?;
                } else {
                    inner.fmt_prec(f, 3)?;
                }
                if 3 < parent_prec {
                    write!(f, ")")?;
                }
                Ok(())
            }
            Self::And(children) => {
                debug_assert!(
                    !children.is_empty(),
                    "empty And must be validated before display"
                );
                if 2 < parent_prec {
                    write!(f, "(")?;
                }
                for (idx, child) in children.iter().enumerate() {
                    if idx > 0 {
                        write!(f, " and ")?;
                    }
                    if matches!(child, Self::And(_)) {
                        write!(f, "(")?;
                        child.fmt_prec(f, 0)?;
                        write!(f, ")")?;
                    } else {
                        child.fmt_prec(f, 2)?;
                    }
                }
                if 2 < parent_prec {
                    write!(f, ")")?;
                }
                Ok(())
            }
            Self::Or(children) => {
                debug_assert!(
                    !children.is_empty(),
                    "empty Or must be validated before display"
                );
                if 1 < parent_prec {
                    write!(f, "(")?;
                }
                for (idx, child) in children.iter().enumerate() {
                    if idx > 0 {
                        write!(f, " or ")?;
                    }
                    if matches!(child, Self::Or(_)) {
                        write!(f, "(")?;
                        child.fmt_prec(f, 0)?;
                        write!(f, ")")?;
                    } else {
                        child.fmt_prec(f, 1)?;
                    }
                }
                if 1 < parent_prec {
                    write!(f, ")")?;
                }
                Ok(())
            }
        }
    }
}

impl fmt::Display for FieldRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Label(key) => write!(f, "labels.{}", fmt_key(key)),
            Self::Annotation(key) => write!(f, "annotations.{}", fmt_key(key)),
            Self::Attribute(key) => write!(f, "attributes.{}", fmt_key(key)),
            Self::Reserved(name) => f.write_str(name),
        }
    }
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Eq(value) => write!(f, "= {value}"),
            Self::Ne(value) => write!(f, "!= {value}"),
            Self::Gt(value) => write!(f, "> {value}"),
            Self::Ge(value) => write!(f, ">= {value}"),
            Self::Lt(value) => write!(f, "< {value}"),
            Self::Le(value) => write!(f, "<= {value}"),
            Self::Matches(regex) => write!(f, "=~ \"{}\"", escape_string(regex)),
            Self::NotMatches(regex) => write!(f, "!~ \"{}\"", escape_string(regex)),
            Self::In(values) => write!(f, "in {}", fmt_values(values)),
            Self::NotIn(values) => write!(f, "not in {}", fmt_values(values)),
            Self::Exists => f.write_str("exists"),
            Self::NotExists => f.write_str("not exists"),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(value) => write!(f, "\"{}\"", escape_string(value)),
            Self::Int(value) => write!(f, "{value}"),
            Self::Float(value) => f.write_str(&fmt_float(*value)),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Duration(nanos) => write!(f, "{nanos}ns"),
            Self::Timestamp(value) => write!(f, "\"{}\"", value.to_rfc3339()),
        }
    }
}

fn fmt_values(values: &[Value]) -> String {
    let rendered = values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{rendered}]")
}

fn fmt_float(value: f64) -> String {
    let rendered = value.to_string();
    if rendered.contains('.') || rendered.contains('e') || rendered.contains('E') {
        rendered
    } else {
        format!("{rendered}.0")
    }
}

fn fmt_key(key: &str) -> String {
    if key.split('.').all(valid_bare_key_segment) {
        key.to_owned()
    } else {
        format!("\"{}\"", escape_string(key))
    }
}

fn valid_bare_key_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '/' | '-'))
}
