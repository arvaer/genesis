use serde::{Deserialize, Serialize};
use std::fmt;

/// Runtime value produced by evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Value {
    Number(i64),
    Symbol(String),
    List(Vec<Value>),
    Nil,
    Bool(bool),
    String(String),
    /// Lambda closure captured at eval time (not serializable across boundaries,
    /// but kept here for in-process usage).
    #[serde(skip)]
    Lambda {
        params: Vec<String>,
        body: Box<crate::ast::Expr>,
        env: crate::eval::Env,
    },
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Number(n) => write!(f, "{n}"),
            Value::Symbol(s) => write!(f, "{s}"),
            Value::List(vs) => {
                write!(f, "(")?;
                for (i, v) in vs.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, ")")
            }
            Value::Nil => write!(f, "nil"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::String(s) => write!(f, "\"{s}\""),
            Value::Lambda { .. } => write!(f, "#<lambda>"),
        }
    }
}
