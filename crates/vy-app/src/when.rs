//! Expressions used by `when` in key bindings.
//!
//! The grammar is small on purpose: identifiers, string and number literals,
//! `==`, `!=`, `&&`, `||`, `!`, and parentheses. Unknown identifiers are false at
//! evaluation time so `keymap.setContext` can add names later.

/// A value visible to a `when` expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Text(String),
    Number(f64),
}

impl Value {
    /// Truthiness: true booleans, non-empty strings, and non-zero numbers.
    pub fn truthy(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            Self::Text(text) => !text.is_empty(),
            Self::Number(number) => *number != 0.0,
        }
    }
}

/// A parsed condition.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Ident(String),
    Text(String),
    Number(f64),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),
}

impl Expr {
    /// Evaluates the condition. Identifiers are looked up by name.
    pub fn eval(&self, lookup: &dyn Fn(&str) -> Option<Value>) -> bool {
        match self {
            Self::Ident(name) => lookup(name).is_some_and(|value| value.truthy()),
            Self::Text(_) | Self::Number(_) => false,
            Self::Not(inner) => !inner.eval(lookup),
            Self::And(left, right) => left.eval(lookup) && right.eval(lookup),
            Self::Or(left, right) => left.eval(lookup) || right.eval(lookup),
            Self::Eq(left, right) => value_of(left, lookup) == value_of(right, lookup),
            Self::Ne(left, right) => value_of(left, lookup) != value_of(right, lookup),
        }
    }
}

fn value_of(expr: &Expr, lookup: &dyn Fn(&str) -> Option<Value>) -> Option<Value> {
    match expr {
        Expr::Ident(name) => lookup(name),
        Expr::Text(text) => Some(Value::Text(text.clone())),
        Expr::Number(number) => Some(Value::Number(*number)),
        _ => None,
    }
}

/// Parses a condition. Empty input is not allowed; omit `when` instead.
pub fn parse(source: &str) -> Result<Expr, ParseError> {
    let mut parser = Parser {
        tokens: tokenize(source)?,
        index: 0,
    };
    let expr = parser.parse_or()?;
    if parser.index != parser.tokens.len() {
        return Err(ParseError::new("unexpected trailing input"));
    }
    Ok(expr)
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ParseError {
    message: String,
}

impl ParseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Text(String),
    Number(f64),
    EqEq,
    BangEq,
    AndAnd,
    OrOr,
    Bang,
    Left,
    Right,
}

fn tokenize(source: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut characters = source.chars().peekable();

    while let Some(character) = characters.next() {
        match character {
            ' ' | '\t' | '\n' | '\r' => {}
            '(' => tokens.push(Token::Left),
            ')' => tokens.push(Token::Right),
            '!' => {
                if characters.peek() == Some(&'=') {
                    characters.next();
                    tokens.push(Token::BangEq);
                } else {
                    tokens.push(Token::Bang);
                }
            }
            '=' => {
                if characters.peek() == Some(&'=') {
                    characters.next();
                    tokens.push(Token::EqEq);
                } else {
                    return Err(ParseError::new("expected `==`"));
                }
            }
            '&' => {
                if characters.peek() == Some(&'&') {
                    characters.next();
                    tokens.push(Token::AndAnd);
                } else {
                    return Err(ParseError::new("expected `&&`"));
                }
            }
            '|' => {
                if characters.peek() == Some(&'|') {
                    characters.next();
                    tokens.push(Token::OrOr);
                } else {
                    return Err(ParseError::new("expected `||`"));
                }
            }
            '\'' | '"' => {
                let quote = character;
                let mut text = String::new();
                loop {
                    match characters.next() {
                        Some(character) if character == quote => break,
                        Some(character) => text.push(character),
                        None => return Err(ParseError::new("unterminated string")),
                    }
                }
                tokens.push(Token::Text(text));
            }
            character if character.is_ascii_digit() => {
                let mut number = String::from(character);
                while let Some(next) = characters.peek()
                    && (next.is_ascii_digit() || *next == '.')
                {
                    number.push(*next);
                    characters.next();
                }
                let value = number
                    .parse::<f64>()
                    .map_err(|_| ParseError::new(format!("invalid number: {number}")))?;
                tokens.push(Token::Number(value));
            }
            character if character.is_alphanumeric() || character == '_' || character == '.' => {
                let mut name = String::from(character);
                while let Some(next) = characters.peek()
                    && (next.is_alphanumeric() || *next == '_' || *next == '.' || *next == '-')
                {
                    name.push(*next);
                    characters.next();
                }
                tokens.push(Token::Ident(name));
            }
            other => {
                return Err(ParseError::new(format!("unexpected character: {other}")));
            }
        }
    }

    if tokens.is_empty() {
        return Err(ParseError::new("empty expression"));
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.index).cloned();
        if token.is_some() {
            self.index += 1;
        }
        token
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while self.peek() == Some(&Token::OrOr) {
            self.next();
            let right = self.parse_and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality()?;
        while self.peek() == Some(&Token::AndAnd) {
            self.next();
            let right = self.parse_equality()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        loop {
            let operator = match self.peek() {
                Some(Token::EqEq) => Token::EqEq,
                Some(Token::BangEq) => Token::BangEq,
                _ => break,
            };
            self.next();
            let right = self.parse_unary()?;
            left = match operator {
                Token::EqEq => Expr::Eq(Box::new(left), Box::new(right)),
                _ => Expr::Ne(Box::new(left), Box::new(right)),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if self.peek() == Some(&Token::Bang) {
            self.next();
            return Ok(Expr::Not(Box::new(self.parse_unary()?)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.next() {
            Some(Token::Ident(name)) => Ok(Expr::Ident(name)),
            Some(Token::Text(text)) => Ok(Expr::Text(text)),
            Some(Token::Number(number)) => Ok(Expr::Number(number)),
            Some(Token::Left) => {
                let expr = self.parse_or()?;
                if self.next() != Some(Token::Right) {
                    return Err(ParseError::new("expected `)`"));
                }
                Ok(expr)
            }
            _ => Err(ParseError::new("expected a condition")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn context() -> HashMap<String, Value> {
        HashMap::from([
            ("editorFocus".into(), Value::Bool(true)),
            ("mode".into(), Value::Text("normal".into())),
            ("hasSelection".into(), Value::Bool(false)),
            ("count".into(), Value::Number(3.0)),
        ])
    }

    fn eval(source: &str) -> bool {
        let context = context();
        parse(source)
            .expect("parse")
            .eval(&|name| context.get(name).cloned())
    }

    #[test]
    fn evaluates_identifiers_and_strings() {
        assert!(eval("editorFocus"));
        assert!(!eval("hasSelection"));
        assert!(!eval("unknownName"));
        assert!(eval("mode == 'normal'"));
        assert!(eval("mode != \"insert\""));
        assert!(eval("count == 3"));
        assert!(eval("count != 4"));
    }

    #[test]
    fn combines_conditions() {
        assert!(eval("editorFocus && mode == 'normal'"));
        assert!(!eval("editorFocus && hasSelection"));
        assert!(eval("hasSelection || mode == 'normal'"));
        assert!(eval("!(mode == 'insert')"));
        assert!(eval("(editorFocus || hasSelection) && !hasSelection"));
    }

    #[test]
    fn rejects_broken_expressions() {
        assert!(parse("").is_err());
        assert!(parse("mode =").is_err());
        assert!(parse("(mode == 'normal'").is_err());
        assert!(parse("mode == 'normal' extra").is_err());
        assert!(parse("mode & editorFocus").is_err());
    }
}
