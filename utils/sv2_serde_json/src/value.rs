use std::collections::HashMap;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Number {
    I64(i64),
    F64(f64),
}

#[derive(Debug, PartialEq, Clone)]
pub enum Value {
    String(String),
    Number(Number),
    Boolean(bool),
    Array(Vec<Value>),
    Object(HashMap<String, Value>),
    Null,
    None, 
    Some(Box<Value>),
}

impl Value {
    pub fn to_json_string(&self) -> String {
        match self {
            Value::String(s) => format!("\"{}\"", s),
            Value::Number(Number::I64(i)) => i.to_string(),
            Value::Number(Number::F64(f)) => f.to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Array(arr) => {
                let elements: Vec<String> = arr.iter().map(|v| v.to_json_string()).collect();
                format!("[{}]", elements.join(","))
            }
            Value::Object(obj) => {
                let members: Vec<String> = obj
                    .iter()
                    .map(|(k, v)| format!("\"{}\":{}", k, v.to_json_string()))
                    .collect();
                format!("{{{}}}", members.join(","))
            }
            Value::Null => "null".to_string(),
            Value::None => {
                // Still dicy on this
                "\n".to_string()
            }
            Value::Some(x) => x.to_json_string(),
        }
    }

    pub fn to_json_bytes(&self) -> Vec<u8> {
        self.to_json_string().into_bytes()
    }
}

pub trait ToJsonValue {
    fn to_json_value(&self) -> Value;
}

pub trait FromJsonValue: Sized {
    fn from_json_value(value: &Value) -> Result<Self, ()>;
}


impl ToJsonValue for String {
    fn to_json_value(&self) -> Value {
        Value::String(self.clone())
    }
}

impl FromJsonValue for String {
    fn from_json_value(value: &Value) -> Result<Self, ()> {
        match value {
            Value::String(s) => Ok(s.clone()),
            _ => Err(()),
        }
    }
}

impl ToJsonValue for i64 {
    fn to_json_value(&self) -> Value {
        Value::Number(Number::I64(*self))
    }
}

impl FromJsonValue for i64 {
    fn from_json_value(value: &Value) -> Result<Self, ()> {
        match value {
            Value::Number(Number::I64(i)) => Ok(*i),
            Value::Number(Number::F64(f)) => Ok(*f as i64),
            _ => Err(()),
        }
    }
}

impl ToJsonValue for u64 {
    fn to_json_value(&self) -> Value {
        Value::Number(Number::I64(*self as i64))
    }
}

impl FromJsonValue for u64 {
    fn from_json_value(value: &Value) -> Result<Self, ()> {
        match value {
            Value::Number(Number::I64(i)) => Ok(*i as u64),
            Value::Number(Number::F64(f)) => Ok(*f as u64),
            _ => Err(()),
        }
    }
}

impl ToJsonValue for i32 {
    fn to_json_value(&self) -> Value {
        Value::Number(Number::I64(*self as i64))
    }
}

impl FromJsonValue for i32 {
    fn from_json_value(value: &Value) -> Result<Self, ()> {
        match value {
            Value::Number(Number::I64(i)) => Ok(*i as i32),
            Value::Number(Number::F64(f)) => Ok(*f as i32),
            _ => Err(()),
        }
    }
}

impl ToJsonValue for f64 {
    fn to_json_value(&self) -> Value {
        Value::Number(Number::F64(*self))
    }
}

impl FromJsonValue for f64 {
    fn from_json_value(value: &Value) -> Result<Self, ()> {
        match value {
            Value::Number(Number::F64(f)) => Ok(*f),
            Value::Number(Number::I64(i)) => Ok(*i as f64),
            _ => Err(()),
        }
    }
}

impl ToJsonValue for bool {
    fn to_json_value(&self) -> Value {
        Value::Boolean(*self)
    }
}

impl FromJsonValue for bool {
    fn from_json_value(value: &Value) -> Result<Self, ()> {
        match value {
            Value::Boolean(b) => Ok(*b),
            _ => Err(()),
        }
    }
}

impl<T> ToJsonValue for Option<T>
where
    T: ToJsonValue,
{
    fn to_json_value(&self) -> Value {
        match self {
            Some(value) => Value::Some(Box::new(value.to_json_value())),
            None => Value::None,
        }
    }
}

impl<T> FromJsonValue for Option<T>
where
    T: FromJsonValue,
{
    fn from_json_value(value: &Value) -> Result<Self, ()> {
        match value {
            Value::Some(boxed_value) => T::from_json_value(boxed_value).map(Some),
            Value::None => Ok(None),
            _ => Err(()),
        }
    }
}

impl<T> ToJsonValue for Vec<T>
where
    T: ToJsonValue,
{
    fn to_json_value(&self) -> Value {
        Value::Array(self.iter().map(|v| v.to_json_value()).collect())
    }
}

impl<T> FromJsonValue for Vec<T>
where
    T: FromJsonValue,
{
    fn from_json_value(value: &Value) -> Result<Self, ()> {
        match value {
            Value::Array(arr) => arr.iter().map(T::from_json_value).collect(),
            _ => Err(()),
        }
    }
}

impl<K, V> ToJsonValue for HashMap<K, V>
where
    K: ToString,
    V: ToJsonValue,
{
    fn to_json_value(&self) -> Value {
        Value::Object(
            self.iter()
                .map(|(k, v)| (k.to_string(), v.to_json_value()))
                .collect(),
        )
    }
}

impl<K, V> FromJsonValue for HashMap<K, V>
where
    K: std::str::FromStr + Eq + std::hash::Hash,
    V: FromJsonValue,
{
    fn from_json_value(value: &Value) -> Result<Self, ()> {
        match value {
            Value::Object(obj) => obj
                .iter()
                .map(|(k, v)| match (k.parse(), V::from_json_value(v)) {
                    (Ok(parsed_key), Ok(parsed_value)) => Ok((parsed_key, parsed_value)),
                    _ => Err(()),
                })
                .collect(),
            _ => Err(()),
        }
    }
}

impl ToJsonValue for Value {
    fn to_json_value(&self) -> Value {
        self.clone()
    }
}

impl FromJsonValue for Value {
    fn from_json_value(value: &Value) -> Result<Self, ()> {
        Ok(value.clone())
    }
}
