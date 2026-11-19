//! JSON read into a tree and written back compactly, enough for talking to language servers.

use std::fmt;
use std::ops::Index;

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

#[cfg(test)]
mod tests {
    use super::Json;

    #[test]
    fn control_characters_are_escaped_when_written() {
        let s = Json::String("a\u{1}b\u{1f}\"\n".into());
        assert_eq!(s.to_string(), r#""a\u0001b\u001f\"\n""#);
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
