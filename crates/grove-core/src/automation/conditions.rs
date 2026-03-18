use std::collections::HashMap;

use crate::errors::{GroveError, GroveResult};

/// Evaluate a step condition expression against current step states.
///
/// Supported syntax:
/// - `steps.<step_key>.state == 'value'`
/// - `steps.<step_key>.state != 'value'`
/// - `expr && expr`
/// - `expr || expr`
/// - `(expr)`
/// - Empty/whitespace → true (no condition means always run)
pub fn evaluate_condition(expr: &str, step_states: &HashMap<String, String>) -> GroveResult<bool> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return Ok(true);
    }
    let tokens = tokenize(trimmed)?;
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
        step_states,
    };
    let result = parser.parse_or()?;
    if parser.pos < parser.tokens.len() {
        return Err(GroveError::Runtime(format!(
            "unexpected token at position {}: {:?}",
            parser.pos, parser.tokens[parser.pos]
        )));
    }
    Ok(result)
}

// ── Token ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    /// Reference to `steps.<key>.state` — stores the step key.
    StepRef(String),
    /// A single-quoted string literal.
    StringLit(String),
    /// `==`
    Eq,
    /// `!=`
    Neq,
    /// `&&`
    And,
    /// `||`
    Or,
    /// `(`
    LParen,
    /// `)`
    RParen,
}

// ── Tokenizer ────────────────────────────────────────────────────────────────

fn tokenize(input: &str) -> GroveResult<Vec<Token>> {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < len {
        // Skip whitespace.
        if chars[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }

        // Two-character operators.
        if i + 1 < len {
            let two = (chars[i], chars[i + 1]);
            match two {
                ('=', '=') => {
                    tokens.push(Token::Eq);
                    i += 2;
                    continue;
                }
                ('!', '=') => {
                    tokens.push(Token::Neq);
                    i += 2;
                    continue;
                }
                ('&', '&') => {
                    tokens.push(Token::And);
                    i += 2;
                    continue;
                }
                ('|', '|') => {
                    tokens.push(Token::Or);
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }

        // Parentheses.
        if chars[i] == '(' {
            tokens.push(Token::LParen);
            i += 1;
            continue;
        }
        if chars[i] == ')' {
            tokens.push(Token::RParen);
            i += 1;
            continue;
        }

        // Single-quoted string literal.
        if chars[i] == '\'' {
            i += 1; // consume opening quote
            let start = i;
            while i < len && chars[i] != '\'' {
                i += 1;
            }
            if i >= len {
                return Err(GroveError::Runtime(
                    "unterminated string literal in condition expression".to_string(),
                ));
            }
            let value: String = chars[start..i].iter().collect();
            tokens.push(Token::StringLit(value));
            i += 1; // consume closing quote
            continue;
        }

        // `steps.<key>.state` reference.
        if input[i..].starts_with("steps.") {
            i += 6; // skip "steps."
            let key_start = i;
            while i < len && chars[i] != '.' && !chars[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= len || chars[i] != '.' {
                return Err(GroveError::Runtime(format!(
                    "expected '.state' after step key at position {}",
                    i
                )));
            }
            let key: String = chars[key_start..i].iter().collect();
            if key.is_empty() {
                return Err(GroveError::Runtime(
                    "empty step key in 'steps.<key>.state' reference".to_string(),
                ));
            }
            i += 1; // skip the dot
            if !input[i..].starts_with("state") {
                return Err(GroveError::Runtime(format!(
                    "expected 'state' after 'steps.{key}.' at position {i}"
                )));
            }
            i += 5; // skip "state"
            tokens.push(Token::StepRef(key));
            continue;
        }

        return Err(GroveError::Runtime(format!(
            "unexpected character '{}' at position {} in condition expression",
            chars[i], i
        )));
    }

    Ok(tokens)
}

// ── Recursive-descent parser ─────────────────────────────────────────────────

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    step_states: &'a HashMap<String, String>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos);
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    /// `or_expr := and_expr ( '||' and_expr )*`
    fn parse_or(&mut self) -> GroveResult<bool> {
        let mut left = self.parse_and()?;
        while self.peek() == Some(&Token::Or) {
            self.advance(); // consume ||
            let right = self.parse_and()?;
            left = left || right;
        }
        Ok(left)
    }

    /// `and_expr := primary ( '&&' primary )*`
    fn parse_and(&mut self) -> GroveResult<bool> {
        let mut left = self.parse_primary()?;
        while self.peek() == Some(&Token::And) {
            self.advance(); // consume &&
            let right = self.parse_primary()?;
            left = left && right;
        }
        Ok(left)
    }

    /// `primary := '(' or_expr ')' | comparison`
    /// `comparison := StepRef ('==' | '!=') StringLit`
    fn parse_primary(&mut self) -> GroveResult<bool> {
        match self.peek() {
            Some(Token::LParen) => {
                self.advance(); // consume (
                let result = self.parse_or()?;
                match self.advance() {
                    Some(Token::RParen) => Ok(result),
                    other => Err(GroveError::Runtime(format!(
                        "expected ')' but got {:?}",
                        other
                    ))),
                }
            }
            Some(Token::StepRef(_)) => {
                let step_key = match self.advance() {
                    Some(Token::StepRef(k)) => k.clone(),
                    _ => unreachable!(),
                };
                let op = match self.advance() {
                    Some(Token::Eq) => Token::Eq,
                    Some(Token::Neq) => Token::Neq,
                    other => {
                        return Err(GroveError::Runtime(format!(
                            "expected '==' or '!=' after step reference, got {:?}",
                            other
                        )));
                    }
                };
                let expected = match self.advance() {
                    Some(Token::StringLit(s)) => s.clone(),
                    other => {
                        return Err(GroveError::Runtime(format!(
                            "expected string literal after operator, got {:?}",
                            other
                        )));
                    }
                };
                let actual = self.step_states.get(&step_key).ok_or_else(|| {
                    GroveError::Runtime(format!(
                        "unknown step '{}' referenced in condition expression",
                        step_key
                    ))
                })?;
                match op {
                    Token::Eq => Ok(actual == &expected),
                    Token::Neq => Ok(actual != &expected),
                    _ => unreachable!(),
                }
            }
            other => Err(GroveError::Runtime(format!(
                "unexpected token in condition expression: {:?}",
                other
            ))),
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn states(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn simple_equality_true() {
        let s = states(&[("scan", "completed")]);
        assert!(evaluate_condition("steps.scan.state == 'completed'", &s).unwrap());
    }

    #[test]
    fn simple_equality_false() {
        let s = states(&[("scan", "failed")]);
        assert!(!evaluate_condition("steps.scan.state == 'completed'", &s).unwrap());
    }

    #[test]
    fn inequality_true() {
        let s = states(&[("scan", "completed")]);
        assert!(evaluate_condition("steps.scan.state != 'failed'", &s).unwrap());
    }

    #[test]
    fn inequality_false() {
        let s = states(&[("scan", "failed")]);
        assert!(!evaluate_condition("steps.scan.state != 'failed'", &s).unwrap());
    }

    #[test]
    fn and_both_completed() {
        let s = states(&[("a", "completed"), ("b", "completed")]);
        assert!(
            evaluate_condition(
                "steps.a.state == 'completed' && steps.b.state == 'completed'",
                &s
            )
            .unwrap()
        );
    }

    #[test]
    fn and_one_failed() {
        let s = states(&[("a", "completed"), ("b", "failed")]);
        assert!(
            !evaluate_condition(
                "steps.a.state == 'completed' && steps.b.state == 'completed'",
                &s
            )
            .unwrap()
        );
    }

    #[test]
    fn or_one_completed() {
        let s = states(&[("a", "failed"), ("b", "completed")]);
        assert!(
            evaluate_condition(
                "steps.a.state == 'completed' || steps.b.state == 'completed'",
                &s
            )
            .unwrap()
        );
    }

    #[test]
    fn or_neither_completed() {
        let s = states(&[("a", "failed"), ("b", "failed")]);
        assert!(
            !evaluate_condition(
                "steps.a.state == 'completed' || steps.b.state == 'completed'",
                &s
            )
            .unwrap()
        );
    }

    #[test]
    fn parentheses_combined() {
        let s = states(&[("a", "failed"), ("b", "completed"), ("c", "completed")]);
        let expr = "(steps.a.state == 'completed' || steps.b.state == 'completed') && steps.c.state != 'failed'";
        assert!(evaluate_condition(expr, &s).unwrap());
    }

    #[test]
    fn parentheses_combined_false() {
        let s = states(&[("a", "failed"), ("b", "completed"), ("c", "failed")]);
        let expr = "(steps.a.state == 'completed' || steps.b.state == 'completed') && steps.c.state != 'failed'";
        assert!(!evaluate_condition(expr, &s).unwrap());
    }

    #[test]
    fn unknown_step_returns_error() {
        let s = states(&[("scan", "completed")]);
        let result = evaluate_condition("steps.unknown.state == 'completed'", &s);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("unknown step"),
            "expected 'unknown step' in error: {err_msg}"
        );
    }

    #[test]
    fn empty_expression_returns_true() {
        let s = HashMap::new();
        assert!(evaluate_condition("", &s).unwrap());
    }

    #[test]
    fn whitespace_only_expression_returns_true() {
        let s = HashMap::new();
        assert!(evaluate_condition("   \t\n  ", &s).unwrap());
    }

    #[test]
    fn tokenize_unterminated_string() {
        let result = tokenize("steps.a.state == 'incomplete");
        assert!(result.is_err());
    }

    #[test]
    fn tokenize_step_key_with_underscores_and_dashes() {
        let s = states(&[("my-step_1", "completed")]);
        assert!(evaluate_condition("steps.my-step_1.state == 'completed'", &s).unwrap());
    }
}
