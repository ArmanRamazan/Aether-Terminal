//! Lexical analysis for the rule DSL using logos.

use logos::Logos;

use crate::error::ScriptError;

/// Byte offset span in source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// A value paired with its source location.
pub type Spanned<T> = (T, Span);

/// Tokens produced by the rule DSL lexer.
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"([ \t\n\r]+|//[^\n]*)")]
pub enum Token {
    // — Keywords —
    #[token("rule")]
    Rule,
    #[token("when")]
    When,
    #[token("then")]
    Then,
    #[token("for")]
    For,
    #[token("and")]
    And,
    #[token("or")]
    Or,
    #[token("alert")]
    Alert,
    #[token("kill")]
    Kill,
    #[token("log")]
    Log,
    #[token("severity")]
    Severity,
    #[token("info")]
    Info,
    #[token("warning")]
    Warning,
    #[token("critical")]
    Critical,
    #[token("process")]
    Process,

    // — Comparison operators —
    #[token(">=")]
    Gte,
    #[token("<=")]
    Lte,
    #[token("==")]
    Eq,
    #[token("!=")]
    Neq,
    #[token(">")]
    Gt,
    #[token("<")]
    Lt,

    // — Punctuation —
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token(".")]
    Dot,

    // — Literals —
    #[regex(r"[0-9]+%", lex_percent)]
    Percent(f64),

    #[regex(r"[0-9]+[smh]", lex_duration)]
    Duration(u64),

    #[regex(r"[0-9]+\.[0-9]+", |lex| lex.slice().parse::<f64>().ok())]
    Float(f64),

    #[regex(r"[0-9]+", |lex| lex.slice().parse::<i64>().ok())]
    Integer(i64),

    #[regex(r#""[^"]*""#, |lex| {
        let s = lex.slice();
        s[1..s.len() - 1].to_string()
    })]
    StringLit(String),

    // — Identifiers (keywords take priority) —
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),
}

fn lex_percent(lex: &mut logos::Lexer<'_, Token>) -> Option<f64> {
    let slice = lex.slice();
    slice[..slice.len() - 1].parse().ok()
}

fn lex_duration(lex: &mut logos::Lexer<'_, Token>) -> Option<u64> {
    let slice = lex.slice();
    let (num, suffix) = slice.split_at(slice.len() - 1);
    let n: u64 = num.parse().ok()?;
    match suffix {
        "s" => Some(n),
        "m" => Some(n * 60),
        "h" => Some(n * 3600),
        _ => None,
    }
}

/// Tokenize source into a sequence of spanned tokens.
pub fn tokenize(source: &str) -> Result<Vec<Spanned<Token>>, ScriptError> {
    let mut tokens = Vec::new();
    let mut lexer = Token::lexer(source);

    while let Some(result) = lexer.next() {
        let span = lexer.span();
        let span = Span {
            start: span.start,
            end: span.end,
        };
        match result {
            Ok(token) => tokens.push((token, span)),
            Err(()) => {
                return Err(ScriptError::Lex(format!(
                    "unexpected token at offset {}",
                    span.start,
                )));
            }
        }
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple_rule() {
        let source = r#"rule high_cpu {
            when process.cpu > 90%
            then alert "high cpu" severity critical
        }"#;
        let tokens = tokenize(source).expect("should tokenize");
        let kinds: Vec<_> = tokens.iter().map(|(t, _)| t.clone()).collect();

        assert_eq!(kinds[0], Token::Rule);
        assert_eq!(kinds[1], Token::Ident("high_cpu".into()));
        assert_eq!(kinds[2], Token::LBrace);
        assert_eq!(kinds[3], Token::When);
        assert_eq!(kinds[4], Token::Process);
        assert_eq!(kinds[5], Token::Dot);
        assert_eq!(kinds[6], Token::Ident("cpu".into()));
        assert_eq!(kinds[7], Token::Gt);
        assert_eq!(kinds[8], Token::Percent(90.0));
        assert_eq!(kinds[9], Token::Then);
        assert_eq!(kinds[10], Token::Alert);
        assert_eq!(kinds[11], Token::StringLit("high cpu".into()));
        assert_eq!(kinds[12], Token::Severity);
        assert_eq!(kinds[13], Token::Critical);
        assert_eq!(kinds[14], Token::RBrace);
        assert_eq!(kinds.len(), 15);
    }

    #[test]
    fn test_tokenize_duration_literals() {
        let tokens = tokenize("30s 5m 2h").expect("should tokenize");
        let kinds: Vec<_> = tokens.iter().map(|(t, _)| t.clone()).collect();

        assert_eq!(kinds[0], Token::Duration(30), "30 seconds");
        assert_eq!(kinds[1], Token::Duration(300), "5 minutes in seconds");
        assert_eq!(kinds[2], Token::Duration(7200), "2 hours in seconds");
    }

    #[test]
    fn test_tokenize_operators() {
        let tokens = tokenize("> < >= <= == !=").expect("should tokenize");
        let kinds: Vec<_> = tokens.iter().map(|(t, _)| t.clone()).collect();

        assert_eq!(
            kinds,
            vec![Token::Gt, Token::Lt, Token::Gte, Token::Lte, Token::Eq, Token::Neq],
        );
    }

    #[test]
    fn test_tokenize_error_on_invalid_token() {
        let result = tokenize("rule @invalid");
        assert!(result.is_err(), "@ should produce a lex error");

        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("unexpected token"),
            "error message should mention unexpected token, got: {err}",
        );
    }
}
