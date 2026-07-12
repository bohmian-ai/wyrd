//! String parser for metadata queries.

use crate::error::WyrdError;
use crate::query::{FieldRef, MAX_DEPTH, MetadataQuery, Operator, Predicate, Value};

/// Maximum accepted byte length for a raw query string.
pub(crate) const MAX_QUERY_INPUT_LEN: usize = 4096;

/// Parse a metadata query string into an AST.
///
/// # Errors
/// Returns `WYRD_QUERY_400_TOO_COMPLEX` when the input exceeds `MAX_QUERY_INPUT_LEN`.
/// Returns `WYRD_QUERY_400_INVALID_SYNTAX` when lexing or parsing fails.
pub(crate) fn parse_query(input: &str) -> Result<MetadataQuery, WyrdError> {
    if input.len() > MAX_QUERY_INPUT_LEN {
        return Err(WyrdError::query_too_complex(
            format!(
                "query string length {} exceeds max {MAX_QUERY_INPUT_LEN}",
                input.len()
            ),
            "input_length",
            input.len(),
            MAX_QUERY_INPUT_LEN,
        ));
    }
    let tokens = lex(input)?;
    let mut parser = Parser {
        tokens,
        cursor: 0,
        input_len: input.len(),
    };
    let query = parser.parse_or(0)?;
    parser.expect_eof()?;
    Ok(query)
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Dot,
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Match,
    NotMatch,
    And,
    Or,
    Not,
    In,
    Exists,
    Ident(String),
    Str(String),
    Int(i64),
    Float(f64),
    Duration(i64),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq)]
struct Spanned {
    tok: Tok,
    offset: usize,
}

struct Lexer<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

fn lex(input: &str) -> Result<Vec<Spanned>, WyrdError> {
    let mut lexer = Lexer {
        input,
        bytes: input.as_bytes(),
        pos: 0,
    };
    lexer.lex()
}

impl Lexer<'_> {
    fn lex(&mut self) -> Result<Vec<Spanned>, WyrdError> {
        let mut tokens = Vec::new();
        while self.pos < self.bytes.len() {
            self.skip_ws();
            if self.pos >= self.bytes.len() {
                break;
            }
            let offset = self.pos;
            let tok = match self.bytes[self.pos] {
                b'(' => {
                    self.pos += 1;
                    Tok::LParen
                }
                b')' => {
                    self.pos += 1;
                    Tok::RParen
                }
                b'[' => {
                    self.pos += 1;
                    Tok::LBracket
                }
                b']' => {
                    self.pos += 1;
                    Tok::RBracket
                }
                b',' => {
                    self.pos += 1;
                    Tok::Comma
                }
                b'.' => {
                    self.pos += 1;
                    Tok::Dot
                }
                b'=' if self.peek_byte(1) == Some(b'~') => {
                    self.pos += 2;
                    Tok::Match
                }
                b'=' => {
                    self.pos += 1;
                    Tok::Eq
                }
                b'!' if self.peek_byte(1) == Some(b'=') => {
                    self.pos += 2;
                    Tok::Ne
                }
                b'!' if self.peek_byte(1) == Some(b'~') => {
                    self.pos += 2;
                    Tok::NotMatch
                }
                b'>' if self.peek_byte(1) == Some(b'=') => {
                    self.pos += 2;
                    Tok::Ge
                }
                b'>' => {
                    self.pos += 1;
                    Tok::Gt
                }
                b'<' if self.peek_byte(1) == Some(b'=') => {
                    self.pos += 2;
                    Tok::Le
                }
                b'<' => {
                    self.pos += 1;
                    Tok::Lt
                }
                b'"' => Tok::Str(self.string_lit()?),
                b'-' | b'0'..=b'9' => self.number_or_duration()?,
                b'A'..=b'Z' | b'a'..=b'z' | b'_' => self.ident_or_keyword(),
                other => {
                    return Err(syntax(
                        format!("unexpected byte at offset {offset}"),
                        offset,
                        "token",
                        char::from(other).to_string(),
                    ));
                }
            };
            tokens.push(Spanned { tok, offset });
        }
        Ok(tokens)
    }

    fn skip_ws(&mut self) {
        while self
            .bytes
            .get(self.pos)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            self.pos += 1;
        }
    }

    fn peek_byte(&self, ahead: usize) -> Option<u8> {
        self.bytes.get(self.pos + ahead).copied()
    }

    fn string_lit(&mut self) -> Result<String, WyrdError> {
        let start = self.pos;
        self.pos += 1;
        let mut out = String::new();
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b'"' => {
                    self.pos += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.pos += 1;
                    let Some(escaped) = self.bytes.get(self.pos).copied() else {
                        return Err(syntax(
                            "unterminated escape sequence",
                            self.pos,
                            "escape",
                            "<eof>",
                        ));
                    };
                    self.pos += 1;
                    match escaped {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        other => {
                            return Err(syntax(
                                "unsupported escape sequence",
                                self.pos - 1,
                                r#"" \ n t r"#,
                                char::from(other).to_string(),
                            ));
                        }
                    }
                }
                _ => {
                    let rest = &self.input[self.pos..];
                    let Some(ch) = rest.chars().next() else {
                        return Err(syntax(
                            "unterminated string literal",
                            self.pos,
                            "\"",
                            "<eof>",
                        ));
                    };
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
        Err(syntax("unterminated string literal", start, "\"", "<eof>"))
    }

    fn number_or_duration(&mut self) -> Result<Tok, WyrdError> {
        let start = self.pos;
        if self.bytes[self.pos] == b'-' {
            self.pos += 1;
            if !self
                .bytes
                .get(self.pos)
                .is_some_and(|byte| byte.is_ascii_digit())
            {
                return Err(syntax("expected digit after '-'", start, "digit", "-"));
            }
        }
        while self
            .bytes
            .get(self.pos)
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            self.pos += 1;
        }

        let mut is_float = false;
        if self.bytes.get(self.pos) == Some(&b'.')
            && self
                .bytes
                .get(self.pos + 1)
                .is_some_and(|byte| byte.is_ascii_digit())
        {
            is_float = true;
            self.pos += 1;
            while self
                .bytes
                .get(self.pos)
                .is_some_and(|byte| byte.is_ascii_digit())
            {
                self.pos += 1;
            }
        }

        let numeric_end = self.pos;
        let mut unit_end = self.pos;
        while self
            .bytes
            .get(unit_end)
            .is_some_and(|byte| byte.is_ascii_alphabetic())
        {
            unit_end += 1;
        }
        if unit_end > numeric_end {
            let unit = &self.input[numeric_end..unit_end];
            if is_float {
                return Err(syntax(
                    "duration requires an integer magnitude",
                    start,
                    "integer duration",
                    &self.input[start..unit_end],
                ));
            }
            self.pos = unit_end;
            let magnitude = self.input[start..numeric_end].parse::<i64>().map_err(|_| {
                syntax(
                    "invalid integer",
                    start,
                    "integer",
                    &self.input[start..numeric_end],
                )
            })?;
            if magnitude < 0 {
                return Err(syntax(
                    "duration cannot be negative",
                    start,
                    "positive duration",
                    &self.input[start..unit_end],
                ));
            }
            let factor = match unit {
                "ns" => 1_i64,
                "us" => 1_000,
                "ms" => 1_000_000,
                "s" => 1_000_000_000,
                "m" => 60_000_000_000,
                "h" => 3_600_000_000_000,
                _ => {
                    return Err(syntax(
                        "unknown duration unit",
                        numeric_end,
                        "duration unit",
                        unit,
                    ));
                }
            };
            return magnitude
                .checked_mul(factor)
                .map(Tok::Duration)
                .ok_or_else(|| {
                    syntax(
                        "duration overflow",
                        start,
                        "duration within i64 nanos",
                        &self.input[start..unit_end],
                    )
                });
        }

        if !is_float && self.bytes.get(self.pos) == Some(&b'-') {
            let found_end = self.input[self.pos..]
                .find(char::is_whitespace)
                .map_or(self.input.len(), |idx| self.pos + idx);
            return Err(syntax(
                "timestamps must be quoted",
                start,
                "quoted timestamp",
                &self.input[start..found_end],
            ));
        }

        let raw = &self.input[start..self.pos];
        if is_float {
            raw.parse::<f64>()
                .map(Tok::Float)
                .map_err(|_| syntax("invalid float", start, "float", raw))
        } else {
            raw.parse::<i64>()
                .map(Tok::Int)
                .map_err(|_| syntax("invalid integer", start, "integer", raw))
        }
    }

    fn ident_or_keyword(&mut self) -> Tok {
        let start = self.pos;
        self.pos += 1;
        while self
            .bytes
            .get(self.pos)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'/' | b'-'))
        {
            self.pos += 1;
        }
        let raw = &self.input[start..self.pos];
        match raw.to_ascii_lowercase().as_str() {
            "and" => Tok::And,
            "or" => Tok::Or,
            "not" => Tok::Not,
            "in" => Tok::In,
            "exists" => Tok::Exists,
            "true" => Tok::Bool(true),
            "false" => Tok::Bool(false),
            _ => Tok::Ident(raw.to_owned()),
        }
    }
}

struct Parser {
    tokens: Vec<Spanned>,
    cursor: usize,
    input_len: usize,
}

impl Parser {
    fn parse_or(&mut self, depth: usize) -> Result<MetadataQuery, WyrdError> {
        if depth > MAX_DEPTH {
            return Err(WyrdError::query_too_complex(
                format!("query nesting depth {depth} exceeds max {MAX_DEPTH}"),
                "depth",
                depth,
                MAX_DEPTH,
            ));
        }
        let mut children = vec![self.parse_and(depth)?];
        while self.matches(|tok| matches!(tok, Tok::Or)) {
            children.push(self.parse_and(depth)?);
        }
        Ok(if children.len() == 1 {
            children.remove(0)
        } else {
            MetadataQuery::Or(children)
        })
    }

    fn parse_and(&mut self, depth: usize) -> Result<MetadataQuery, WyrdError> {
        let mut children = vec![self.parse_unary(depth)?];
        while self.matches(|tok| matches!(tok, Tok::And)) {
            children.push(self.parse_unary(depth)?);
        }
        Ok(if children.len() == 1 {
            children.remove(0)
        } else {
            MetadataQuery::And(children)
        })
    }

    fn parse_unary(&mut self, depth: usize) -> Result<MetadataQuery, WyrdError> {
        if self.matches(|tok| matches!(tok, Tok::Not)) {
            return Ok(MetadataQuery::Not(Box::new(self.parse_unary(depth + 1)?)));
        }
        if self.matches(|tok| matches!(tok, Tok::LParen)) {
            let query = self.parse_or(depth + 1)?;
            self.expect_tok(|tok| matches!(tok, Tok::RParen), ")")?;
            return Ok(query);
        }
        self.parse_predicate()
    }

    fn parse_predicate(&mut self) -> Result<MetadataQuery, WyrdError> {
        let field = self.parse_field()?;
        let Some(tok) = self.peek() else {
            return Err(self.syntax_at_eof("operator"));
        };
        let op = match &tok.tok {
            Tok::Eq => {
                self.cursor += 1;
                Operator::Eq(self.parse_value()?)
            }
            Tok::Ne => {
                self.cursor += 1;
                Operator::Ne(self.parse_value()?)
            }
            Tok::Gt => {
                self.cursor += 1;
                Operator::Gt(self.parse_value()?)
            }
            Tok::Ge => {
                self.cursor += 1;
                Operator::Ge(self.parse_value()?)
            }
            Tok::Lt => {
                self.cursor += 1;
                Operator::Lt(self.parse_value()?)
            }
            Tok::Le => {
                self.cursor += 1;
                Operator::Le(self.parse_value()?)
            }
            Tok::Match => {
                self.cursor += 1;
                Operator::Matches(self.expect_string()?)
            }
            Tok::NotMatch => {
                self.cursor += 1;
                Operator::NotMatches(self.expect_string()?)
            }
            Tok::In => {
                self.cursor += 1;
                Operator::In(self.parse_list()?)
            }
            Tok::Not => {
                self.cursor += 1;
                if self.matches(|tok| matches!(tok, Tok::In)) {
                    Operator::NotIn(self.parse_list()?)
                } else if self.matches(|tok| matches!(tok, Tok::Exists)) {
                    Operator::NotExists
                } else {
                    return Err(self.error_here("in or exists"));
                }
            }
            Tok::Exists => {
                self.cursor += 1;
                Operator::Exists
            }
            _ => return Err(self.error_here("operator")),
        };
        Ok(MetadataQuery::Predicate(Predicate { field, op }))
    }

    fn parse_field(&mut self) -> Result<FieldRef, WyrdError> {
        let Some(spanned) = self.advance().cloned() else {
            return Err(self.syntax_at_eof("field"));
        };
        let Tok::Ident(ident) = spanned.tok else {
            return Err(syntax(
                "expected field",
                spanned.offset,
                "field",
                token_name(&spanned.tok),
            ));
        };

        if matches!(ident.as_str(), "labels" | "annotations" | "attributes")
            && self.matches(|tok| matches!(tok, Tok::Dot))
        {
            let key = self.parse_key_path()?;
            return Ok(match ident.as_str() {
                "labels" => FieldRef::Label(key),
                "annotations" => FieldRef::Annotation(key),
                "attributes" => FieldRef::Attribute(key),
                _ => unreachable!("namespace checked above"),
            });
        }

        if self.peek().is_some_and(|tok| matches!(tok.tok, Tok::Dot)) {
            return Err(syntax(
                "unexpected '.'; only labels/annotations/attributes take a dotted key",
                self.peek().map_or(self.input_len, |tok| tok.offset),
                "operator",
                ".",
            ));
        }

        Ok(FieldRef::Reserved(ident))
    }

    fn parse_key_path(&mut self) -> Result<String, WyrdError> {
        let mut segments = vec![self.parse_key_segment()?];
        while self.matches(|tok| matches!(tok, Tok::Dot)) {
            segments.push(self.parse_key_segment()?);
        }
        Ok(segments.join("."))
    }

    fn parse_key_segment(&mut self) -> Result<String, WyrdError> {
        let Some(spanned) = self.advance().cloned() else {
            return Err(self.syntax_at_eof("key segment"));
        };
        match spanned.tok {
            Tok::Ident(value) | Tok::Str(value) => Ok(value),
            // Keywords are lexically identical to identifiers in the dotted-key
            // context (labels.in, annotations.not.exists, etc. are valid keys).
            Tok::And => Ok("and".to_owned()),
            Tok::Or => Ok("or".to_owned()),
            Tok::Not => Ok("not".to_owned()),
            Tok::In => Ok("in".to_owned()),
            Tok::Exists => Ok("exists".to_owned()),
            Tok::Bool(true) => Ok("true".to_owned()),
            Tok::Bool(false) => Ok("false".to_owned()),
            other => Err(syntax(
                "expected key segment after '.'",
                spanned.offset,
                "key segment",
                token_name(&other),
            )),
        }
    }

    fn parse_value(&mut self) -> Result<Value, WyrdError> {
        let Some(spanned) = self.advance().cloned() else {
            return Err(self.syntax_at_eof("value"));
        };
        match spanned.tok {
            Tok::Str(value) => Ok(Value::String(value)),
            Tok::Int(value) => Ok(Value::Int(value)),
            Tok::Float(value) => Ok(Value::Float(value)),
            Tok::Bool(value) => Ok(Value::Bool(value)),
            Tok::Duration(value) => Ok(Value::Duration(value)),
            other => Err(syntax(
                "expected value",
                spanned.offset,
                "value",
                token_name(&other),
            )),
        }
    }

    fn parse_list(&mut self) -> Result<Vec<Value>, WyrdError> {
        self.expect_tok(|tok| matches!(tok, Tok::LBracket), "[")?;
        let mut values = vec![self.parse_value()?];
        while self.matches(|tok| matches!(tok, Tok::Comma)) {
            values.push(self.parse_value()?);
        }
        self.expect_tok(|tok| matches!(tok, Tok::RBracket), "]")?;
        Ok(values)
    }

    fn expect_string(&mut self) -> Result<String, WyrdError> {
        let Some(spanned) = self.advance().cloned() else {
            return Err(self.syntax_at_eof("string"));
        };
        match spanned.tok {
            Tok::Str(value) => Ok(value),
            other => Err(syntax(
                "expected string",
                spanned.offset,
                "string",
                token_name(&other),
            )),
        }
    }

    fn expect_tok(
        &mut self,
        pred: impl FnOnce(&Tok) -> bool,
        expected: &'static str,
    ) -> Result<(), WyrdError> {
        let Some(spanned) = self.peek() else {
            return Err(self.syntax_at_eof(expected));
        };
        if pred(&spanned.tok) {
            self.cursor += 1;
            Ok(())
        } else {
            Err(syntax(
                format!("expected {expected}"),
                spanned.offset,
                expected,
                token_name(&spanned.tok),
            ))
        }
    }

    fn expect_eof(&self) -> Result<(), WyrdError> {
        if let Some(tok) = self.peek() {
            Err(syntax(
                "unexpected trailing input",
                tok.offset,
                "EOF",
                token_name(&tok.tok),
            ))
        } else {
            Ok(())
        }
    }

    fn matches(&mut self, pred: impl FnOnce(&Tok) -> bool) -> bool {
        if self.peek().is_some_and(|spanned| pred(&spanned.tok)) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn advance(&mut self) -> Option<&Spanned> {
        let out = self.tokens.get(self.cursor);
        if out.is_some() {
            self.cursor += 1;
        }
        out
    }

    fn peek(&self) -> Option<&Spanned> {
        self.tokens.get(self.cursor)
    }

    fn error_here(&self, expected: &'static str) -> WyrdError {
        self.peek().map_or_else(
            || self.syntax_at_eof(expected),
            |spanned| {
                syntax(
                    format!("expected {expected}"),
                    spanned.offset,
                    expected,
                    token_name(&spanned.tok),
                )
            },
        )
    }

    fn syntax_at_eof(&self, expected: &'static str) -> WyrdError {
        syntax(
            format!("expected {expected} at end of input"),
            self.input_len,
            expected,
            "<eof>",
        )
    }
}

fn syntax(
    message: impl Into<String>,
    offset: usize,
    expected: impl Into<String>,
    found: impl Into<String>,
) -> WyrdError {
    WyrdError::query_invalid_syntax(message, offset, expected, found)
}

fn token_name(tok: &Tok) -> String {
    match tok {
        Tok::LParen => "(".to_owned(),
        Tok::RParen => ")".to_owned(),
        Tok::LBracket => "[".to_owned(),
        Tok::RBracket => "]".to_owned(),
        Tok::Comma => ",".to_owned(),
        Tok::Dot => ".".to_owned(),
        Tok::Eq => "=".to_owned(),
        Tok::Ne => "!=".to_owned(),
        Tok::Gt => ">".to_owned(),
        Tok::Ge => ">=".to_owned(),
        Tok::Lt => "<".to_owned(),
        Tok::Le => "<=".to_owned(),
        Tok::Match => "=~".to_owned(),
        Tok::NotMatch => "!~".to_owned(),
        Tok::And => "and".to_owned(),
        Tok::Or => "or".to_owned(),
        Tok::Not => "not".to_owned(),
        Tok::In => "in".to_owned(),
        Tok::Exists => "exists".to_owned(),
        Tok::Ident(value) => value.clone(),
        Tok::Str(value) => format!("\"{}\"", escape_string(value)),
        Tok::Int(value) => value.to_string(),
        Tok::Float(value) => value.to_string(),
        Tok::Duration(value) => format!("{value}ns"),
        Tok::Bool(value) => value.to_string(),
    }
}

pub(crate) fn escape_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::query::{FieldRef, MAX_DEPTH, MetadataQuery, Operator, Value};

    use super::MAX_QUERY_INPUT_LEN;

    #[test]
    fn rejects_input_exceeding_max_len() {
        let long = format!("a = \"{}\"", "x".repeat(MAX_QUERY_INPUT_LEN));
        let err = MetadataQuery::parse(&long).expect_err("over-length query is rejected");
        assert_eq!(err.code(), "WYRD_QUERY_400_TOO_COMPLEX");
        let details = err.as_problem_json()["details"].clone();
        assert_eq!(details["cap"], "input_length");
    }

    #[test]
    fn rejects_paren_nesting_beyond_max_depth() {
        let open = "(".repeat(MAX_DEPTH + 1);
        let close = ")".repeat(MAX_DEPTH + 1);
        let input = format!("{open}a = \"x\"{close}");
        let err = MetadataQuery::parse(&input).expect_err("over-deep paren nesting is rejected");
        assert_eq!(err.code(), "WYRD_QUERY_400_TOO_COMPLEX");
        let details = err.as_problem_json()["details"].clone();
        assert_eq!(details["cap"], "depth");
    }

    #[test]
    fn rejects_chained_not_beyond_max_depth() {
        let nots = "not ".repeat(MAX_DEPTH + 1);
        let input = format!("{nots}a = \"x\"");
        let err = MetadataQuery::parse(&input).expect_err("over-deep not chain is rejected");
        assert_eq!(err.code(), "WYRD_QUERY_400_TOO_COMPLEX");
        let details = err.as_problem_json()["details"].clone();
        assert_eq!(details["cap"], "depth");
    }

    #[test]
    fn parses_label_equality() {
        let query = MetadataQuery::parse("labels.env = \"prod\"").expect("query parses");
        let MetadataQuery::Predicate(predicate) = query else {
            panic!("expected predicate");
        };
        assert_eq!(predicate.field, FieldRef::Label("env".to_owned()));
        assert_eq!(predicate.op, Operator::Eq(Value::String("prod".to_owned())));
    }

    #[test]
    fn parses_precedence() {
        let query = MetadataQuery::parse("a = 1 and b = 2 or c = 3").expect("query parses");
        assert!(matches!(query, MetadataQuery::Or(_)));
    }

    #[test]
    fn parses_dotted_and_quoted_keys() {
        let query =
            MetadataQuery::parse("attributes.http.status_code = 500").expect("query parses");
        let MetadataQuery::Predicate(predicate) = query else {
            panic!("expected predicate");
        };
        assert_eq!(
            predicate.field,
            FieldRef::Attribute("http.status_code".to_owned())
        );

        let query = MetadataQuery::parse("labels.\"acme.com/team\" = \"x\"").expect("query parses");
        let MetadataQuery::Predicate(predicate) = query else {
            panic!("expected predicate");
        };
        assert_eq!(predicate.field, FieldRef::Label("acme.com/team".to_owned()));
    }

    #[test]
    fn invalid_double_equals_reports_second_equals() {
        let err = MetadataQuery::parse("labels.env == \"x\"").expect_err("query is invalid");
        let problem = err.as_problem_json();
        assert_eq!(problem["code"], "WYRD_QUERY_400_INVALID_SYNTAX");
        assert_eq!(problem["details"]["offset"], 12);
        assert_eq!(problem["details"]["expected"], "value");
        assert_eq!(problem["details"]["found"], "=");
    }
}
