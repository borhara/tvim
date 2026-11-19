//! JSON read into a tree and written back compactly, enough for talking to language servers.

use std::fmt;
use std::ops::Index;

const MAX_DEPTH: usize = 256;
// 2^53: past here an f64 can't hold every integer
const MAX_EXACT: f64 = 9_007_199_254_740_992.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    // kept in order; objects are small enough that a scan beats hashing
    Object(Vec<(String, Json)>),
}

static NULL: Json = Json::Null;

impl Json {
    pub fn parse(text: &str) -> Result<Json, String> {
        let mut p = Parser {
            text,
            bytes: text.as_bytes(),
            i: 0,
            depth: 0,
        };
        let value = p.value()?;
        p.space();
        if p.i < p.bytes.len() {
            return Err(p.error("the end"));
        }
        Ok(value)
    }

    pub fn object<K: Into<String>>(fields: impl IntoIterator<Item = (K, Json)>) -> Json {
        Json::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    // a repeated key reads as its last value
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(fields) => fields.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        *self == Json::Null
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        self.as_f64()
            .filter(|n| n.fract() == 0.0 && n.abs() <= MAX_EXACT)
            .map(|n| n as i64)
    }

    pub fn as_u64(&self) -> Option<u64> {
        self.as_i64().and_then(|n| u64::try_from(n).ok())
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }
}

// missing keys and indexes give null, so lookups chain: msg["result"]["range"]["start"]
impl Index<&str> for Json {
    type Output = Json;

    fn index(&self, key: &str) -> &Json {
        self.get(key).unwrap_or(&NULL)
    }
}

impl Index<usize> for Json {
    type Output = Json;

    fn index(&self, i: usize) -> &Json {
        self.as_array().and_then(|items| items.get(i)).unwrap_or(&NULL)
    }
}

impl From<bool> for Json {
    fn from(b: bool) -> Json {
        Json::Bool(b)
    }
}

impl From<&str> for Json {
    fn from(s: &str) -> Json {
        Json::String(s.to_string())
    }
}

impl From<String> for Json {
    fn from(s: String) -> Json {
        Json::String(s)
    }
}

macro_rules! from_number {
    ($($t:ty),*) => {$(
        impl From<$t> for Json {
            fn from(n: $t) -> Json {
                Json::Number(n as f64)
            }
        }
    )*};
}

from_number!(i32, i64, u32, u64, usize, f64);

impl<T: Into<Json>> From<Option<T>> for Json {
    fn from(value: Option<T>) -> Json {
        value.map_or(Json::Null, Into::into)
    }
}

impl<T: Into<Json>> From<Vec<T>> for Json {
    fn from(items: Vec<T>) -> Json {
        Json::Array(items.into_iter().map(Into::into).collect())
    }
}

impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Json::Null => f.write_str("null"),
            Json::Bool(b) => write!(f, "{b}"),
            Json::Number(n) => number(f, *n),
            Json::String(s) => quoted(f, s),
            Json::Array(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            Json::Object(fields) => {
                f.write_str("{")?;
                for (i, (key, value)) in fields.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    quoted(f, key)?;
                    write!(f, ":{value}")?;
                }
                f.write_str("}")
            }
        }
    }
}

fn number(f: &mut fmt::Formatter, n: f64) -> fmt::Result {
    if !n.is_finite() {
        // JSON can't write infinity or NaN
        f.write_str("null")
    } else if n.fract() == 0.0 && n.abs() <= MAX_EXACT {
        write!(f, "{}", n as i64)
    } else {
        // the shortest form that reads back the same, with an exponent when that's shorter
        write!(f, "{n:?}")
    }
}

fn quoted(f: &mut fmt::Formatter, s: &str) -> fmt::Result {
    f.write_str("\"")?;
    let mut from = 0;
    // everything that needs escaping is ASCII, so each byte found is a whole character
    for (i, b) in s.bytes().enumerate() {
        let escape = match b {
            b'"' => Some("\\\""),
            b'\\' => Some("\\\\"),
            b'\n' => Some("\\n"),
            b'\r' => Some("\\r"),
            b'\t' => Some("\\t"),
            0..=0x1f => None,
            _ => continue,
        };
        f.write_str(&s[from..i])?;
        match escape {
            Some(e) => f.write_str(e)?,
            None => write!(f, "\\u{b:04x}")?,
        }
        from = i + 1;
    }
    f.write_str(&s[from..])?;
    f.write_str("\"")
}

struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    i: usize,
    depth: usize,
}

impl Parser<'_> {
    fn error(&self, expected: &str) -> String {
        if self.i < self.bytes.len() {
            format!("expected {expected} at byte {}", self.i)
        } else {
            format!("expected {expected} before the end")
        }
    }

    fn space(&mut self) {
        while matches!(self.bytes.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    // after any space, steps over `b` if it comes next
    fn eat(&mut self, b: u8) -> bool {
        self.space();
        self.skip(&[b])
    }

    // steps over one byte if it's any of these
    fn skip(&mut self, any: &[u8]) -> bool {
        let found = self.bytes.get(self.i).is_some_and(|b| any.contains(b));
        if found {
            self.i += 1;
        }
        found
    }

    fn digits(&mut self) -> usize {
        let start = self.i;
        while self.bytes.get(self.i).is_some_and(u8::is_ascii_digit) {
            self.i += 1;
        }
        self.i - start
    }

    fn value(&mut self) -> Result<Json, String> {
        self.space();
        match self.bytes.get(self.i) {
            Some(b'{') => self.nested(Parser::object),
            Some(b'[') => self.nested(Parser::array),
            Some(b'"') => self.string().map(Json::String),
            Some(b't') => self.word("true", Json::Bool(true)),
            Some(b'f') => self.word("false", Json::Bool(false)),
            Some(b'n') => self.word("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.error("a value")),
        }
    }

    // bounded, so a hostile message can't overflow the stack
    fn nested(&mut self, inside: fn(&mut Self) -> Result<Json, String>) -> Result<Json, String> {
        if self.depth == MAX_DEPTH {
            return Err(format!("nested more than {MAX_DEPTH} deep at byte {}", self.i));
        }
        self.depth += 1;
        self.i += 1;
        let value = inside(self);
        self.depth -= 1;
        value
    }

    fn array(&mut self) -> Result<Json, String> {
        let mut items = Vec::new();
        if self.eat(b']') {
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.value()?);
            if self.eat(b']') {
                return Ok(Json::Array(items));
            }
            if !self.eat(b',') {
                return Err(self.error(", or ]"));
            }
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        let mut fields = Vec::new();
        if self.eat(b'}') {
            return Ok(Json::Object(fields));
        }
        loop {
            self.space();
            if self.bytes.get(self.i) != Some(&b'"') {
                return Err(self.error("a key"));
            }
            let key = self.string()?;
            if !self.eat(b':') {
                return Err(self.error(":"));
            }
            fields.push((key, self.value()?));
            if self.eat(b'}') {
                return Ok(Json::Object(fields));
            }
            if !self.eat(b',') {
                return Err(self.error(", or }"));
            }
        }
    }

    // from the opening quote
    fn string(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut out = String::new();
        loop {
            // plain text up to the next quote, backslash or control character
            let rest = &self.bytes[self.i..];
            let plain = rest
                .iter()
                .position(|&b| matches!(b, b'"' | b'\\' | 0..=0x1f))
                .unwrap_or(rest.len());
            out.push_str(&self.text[self.i..self.i + plain]);
            self.i += plain;
            match self.bytes.get(self.i) {
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    out.push(self.escape()?);
                }
                Some(_) => return Err(format!("control character in a string at byte {}", self.i)),
                None => return Err("unterminated string".into()),
            }
        }
    }

    // just after the backslash
    fn escape(&mut self) -> Result<char, String> {
        let c = match self.bytes.get(self.i) {
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'/') => '/',
            Some(b'b') => '\u{8}',
            Some(b'f') => '\u{c}',
            Some(b'n') => '\n',
            Some(b'r') => '\r',
            Some(b't') => '\t',
            _ => return Err(self.error("an escape")),
        };
        self.i += 1;
        Ok(c)
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.i;
        self.skip(b"-");
        // no leading zeros: 0 stands alone
        if !self.skip(b"0") && self.digits() == 0 {
            return Err(self.error("a digit"));
        }
        if self.skip(b".") && self.digits() == 0 {
            return Err(self.error("a digit"));
        }
        if self.skip(b"eE") {
            self.skip(b"+-");
            if self.digits() == 0 {
                return Err(self.error("a digit"));
            }
        }
        self.text[start..self.i]
            .parse()
            .map(Json::Number)
            .map_err(|_| format!("bad number at byte {start}"))
    }

    fn word(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if !self.bytes[self.i..].starts_with(word.as_bytes()) {
            return Err(self.error(word));
        }
        self.i += word.len();
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::Json;

    fn round_trip(text: &str) -> String {
        Json::parse(text).unwrap_or_else(|e| panic!("{text}: {e}")).to_string()
    }

    #[test]
    fn values_read_back_as_they_were_written() {
        for text in [
            "null",
            "true",
            "false",
            "0",
            "-12",
            "3.25",
            "1e-7",
            r#""""#,
            r#""tab\tquote\"slash\\""#,
            "[]",
            "{}",
            r#"[1,[2,[3]],{"a":null}]"#,
            r#"{"id":1,"method":"initialize","params":{"rootUri":"file:///a%20b"}}"#,
        ] {
            assert_eq!(round_trip(text), text);
        }
    }

    #[test]
    fn space_between_tokens_is_dropped() {
        assert_eq!(round_trip(" {\n\t\"a\" : [ 1 , 2 ] \r\n} "), r#"{"a":[1,2]}"#);
    }

    #[test]
    fn numbers() {
        assert_eq!(Json::parse("-0").unwrap().as_f64(), Some(0.0));
        assert_eq!(Json::parse("1.5E+3").unwrap().as_i64(), Some(1500));
        assert_eq!(Json::parse("2.5").unwrap().as_i64(), None, "not a whole number");
        assert_eq!(Json::parse("-1").unwrap().as_u64(), None);
        assert_eq!(Json::parse("1e300").unwrap().as_i64(), None, "too big to be exact");
        assert_eq!(round_trip("1e300"), "1e300");
        assert_eq!(round_trip("0.1"), "0.1");
        assert_eq!(round_trip("100.0"), "100");
        assert_eq!(Json::Number(f64::NAN).to_string(), "null");
        assert_eq!(Json::Number(f64::INFINITY).to_string(), "null");
    }

    #[test]
    fn control_characters_are_escaped_when_written() {
        let s = Json::String("a\u{1}b\u{1f}\"\n".into());
        assert_eq!(s.to_string(), r#""a\u0001b\u001f\"\n""#);
    }

    #[test]
    fn malformed_text_is_refused() {
        for text in [
            "",
            "   ",
            "nul",
            "truex",
            "01",
            "-",
            "1.",
            ".5",
            "1e",
            "+1",
            "[1,]",
            "[1 2]",
            "{\"a\" 1}",
            "{\"a\":1,}",
            "{a:1}",
            "{\"a\":1",
            "\"open",
            "\"bad \\x escape\"",
            "\"\\u12\"",
            "\"\\u+123\"",
            "\"line\nbreak\"",
            "[] []",
        ] {
            assert!(Json::parse(text).is_err(), "{text:?} should be refused");
        }
    }

    #[test]
    fn errors_say_what_was_expected_and_where() {
        assert_eq!(Json::parse("[1 2]").unwrap_err(), "expected , or ] at byte 3");
        assert_eq!(Json::parse("{\"a\":").unwrap_err(), "expected a value before the end");
    }

    #[test]
    fn nesting_is_bounded() {
        let ok = format!("{}{}", "[".repeat(256), "]".repeat(256));
        assert!(Json::parse(&ok).is_ok());
        let deep = format!("{}{}", "[".repeat(257), "]".repeat(257));
        assert!(Json::parse(&deep).unwrap_err().starts_with("nested more than 256 deep"));
        assert!(Json::parse(&"[".repeat(100_000)).is_err());
    }

    #[test]
    fn lookups_chain_through_missing_parts() {
        let msg = Json::parse(r#"{"result":{"items":[{"label":"one"}]},"id":7}"#).unwrap();
        assert_eq!(msg["result"]["items"][0]["label"].as_str(), Some("one"));
        assert_eq!(msg["id"].as_u64(), Some(7));
        assert!(msg["error"]["code"].is_null());
        assert!(msg["result"]["items"][5].is_null());
        assert!(msg["id"]["x"].is_null(), "a number has no keys");
        assert_eq!(msg.get("error"), None);
    }

    #[test]
    fn a_repeated_key_reads_as_its_last_value() {
        let obj = Json::parse(r#"{"a":1,"a":2}"#).unwrap();
        assert_eq!(obj["a"].as_i64(), Some(2));
    }

    #[test]
    fn building_messages() {
        let msg = Json::object([
            ("jsonrpc", "2.0".into()),
            ("id", 1.into()),
            (
                "params",
                Json::object([("text", Json::from("é\n")), ("version", None::<i32>.into())]),
            ),
            ("list", vec![1u32, 2].into()),
            ("ok", true.into()),
        ]);
        assert_eq!(
            msg.to_string(),
            r#"{"jsonrpc":"2.0","id":1,"params":{"text":"é\n","version":null},"list":[1,2],"ok":true}"#
        );
    }
}
