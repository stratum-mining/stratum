use std::collections::HashMap;


#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Number {
    I64(i64),
    F64(f64)
}

#[derive(Debug, PartialEq, Clone)]
pub enum Value {
    String(String),
    Number(Number),
    Boolean(bool),
    Array(Vec<Value>),
    Object(HashMap<String, Value>),
    Null
}

impl Value {
    pub fn to_json_string(&self) -> String {
        match self {
            Value::String(s) => format!("\"{}\"", s), // JSON strings are wrapped in quotes
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
        }
    }

    pub fn to_json_bytes(&self) -> Vec<u8> {
        self.to_json_string().into_bytes()
    }
}



impl TryFrom<&Value> for String {
    type Error = ();

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::String(value) => Ok(value.clone()),
            _ => Err(())
        }
    }
}

impl TryFrom<&Value> for i64 {
    type Error = ();

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::Number(value) => match value {
                Number::I64(value) => Ok(*value),
                Number::F64(value) => Ok(*value as i64)
            },
            _ => Err(())
        }
    }
}

impl TryFrom<&Value> for f64 {
    type Error = ();

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::Number(value) => match value {
                Number::I64(value) => Ok(*value as f64),
                Number::F64(value) => Ok(*value)
            },
            _ => Err(())
        }
    }
}


impl TryFrom<&Value> for bool {
    type Error = ();

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::Boolean(value) => Ok(*value),
            _ => Err(())
        }
    }
}


impl<'a> TryFrom<&'a Value> for &'a Vec<Value> {
    type Error = ();

    fn try_from(value: &'a Value) -> Result<Self, Self::Error> {
        match value {
            Value::Array(value) => Ok(value),
            _ => Err(())
        }
    }
}

impl<'a> TryFrom<&'a Value> for &'a HashMap<String, Value> {
    type Error = ();

    fn try_from(value: &'a Value) -> Result<Self, Self::Error> {
        match value {
            Value::Object(value) => Ok(value),
            _ => Err(())
        }
    }
}

