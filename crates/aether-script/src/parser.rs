//! Recursive descent parser: token stream → AST.
//!
//! Grammar (precedence low→high): or → and → not → comparison.
//! Error recovery: on failure, skip tokens until the next `rule` keyword.

use crate::ast::{Action, CmpOp, Condition, Expr, Literal, Rule, RuleFile, Severity};
use crate::lexer::{Span, Spanned, Token};

/// A parse error with location and message.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "parse error at {}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}

struct Parser {
    tokens: Vec<Spanned<Token>>,
    pos: usize,
    errors: Vec<ParseError>,
}

/// Parse a token stream into a rule file AST.
///
/// Collects errors and attempts recovery per-rule. Returns `Err` only if
/// at least one error was encountered.
pub fn parse(tokens: Vec<Spanned<Token>>) -> Result<RuleFile, Vec<ParseError>> {
    let mut parser = Parser {
        tokens,
        pos: 0,
        errors: Vec::new(),
    };
    let rules = parser.parse_rule_file();
    if parser.errors.is_empty() {
        Ok(RuleFile { rules })
    } else {
        Err(parser.errors)
    }
}

impl Parser {
    // ── Helpers ──────────────────────────────────────────────────────

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|(t, _)| t)
    }

    fn span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|(_, s)| *s)
            .unwrap_or(Span {
                start: self.eof_offset(),
                end: self.eof_offset(),
            })
    }

    fn eof_offset(&self) -> usize {
        self.tokens.last().map(|(_, s)| s.end).unwrap_or(0)
    }

    fn advance(&mut self) -> &Spanned<Token> {
        let tok = &self.tokens[self.pos];
        self.pos += 1;
        tok
    }

    fn expect(&mut self, expected: &Token) -> Result<Span, ()> {
        if self.peek() == Some(expected) {
            let (_, span) = self.advance();
            Ok(*span)
        } else {
            self.error(format!("expected {}, found {}", fmt_token(expected), self.found()));
            Err(())
        }
    }

    fn error(&mut self, message: String) {
        self.errors.push(ParseError {
            message,
            span: self.span(),
        });
    }

    fn found(&self) -> String {
        match self.peek() {
            Some(t) => fmt_token(t),
            None => "end of input".to_string(),
        }
    }

    /// Skip tokens until we see `rule` or EOF (error recovery).
    fn recover_to_rule(&mut self) {
        while let Some(tok) = self.peek() {
            if matches!(tok, Token::Rule) {
                return;
            }
            self.pos += 1;
        }
    }

    // ── Top-level ───────────────────────────────────────────────────

    fn parse_rule_file(&mut self) -> Vec<Rule> {
        let mut rules = Vec::new();
        while self.peek().is_some() {
            match self.parse_rule() {
                Ok(rule) => rules.push(rule),
                Err(()) => self.recover_to_rule(),
            }
        }
        rules
    }

    // ── Rule ────────────────────────────────────────────────────────

    fn parse_rule(&mut self) -> Result<Rule, ()> {
        let start_span = self.expect(&Token::Rule)?;

        let name = match self.peek() {
            Some(Token::Ident(_)) => {
                let (tok, _) = self.advance().clone();
                match tok {
                    Token::Ident(name) => name,
                    _ => unreachable!(),
                }
            }
            _ => {
                self.error(format!("expected rule name, found {}", self.found()));
                return Err(());
            }
        };

        self.expect(&Token::LBrace)?;
        self.expect(&Token::When)?;

        let condition = self.parse_condition()?;

        self.expect(&Token::Then)?;

        let mut actions = Vec::new();
        while let Some(Token::Alert | Token::Kill | Token::Log) = self.peek() {
            actions.push(self.parse_action()?);
        }
        if actions.is_empty() {
            self.error("expected at least one action after 'then'".to_string());
            return Err(());
        }

        let end_span = self.expect(&Token::RBrace)?;

        Ok(Rule {
            name,
            condition,
            actions,
            span: Span {
                start: start_span.start,
                end: end_span.end,
            },
        })
    }

    // ── Condition (with optional duration) ──────────────────────────

    fn parse_condition(&mut self) -> Result<Condition, ()> {
        let cond = self.parse_or()?;

        // Optional `for <duration>` wrapper.
        if self.peek() == Some(&Token::For) {
            self.advance();
            match self.peek() {
                Some(Token::Duration(_)) => {
                    let (tok, _) = self.advance().clone();
                    match tok {
                        Token::Duration(secs) => Ok(Condition::Duration {
                            condition: Box::new(cond),
                            seconds: secs,
                        }),
                        _ => unreachable!(),
                    }
                }
                _ => {
                    self.error(format!(
                        "expected duration after 'for', found {}",
                        self.found()
                    ));
                    Err(())
                }
            }
        } else {
            Ok(cond)
        }
    }

    // ── Boolean operators (precedence: or < and < not) ──────────────

    fn parse_or(&mut self) -> Result<Condition, ()> {
        let mut left = self.parse_and()?;
        while self.peek() == Some(&Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Condition::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Condition, ()> {
        let mut left = self.parse_not()?;
        while self.peek() == Some(&Token::And) {
            self.advance();
            let right = self.parse_not()?;
            left = Condition::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<Condition, ()> {
        if matches!(self.peek(), Some(Token::Ident(s)) if s == "not") {
            self.advance();
            let inner = self.parse_not()?;
            Ok(Condition::Not(Box::new(inner)))
        } else {
            self.parse_comparison()
        }
    }

    // ── Comparison ──────────────────────────────────────────────────

    fn parse_comparison(&mut self) -> Result<Condition, ()> {
        let left = self.parse_expr()?;
        let op = self.parse_cmp_op()?;
        let right = self.parse_expr()?;
        Ok(Condition::Comparison { left, op, right })
    }

    fn parse_cmp_op(&mut self) -> Result<CmpOp, ()> {
        let op = match self.peek() {
            Some(Token::Gt) => CmpOp::Gt,
            Some(Token::Lt) => CmpOp::Lt,
            Some(Token::Gte) => CmpOp::Gte,
            Some(Token::Lte) => CmpOp::Lte,
            Some(Token::Eq) => CmpOp::Eq,
            Some(Token::Neq) => CmpOp::Neq,
            _ => {
                self.error(format!(
                    "expected comparison operator, found {}",
                    self.found()
                ));
                return Err(());
            }
        };
        self.advance();
        Ok(op)
    }

    // ── Expressions ─────────────────────────────────────────────────

    fn parse_expr(&mut self) -> Result<Expr, ()> {
        match self.peek() {
            Some(Token::Process) => {
                self.advance();
                self.expect(&Token::Dot)?;
                match self.peek() {
                    Some(Token::Ident(_)) => {
                        let (tok, _) = self.advance().clone();
                        match tok {
                            Token::Ident(field) => Ok(Expr::FieldAccess {
                                object: "process".to_string(),
                                field,
                            }),
                            _ => unreachable!(),
                        }
                    }
                    _ => {
                        self.error(format!("expected field name after '.', found {}", self.found()));
                        Err(())
                    }
                }
            }
            Some(Token::Ident(_)) => {
                let (tok, _) = self.advance().clone();
                let object = match tok {
                    Token::Ident(name) => name,
                    _ => unreachable!(),
                };
                if self.peek() == Some(&Token::Dot) {
                    self.advance();
                    match self.peek() {
                        Some(Token::Ident(_)) => {
                            let (tok, _) = self.advance().clone();
                            match tok {
                                Token::Ident(field) => Ok(Expr::FieldAccess { object, field }),
                                _ => unreachable!(),
                            }
                        }
                        _ => {
                            self.error(format!(
                                "expected field name after '.', found {}",
                                self.found()
                            ));
                            Err(())
                        }
                    }
                } else {
                    self.error(format!("expected '.' after identifier '{object}'"));
                    Err(())
                }
            }
            Some(Token::Integer(_)) => {
                let (tok, _) = self.advance().clone();
                match tok {
                    Token::Integer(n) => Ok(Expr::Literal(Literal::Int(n))),
                    _ => unreachable!(),
                }
            }
            Some(Token::Float(_)) => {
                let (tok, _) = self.advance().clone();
                match tok {
                    Token::Float(n) => Ok(Expr::Literal(Literal::Float(n))),
                    _ => unreachable!(),
                }
            }
            Some(Token::Percent(_)) => {
                let (tok, _) = self.advance().clone();
                match tok {
                    Token::Percent(n) => Ok(Expr::Literal(Literal::Percent(n))),
                    _ => unreachable!(),
                }
            }
            Some(Token::Duration(_)) => {
                let (tok, _) = self.advance().clone();
                match tok {
                    Token::Duration(n) => Ok(Expr::Literal(Literal::Duration(n))),
                    _ => unreachable!(),
                }
            }
            Some(Token::StringLit(_)) => {
                let (tok, _) = self.advance().clone();
                match tok {
                    Token::StringLit(s) => Ok(Expr::Literal(Literal::Str(s))),
                    _ => unreachable!(),
                }
            }
            _ => {
                self.error(format!("expected expression, found {}", self.found()));
                Err(())
            }
        }
    }

    // ── Actions ─────────────────────────────────────────────────────

    fn parse_action(&mut self) -> Result<Action, ()> {
        match self.peek() {
            Some(Token::Alert) => {
                self.advance();
                let message = self.expect_string("alert message")?;
                self.expect(&Token::Severity)?;
                let severity = self.parse_severity()?;
                Ok(Action::Alert { message, severity })
            }
            Some(Token::Kill) => {
                self.advance();
                Ok(Action::Kill)
            }
            Some(Token::Log) => {
                self.advance();
                let message = self.expect_string("log message")?;
                Ok(Action::Log { message })
            }
            _ => {
                self.error(format!("expected action, found {}", self.found()));
                Err(())
            }
        }
    }

    fn expect_string(&mut self, context: &str) -> Result<String, ()> {
        match self.peek() {
            Some(Token::StringLit(_)) => {
                let (tok, _) = self.advance().clone();
                match tok {
                    Token::StringLit(s) => Ok(s),
                    _ => unreachable!(),
                }
            }
            _ => {
                self.error(format!(
                    "expected string for {context}, found {}",
                    self.found()
                ));
                Err(())
            }
        }
    }

    fn parse_severity(&mut self) -> Result<Severity, ()> {
        match self.peek() {
            Some(Token::Info) => {
                self.advance();
                Ok(Severity::Info)
            }
            Some(Token::Warning) => {
                self.advance();
                Ok(Severity::Warning)
            }
            Some(Token::Critical) => {
                self.advance();
                Ok(Severity::Critical)
            }
            _ => {
                self.error(format!(
                    "expected severity (info, warning, critical), found {}",
                    self.found()
                ));
                Err(())
            }
        }
    }
}

fn fmt_token(token: &Token) -> String {
    match token {
        Token::Rule => "'rule'".to_string(),
        Token::When => "'when'".to_string(),
        Token::Then => "'then'".to_string(),
        Token::For => "'for'".to_string(),
        Token::And => "'and'".to_string(),
        Token::Or => "'or'".to_string(),
        Token::Alert => "'alert'".to_string(),
        Token::Kill => "'kill'".to_string(),
        Token::Log => "'log'".to_string(),
        Token::Severity => "'severity'".to_string(),
        Token::Info => "'info'".to_string(),
        Token::Warning => "'warning'".to_string(),
        Token::Critical => "'critical'".to_string(),
        Token::Process => "'process'".to_string(),
        Token::Gte => "'>='".to_string(),
        Token::Lte => "'<='".to_string(),
        Token::Eq => "'=='".to_string(),
        Token::Neq => "'!='".to_string(),
        Token::Gt => "'>'".to_string(),
        Token::Lt => "'<'".to_string(),
        Token::LBrace => "'{'".to_string(),
        Token::RBrace => "'}'".to_string(),
        Token::Dot => "'.'".to_string(),
        Token::Percent(n) => format!("{n}%"),
        Token::Duration(n) => format!("{n}s"),
        Token::Float(n) => format!("{n}"),
        Token::Integer(n) => format!("{n}"),
        Token::StringLit(s) => format!("\"{s}\""),
        Token::Ident(s) => format!("identifier '{s}'"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn parse_source(source: &str) -> Result<RuleFile, Vec<ParseError>> {
        let tokens = tokenize(source).expect("lexer should succeed");
        parse(tokens)
    }

    #[test]
    fn test_parse_simple_rule() {
        let ast = parse_source(
            r#"rule high_cpu {
                when process.cpu > 90%
                then alert "CPU too high" severity critical
            }"#,
        )
        .expect("should parse");

        assert_eq!(ast.rules.len(), 1);
        let rule = &ast.rules[0];
        assert_eq!(rule.name, "high_cpu");
        assert!(matches!(
            &rule.condition,
            Condition::Comparison {
                left: Expr::FieldAccess { object, field },
                op: CmpOp::Gt,
                right: Expr::Literal(Literal::Percent(p)),
            } if object == "process" && field == "cpu" && (*p - 90.0).abs() < f64::EPSILON
        ));
        assert_eq!(rule.actions.len(), 1);
        assert!(matches!(
            &rule.actions[0],
            Action::Alert { message, severity: Severity::Critical }
            if message == "CPU too high"
        ));
    }

    #[test]
    fn test_parse_compound_condition_and_or() {
        let ast = parse_source(
            r#"rule compound {
                when process.cpu > 80% and process.mem > 70% or process.threads > 100
                then alert "compound" severity warning
            }"#,
        )
        .expect("should parse");

        let rule = &ast.rules[0];
        // or binds loosest: (cpu > 80 AND mem > 70) OR (threads > 100)
        assert!(matches!(&rule.condition, Condition::Or(_, _)));
        if let Condition::Or(left, _right) = &rule.condition {
            assert!(matches!(left.as_ref(), Condition::And(_, _)));
        }
    }

    #[test]
    fn test_parse_duration_condition() {
        let ast = parse_source(
            r#"rule leak {
                when process.mem_growth > 5% for 60s
                then alert "memory leak" severity warning
            }"#,
        )
        .expect("should parse");

        let rule = &ast.rules[0];
        assert!(matches!(
            &rule.condition,
            Condition::Duration { seconds: 60, .. }
        ));
        assert_eq!(rule.duration_secs(), Some(60));
    }

    #[test]
    fn test_parse_error_recovery() {
        // First rule is broken (missing 'when'), second is valid.
        let source = r#"
            rule broken {
                process.cpu > 50%
                then alert "oops" severity info
            }
            rule ok {
                when process.cpu > 90%
                then alert "fine" severity critical
            }
        "#;
        let err = parse_source(source).unwrap_err();
        // At least one error from the broken rule.
        assert!(!err.is_empty(), "should have errors");
        // Despite errors, we should have attempted recovery.
        // The parser records errors and skips to the next 'rule'.
    }

    #[test]
    fn test_parse_all_default_rules() {
        let source = include_str!("../../../rules/default.aether");
        let tokens = tokenize(source).expect("lexer should succeed on default.aether");
        let ast = parse(tokens).expect("parser should succeed on default.aether");
        assert!(
            ast.rules.len() >= 3,
            "default.aether should have at least 3 rules, got {}",
            ast.rules.len()
        );
    }
}
