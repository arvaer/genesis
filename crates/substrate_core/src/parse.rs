use crate::ast::Expr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("unexpected closing paren")]
    UnexpectedCloseParen,
    #[error("trailing input after expression")]
    TrailingInput,
}

/// Tokenize input into a flat list of tokens.
fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            ';' => {
                // Line comment: skip to end of line.
                while let Some(&ch) = chars.peek() {
                    chars.next();
                    if ch == '\n' {
                        break;
                    }
                }
            }
            '(' => {
                tokens.push("(".to_string());
                chars.next();
            }
            ')' => {
                tokens.push(")".to_string());
                chars.next();
            }
            '\'' => {
                tokens.push("'".to_string());
                chars.next();
            }
            '"' => {
                chars.next();
                let mut s = String::new();
                s.push('"');
                while let Some(&ch) = chars.peek() {
                    chars.next();
                    if ch == '"' {
                        s.push('"');
                        break;
                    }
                    s.push(ch);
                }
                tokens.push(s);
            }
            _ => {
                let mut tok = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' || ch == '(' || ch == ')'
                    {
                        break;
                    }
                    tok.push(ch);
                    chars.next();
                }
                tokens.push(tok);
            }
        }
    }
    tokens
}

fn parse_tokens(tokens: &[String], pos: &mut usize) -> Result<Expr, ParseError> {
    if *pos >= tokens.len() {
        return Err(ParseError::UnexpectedEof);
    }
    let tok = &tokens[*pos];
    if tok == "(" {
        *pos += 1;
        let mut elems = Vec::new();
        loop {
            if *pos >= tokens.len() {
                return Err(ParseError::UnexpectedEof);
            }
            if tokens[*pos] == ")" {
                *pos += 1;
                return Ok(Expr::List(elems));
            }
            elems.push(parse_tokens(tokens, pos)?);
        }
    } else if tok == ")" {
        Err(ParseError::UnexpectedCloseParen)
    } else if tok == "'" {
        // Sugar: 'x => (quote x)
        *pos += 1;
        let inner = parse_tokens(tokens, pos)?;
        Ok(Expr::List(vec![Expr::Symbol("quote".to_string()), inner]))
    } else if tok.starts_with('"') && tok.ends_with('"') && tok.len() >= 2 {
        *pos += 1;
        let content = tok[1..tok.len() - 1].to_string();
        // Represent string literals as (quote <symbol>) for simplicity in v0
        // Actually, store them as a special symbol prefixed to distinguish.
        // For v0 we keep it simple: strings become Symbol with the content.
        Ok(Expr::List(vec![
            Expr::Symbol("quote".to_string()),
            Expr::Symbol(content),
        ]))
    } else if let Ok(n) = tok.parse::<i64>() {
        *pos += 1;
        Ok(Expr::Number(n))
    } else {
        *pos += 1;
        Ok(Expr::Symbol(tok.clone()))
    }
}

/// Parse a single s-expression from a string.
pub fn parse(input: &str) -> Result<Expr, ParseError> {
    let tokens = tokenize(input);
    if tokens.is_empty() {
        return Err(ParseError::UnexpectedEof);
    }
    let mut pos = 0;
    let expr = parse_tokens(&tokens, &mut pos)?;
    if pos < tokens.len() {
        return Err(ParseError::TrailingInput);
    }
    Ok(expr)
}

/// Parse multiple top-level expressions.
pub fn parse_many(input: &str) -> Result<Vec<Expr>, ParseError> {
    let tokens = tokenize(input);
    let mut pos = 0;
    let mut exprs = Vec::new();
    while pos < tokens.len() {
        exprs.push(parse_tokens(&tokens, &mut pos)?);
    }
    Ok(exprs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_number() {
        assert_eq!(parse("42").unwrap(), Expr::Number(42));
    }

    #[test]
    fn parse_symbol() {
        assert_eq!(parse("foo").unwrap(), Expr::Symbol("foo".to_string()));
    }

    #[test]
    fn parse_list() {
        let expr = parse("(+ 1 2)").unwrap();
        assert_eq!(
            expr,
            Expr::List(vec![
                Expr::Symbol("+".to_string()),
                Expr::Number(1),
                Expr::Number(2),
            ])
        );
    }

    #[test]
    fn parse_nested() {
        let expr = parse("(if (< x 0) (- 0 x) x)").unwrap();
        assert!(matches!(expr, Expr::List(_)));
    }

    #[test]
    fn roundtrip() {
        let input = "(define add (lambda (a b) (+ a b)))";
        let expr = parse(input).unwrap();
        let printed = expr.to_string();
        let reparsed = parse(&printed).unwrap();
        assert_eq!(expr, reparsed);
    }
}
