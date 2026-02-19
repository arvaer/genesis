use crate::ast::Expr;
use crate::effect::{CapabilityId, Eff, Effect};
use crate::value::Value;
use std::collections::HashMap;
use thiserror::Error;

/// Environment: variable bindings (immutable; clone-on-extend).
pub type Env = HashMap<String, Value>;

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("unbound symbol: {0}")]
    UnboundSymbol(String),
    #[error("not a function: {0}")]
    NotAFunction(String),
    #[error("bad special form: {0}")]
    BadForm(String),
    #[error("arity mismatch: expected {expected}, got {got}")]
    ArityMismatch { expected: usize, got: usize },
    #[error("type error: {0}")]
    TypeError(String),
}

/// Evaluate an expression in the given environment, producing Eff<Value>.
/// No side effects are executed — effects are recorded as data.
pub fn eval(expr: &Expr, env: &Env) -> Result<Eff<Value>, EvalError> {
    match expr {
        Expr::Number(n) => Ok(Eff::pure(Value::Number(*n))),
        Expr::Symbol(s) => {
            if s == "nil" {
                return Ok(Eff::pure(Value::Nil));
            }
            if s == "true" {
                return Ok(Eff::pure(Value::Bool(true)));
            }
            if s == "false" {
                return Ok(Eff::pure(Value::Bool(false)));
            }
            env.get(s)
                .cloned()
                .map(Eff::pure)
                .ok_or_else(|| EvalError::UnboundSymbol(s.clone()))
        }
        Expr::List(elems) => {
            if elems.is_empty() {
                return Ok(Eff::pure(Value::Nil));
            }
            // Check for special forms.
            if let Expr::Symbol(head) = &elems[0] {
                match head.as_str() {
                    "quote" => return eval_quote(elems),
                    "if" => return eval_if(elems, env),
                    "lambda" => return eval_lambda(elems, env),
                    "define" => return eval_define(elems, env),
                    "let" => return eval_let(elems, env),
                    "begin" => return eval_begin(elems, env),
                    "effect" => return eval_effect(elems, env),
                    _ => {}
                }
            }
            // Function application.
            eval_application(elems, env)
        }
    }
}

fn eval_quote(elems: &[Expr]) -> Result<Eff<Value>, EvalError> {
    if elems.len() != 2 {
        return Err(EvalError::BadForm(
            "quote requires exactly 1 argument".into(),
        ));
    }
    Ok(Eff::pure(expr_to_value(&elems[1])))
}

/// Convert an Expr to a Value (for quote).
fn expr_to_value(expr: &Expr) -> Value {
    match expr {
        Expr::Number(n) => Value::Number(*n),
        Expr::Symbol(s) => Value::Symbol(s.clone()),
        Expr::List(es) => Value::List(es.iter().map(expr_to_value).collect()),
    }
}

fn eval_if(elems: &[Expr], env: &Env) -> Result<Eff<Value>, EvalError> {
    if elems.len() != 4 {
        return Err(EvalError::BadForm("if requires exactly 3 arguments".into()));
    }
    let cond_eff = eval(&elems[1], env)?;
    let is_true = is_truthy(&cond_eff.value);
    if is_true {
        let branch = eval(&elems[2], env)?;
        Ok(Eff {
            value: branch.value,
            effects: concat_effects(cond_eff.effects, branch.effects),
        })
    } else {
        let branch = eval(&elems[3], env)?;
        Ok(Eff {
            value: branch.value,
            effects: concat_effects(cond_eff.effects, branch.effects),
        })
    }
}

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Nil => false,
        Value::Bool(b) => *b,
        Value::Number(0) => false,
        _ => true,
    }
}

fn eval_lambda(elems: &[Expr], env: &Env) -> Result<Eff<Value>, EvalError> {
    if elems.len() != 3 {
        return Err(EvalError::BadForm(
            "lambda requires (lambda (params...) body)".into(),
        ));
    }
    let params = match &elems[1] {
        Expr::List(ps) => {
            let mut names = Vec::new();
            for p in ps {
                match p {
                    Expr::Symbol(s) => names.push(s.clone()),
                    _ => return Err(EvalError::BadForm("lambda params must be symbols".into())),
                }
            }
            names
        }
        _ => return Err(EvalError::BadForm("lambda params must be a list".into())),
    };
    Ok(Eff::pure(Value::Lambda {
        params,
        body: Box::new(elems[2].clone()),
        env: env.clone(),
    }))
}

fn eval_define(elems: &[Expr], env: &Env) -> Result<Eff<Value>, EvalError> {
    // (define name expr) — evaluates expr, binds in environment.
    // In v0, define is only meaningful at top-level; we just return the value
    // and the caller (harness) captures the binding.
    if elems.len() != 3 {
        return Err(EvalError::BadForm(
            "define requires (define name expr)".into(),
        ));
    }
    match &elems[1] {
        Expr::Symbol(_) => eval(&elems[2], env),
        _ => Err(EvalError::BadForm("define name must be a symbol".into())),
    }
}

fn eval_let(elems: &[Expr], env: &Env) -> Result<Eff<Value>, EvalError> {
    // (let ((x 1) (y 2)) body)
    if elems.len() != 3 {
        return Err(EvalError::BadForm(
            "let requires (let ((bindings...)) body)".into(),
        ));
    }
    let bindings = match &elems[1] {
        Expr::List(bs) => bs,
        _ => return Err(EvalError::BadForm("let bindings must be a list".into())),
    };
    let mut new_env = env.clone();
    let mut all_effects = Vec::new();
    for b in bindings {
        match b {
            Expr::List(pair) if pair.len() == 2 => {
                let name = match &pair[0] {
                    Expr::Symbol(s) => s.clone(),
                    _ => {
                        return Err(EvalError::BadForm(
                            "let binding name must be a symbol".into(),
                        ))
                    }
                };
                let val_eff = eval(&pair[1], &new_env)?;
                all_effects.extend(val_eff.effects);
                new_env.insert(name, val_eff.value);
            }
            _ => return Err(EvalError::BadForm("let binding must be (name expr)".into())),
        }
    }
    let body_eff = eval(&elems[2], &new_env)?;
    all_effects.extend(body_eff.effects);
    Ok(Eff {
        value: body_eff.value,
        effects: all_effects,
    })
}

fn eval_begin(elems: &[Expr], env: &Env) -> Result<Eff<Value>, EvalError> {
    if elems.len() < 2 {
        return Ok(Eff::pure(Value::Nil));
    }
    let mut all_effects = Vec::new();
    let mut last_val = Value::Nil;
    let mut current_env = env.clone();
    for e in &elems[1..] {
        // Support define inside begin to build up an environment.
        if let Expr::List(inner) = e {
            if let Some(Expr::Symbol(s)) = inner.first() {
                if s == "define" && inner.len() == 3 {
                    if let Expr::Symbol(name) = &inner[1] {
                        let val_eff = eval(&inner[2], &current_env)?;
                        all_effects.extend(val_eff.effects);
                        current_env.insert(name.clone(), val_eff.value.clone());
                        last_val = val_eff.value;
                        continue;
                    }
                }
            }
        }
        let eff = eval(e, &current_env)?;
        all_effects.extend(eff.effects);
        last_val = eff.value;
    }
    Ok(Eff {
        value: last_val,
        effects: all_effects,
    })
}

fn eval_effect(elems: &[Expr], env: &Env) -> Result<Eff<Value>, EvalError> {
    // (effect cap op arg1 arg2 ...)
    // cap should evaluate to a Number (treated as CapabilityId)
    // op should evaluate to a Symbol or String
    // args are evaluated normally
    if elems.len() < 3 {
        return Err(EvalError::BadForm(
            "effect requires at least (effect cap op)".into(),
        ));
    }
    let mut all_effects = Vec::new();

    let cap_eff = eval(&elems[1], env)?;
    all_effects.extend(cap_eff.effects);
    let cap_id = match &cap_eff.value {
        Value::Number(n) => CapabilityId(*n as u64),
        _ => return Err(EvalError::TypeError("effect cap must be a number".into())),
    };

    // The op is taken as a literal symbol (not evaluated).
    let op = match &elems[2] {
        Expr::Symbol(s) => s.clone(),
        _ => {
            // If not a bare symbol, evaluate it.
            let op_eff = eval(&elems[2], env)?;
            all_effects.extend(op_eff.effects);
            match &op_eff.value {
                Value::Symbol(s) => s.clone(),
                Value::String(s) => s.clone(),
                _ => {
                    return Err(EvalError::TypeError(
                        "effect op must be a symbol or string".into(),
                    ))
                }
            }
        }
    };

    let mut args = Vec::new();
    for e in &elems[3..] {
        let arg_eff = eval(e, env)?;
        all_effects.extend(arg_eff.effects);
        args.push(arg_eff.value);
    }

    let effect = Effect { cap_id, op, args };
    all_effects.push(effect);
    Ok(Eff {
        value: Value::Nil,
        effects: all_effects,
    })
}

fn eval_application(elems: &[Expr], env: &Env) -> Result<Eff<Value>, EvalError> {
    // If the head is a symbol, try to resolve it — but fall through to builtins on failure.
    let head_is_builtin_sym = if let Expr::Symbol(name) = &elems[0] {
        is_builtin_fn(name)
    } else {
        false
    };

    if head_is_builtin_sym {
        // Evaluate arguments only, then dispatch to builtin.
        let mut all_effects = Vec::new();
        let mut arg_vals = Vec::new();
        for e in &elems[1..] {
            let arg_eff = eval(e, env)?;
            all_effects.extend(arg_eff.effects);
            arg_vals.push(arg_eff.value);
        }
        if let Expr::Symbol(name) = &elems[0] {
            let result = eval_builtin(name, &arg_vals)?;
            return Ok(Eff {
                value: result,
                effects: all_effects,
            });
        }
    }

    let func_eff = eval(&elems[0], env)?;
    let mut all_effects = func_eff.effects;

    let mut arg_vals = Vec::new();
    for e in &elems[1..] {
        let arg_eff = eval(e, env)?;
        all_effects.extend(arg_eff.effects);
        arg_vals.push(arg_eff.value);
    }

    match &func_eff.value {
        Value::Lambda {
            params,
            body,
            env: closure_env,
        } => {
            if params.len() != arg_vals.len() {
                return Err(EvalError::ArityMismatch {
                    expected: params.len(),
                    got: arg_vals.len(),
                });
            }
            let mut call_env = closure_env.clone();
            for (p, v) in params.iter().zip(arg_vals) {
                call_env.insert(p.clone(), v);
            }
            let body_eff = eval(body, &call_env)?;
            all_effects.extend(body_eff.effects);
            Ok(Eff {
                value: body_eff.value,
                effects: all_effects,
            })
        }
        _ => {
            // Try built-in by name if head was a symbol in the Expr.
            if let Expr::Symbol(name) = &elems[0] {
                let result = eval_builtin(name, &arg_vals)?;
                Ok(Eff {
                    value: result,
                    effects: all_effects,
                })
            } else {
                Err(EvalError::NotAFunction(format!("{}", func_eff.value)))
            }
        }
    }
}

fn is_builtin_fn(name: &str) -> bool {
    matches!(
        name,
        "+" | "-" | "*" | "=" | "<" | "list" | "car" | "cdr" | "cons" | "null?" | "not"
    )
}

fn eval_builtin(name: &str, args: &[Value]) -> Result<Value, EvalError> {
    match name {
        "+" => {
            check_arity(name, 2, args.len())?;
            let a = as_number(&args[0])?;
            let b = as_number(&args[1])?;
            Ok(Value::Number(a + b))
        }
        "-" => {
            check_arity(name, 2, args.len())?;
            let a = as_number(&args[0])?;
            let b = as_number(&args[1])?;
            Ok(Value::Number(a - b))
        }
        "*" => {
            check_arity(name, 2, args.len())?;
            let a = as_number(&args[0])?;
            let b = as_number(&args[1])?;
            Ok(Value::Number(a * b))
        }
        "=" => {
            check_arity(name, 2, args.len())?;
            let a = as_number(&args[0])?;
            let b = as_number(&args[1])?;
            Ok(Value::Bool(a == b))
        }
        "<" => {
            check_arity(name, 2, args.len())?;
            let a = as_number(&args[0])?;
            let b = as_number(&args[1])?;
            Ok(Value::Bool(a < b))
        }
        "list" => Ok(Value::List(args.to_vec())),
        "car" => {
            check_arity(name, 1, args.len())?;
            match &args[0] {
                Value::List(vs) if !vs.is_empty() => Ok(vs[0].clone()),
                _ => Err(EvalError::TypeError("car requires a non-empty list".into())),
            }
        }
        "cdr" => {
            check_arity(name, 1, args.len())?;
            match &args[0] {
                Value::List(vs) if !vs.is_empty() => Ok(Value::List(vs[1..].to_vec())),
                _ => Err(EvalError::TypeError("cdr requires a non-empty list".into())),
            }
        }
        "cons" => {
            check_arity(name, 2, args.len())?;
            match &args[1] {
                Value::List(vs) => {
                    let mut new = vec![args[0].clone()];
                    new.extend(vs.iter().cloned());
                    Ok(Value::List(new))
                }
                Value::Nil => Ok(Value::List(vec![args[0].clone()])),
                _ => Ok(Value::List(vec![args[0].clone(), args[1].clone()])),
            }
        }
        "null?" => {
            check_arity(name, 1, args.len())?;
            match &args[0] {
                Value::Nil => Ok(Value::Bool(true)),
                Value::List(vs) => Ok(Value::Bool(vs.is_empty())),
                _ => Ok(Value::Bool(false)),
            }
        }
        "not" => {
            check_arity(name, 1, args.len())?;
            Ok(Value::Bool(!is_truthy(&args[0])))
        }
        _ => Err(EvalError::UnboundSymbol(name.to_string())),
    }
}

fn check_arity(_name: &str, expected: usize, got: usize) -> Result<(), EvalError> {
    if expected != got {
        Err(EvalError::ArityMismatch { expected, got })
    } else {
        Ok(())
    }
}

fn as_number(v: &Value) -> Result<i64, EvalError> {
    match v {
        Value::Number(n) => Ok(*n),
        _ => Err(EvalError::TypeError(format!("expected number, got {v}"))),
    }
}

fn concat_effects(mut a: Vec<Effect>, b: Vec<Effect>) -> Vec<Effect> {
    a.extend(b);
    a
}

/// Evaluate a program (sequence of expressions) building up an environment.
/// Returns the final Eff<Value> with all accumulated effects.
pub fn eval_program(exprs: &[Expr]) -> Result<Eff<Value>, EvalError> {
    let mut env = Env::new();
    let mut all_effects = Vec::new();
    let mut last_val = Value::Nil;

    for expr in exprs {
        // Handle top-level define specially.
        if let Expr::List(elems) = expr {
            if let Some(Expr::Symbol(s)) = elems.first() {
                if s == "define" && elems.len() == 3 {
                    if let Expr::Symbol(name) = &elems[1] {
                        let val_eff = eval(&elems[2], &env)?;
                        all_effects.extend(val_eff.effects);
                        env.insert(name.clone(), val_eff.value.clone());
                        last_val = val_eff.value;
                        continue;
                    }
                }
            }
        }
        let eff = eval(expr, &env)?;
        all_effects.extend(eff.effects);
        last_val = eff.value;
    }

    Ok(Eff {
        value: last_val,
        effects: all_effects,
    })
}

/// Evaluate a single expression with a provided environment.
/// Used by the graph engine to evaluate function bodies in context.
pub fn eval_with_env(expr: &Expr, env: &Env) -> Result<Eff<Value>, EvalError> {
    eval(expr, env)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    #[test]
    fn eval_number() {
        let expr = parse("42").unwrap();
        let result = eval(&expr, &Env::new()).unwrap();
        assert_eq!(result.value, Value::Number(42));
        assert!(result.effects.is_empty());
    }

    #[test]
    fn eval_arithmetic() {
        let expr = parse("(+ 1 2)").unwrap();
        let result = eval(&expr, &Env::new()).unwrap();
        assert_eq!(result.value, Value::Number(3));
    }

    #[test]
    fn eval_nested_arithmetic() {
        let expr = parse("(* (+ 2 3) (- 10 4))").unwrap();
        let result = eval(&expr, &Env::new()).unwrap();
        assert_eq!(result.value, Value::Number(30));
    }

    #[test]
    fn eval_if_true() {
        let expr = parse("(if (< 1 2) 10 20)").unwrap();
        let result = eval(&expr, &Env::new()).unwrap();
        assert_eq!(result.value, Value::Number(10));
    }

    #[test]
    fn eval_if_false() {
        let expr = parse("(if (< 2 1) 10 20)").unwrap();
        let result = eval(&expr, &Env::new()).unwrap();
        assert_eq!(result.value, Value::Number(20));
    }

    #[test]
    fn eval_lambda_application() {
        let expr = parse("((lambda (x y) (+ x y)) 3 4)").unwrap();
        let result = eval(&expr, &Env::new()).unwrap();
        assert_eq!(result.value, Value::Number(7));
    }

    #[test]
    fn eval_effect_form() {
        let expr = parse("(effect 0 write 42)").unwrap();
        let result = eval(&expr, &Env::new()).unwrap();
        assert_eq!(result.value, Value::Nil);
        assert_eq!(result.effects.len(), 1);
        assert_eq!(result.effects[0].cap_id, CapabilityId(0));
        assert_eq!(result.effects[0].op, "write");
        assert_eq!(result.effects[0].args, vec![Value::Number(42)]);
    }

    #[test]
    fn eval_deterministic() {
        let expr = parse("(+ (* 3 4) (- 10 5))").unwrap();
        let r1 = eval(&expr, &Env::new()).unwrap();
        let r2 = eval(&expr, &Env::new()).unwrap();
        assert_eq!(r1, r2);
    }
}
