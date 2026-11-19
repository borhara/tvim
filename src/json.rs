//! JSON read into a tree and written back compactly, enough for talking to language servers.

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
