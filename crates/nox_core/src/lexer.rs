use crate::{Diagnostic, Span, Token, TokenKind};

pub(crate) fn lex(source: &str) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(source).lex()
}

struct Lexer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    start: usize,
    current: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            start: 0,
            current: 0,
            tokens: Vec::new(),
        }
    }

    fn lex(mut self) -> Result<Vec<Token>, Diagnostic> {
        while !self.is_at_end() {
            self.start = self.current;
            self.scan_token()?;
        }

        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span {
                start: self.source.len(),
                end: self.source.len(),
            },
        });
        Ok(self.tokens)
    }

    fn scan_token(&mut self) -> Result<(), Diagnostic> {
        let byte = self.advance();
        match byte {
            b'(' => self.push(TokenKind::LeftParen),
            b')' => self.push(TokenKind::RightParen),
            b'{' => self.push(TokenKind::LeftBrace),
            b'}' => self.push(TokenKind::RightBrace),
            b'[' => self.push(TokenKind::LeftBracket),
            b']' => self.push(TokenKind::RightBracket),
            b',' => self.push(TokenKind::Comma),
            b':' => self.push(TokenKind::Colon),
            b';' => self.push(TokenKind::Semicolon),
            b'.' => {
                if self.match_byte(b'.') {
                    self.push(TokenKind::DotDot);
                } else {
                    self.push(TokenKind::Dot);
                }
            }
            b'&' => {
                if self.match_byte(b'&') {
                    self.push(TokenKind::AndAnd);
                } else {
                    return Err(Diagnostic::new("expected '&' after '&'", self.span()));
                }
            }
            b'|' => {
                if self.match_byte(b'|') {
                    self.push(TokenKind::OrOr);
                } else {
                    return Err(Diagnostic::new("expected '|' after '|'", self.span()));
                }
            }
            b'+' => self.push(TokenKind::Plus),
            b'-' => {
                let kind = if self.match_byte(b'>') {
                    TokenKind::Arrow
                } else {
                    TokenKind::Minus
                };
                self.push(kind);
            }
            b'*' => self.push(TokenKind::Star),
            b'/' => {
                if self.match_byte(b'/') {
                    while self.peek() != b'\n' && !self.is_at_end() {
                        self.advance();
                    }
                } else {
                    self.push(TokenKind::Slash);
                }
            }
            b'!' => {
                let kind = if self.match_byte(b'=') {
                    TokenKind::BangEqual
                } else {
                    TokenKind::Bang
                };
                self.push(kind);
            }
            b'=' => {
                let kind = if self.match_byte(b'>') {
                    TokenKind::FatArrow
                } else if self.match_byte(b'=') {
                    TokenKind::EqualEqual
                } else {
                    TokenKind::Equal
                };
                self.push(kind);
            }
            b'>' => {
                let kind = if self.match_byte(b'=') {
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::Greater
                };
                self.push(kind);
            }
            b'<' => {
                let kind = if self.match_byte(b'=') {
                    TokenKind::LessEqual
                } else {
                    TokenKind::Less
                };
                self.push(kind);
            }
            b'"' => self.string()?,
            b' ' | b'\r' | b'\t' | b'\n' => {}
            b'0'..=b'9' => self.number()?,
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.identifier(),
            _ => {
                return Err(Diagnostic::new(
                    format!("unexpected character '{}'", byte as char),
                    self.span(),
                ));
            }
        }
        Ok(())
    }

    fn string(&mut self) -> Result<(), Diagnostic> {
        let mut value = String::new();
        while !self.is_at_end() {
            match self.peek() {
                b'"' => {
                    self.advance();
                    self.push(TokenKind::String(value));
                    return Ok(());
                }
                b'\\' => {
                    let escape_start = self.current;
                    self.advance();
                    if self.is_at_end() {
                        return Err(Diagnostic::new("unterminated string", self.span()));
                    }
                    let escaped = self.advance();
                    let character = match escaped {
                        b'n' => '\n',
                        b't' => '\t',
                        b'"' => '"',
                        b'\\' => '\\',
                        _ => {
                            return Err(Diagnostic::new(
                                format!("unsupported escape sequence '\\{}'", escaped as char),
                                Span {
                                    start: escape_start,
                                    end: self.current,
                                },
                            ));
                        }
                    };
                    value.push(character);
                }
                b'\n' | b'\r' => {
                    return Err(Diagnostic::new(
                        "multiline strings are not supported",
                        Span {
                            start: self.current,
                            end: self.current + 1,
                        },
                    ));
                }
                _ => value.push(self.advance_char()),
            }
        }

        Err(Diagnostic::new("unterminated string", self.span()))
    }

    fn number(&mut self) -> Result<(), Diagnostic> {
        while self.peek().is_ascii_digit() {
            self.advance();
        }

        let mut is_float = false;
        if self.peek() == b'.' && self.peek_next().is_ascii_digit() {
            is_float = true;
            self.advance();
            while self.peek().is_ascii_digit() {
                self.advance();
            }
        }

        let text = &self.source[self.start..self.current];
        if is_float {
            let value = text
                .parse::<f64>()
                .map_err(|_| Diagnostic::new("invalid float literal", self.span()))?;
            self.push(TokenKind::Float(value));
        } else {
            let value = text
                .parse::<i64>()
                .map_err(|_| Diagnostic::new("invalid int literal", self.span()))?;
            self.push(TokenKind::Int(value));
        }
        Ok(())
    }

    fn identifier(&mut self) {
        while self.peek().is_ascii_alphanumeric() || self.peek() == b'_' {
            self.advance();
        }

        let text = &self.source[self.start..self.current];
        let kind = match text {
            "let" => TokenKind::Let,
            "const" => TokenKind::Const,
            "fn" => TokenKind::Fn,
            "return" => TokenKind::Return,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "match" => TokenKind::Match,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "import" => TokenKind::Import,
            "as" => TokenKind::As,
            "export" => TokenKind::Export,
            "record" => TokenKind::Record,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "null" => TokenKind::Null,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            _ => TokenKind::Identifier(text.to_string()),
        };
        self.push(kind);
    }

    fn push(&mut self, kind: TokenKind) {
        self.tokens.push(Token {
            kind,
            span: self.span(),
        });
    }

    fn match_byte(&mut self, expected: u8) -> bool {
        if self.is_at_end() || self.bytes[self.current] != expected {
            return false;
        }
        self.current += 1;
        true
    }

    fn advance(&mut self) -> u8 {
        let byte = self.bytes[self.current];
        self.current += 1;
        byte
    }

    fn advance_char(&mut self) -> char {
        let character = self.source[self.current..]
            .chars()
            .next()
            .expect("advance_char requires a remaining character");
        self.current += character.len_utf8();
        character
    }

    fn peek(&self) -> u8 {
        if self.is_at_end() {
            b'\0'
        } else {
            self.bytes[self.current]
        }
    }

    fn peek_next(&self) -> u8 {
        if self.current + 1 >= self.bytes.len() {
            b'\0'
        } else {
            self.bytes[self.current + 1]
        }
    }

    fn is_at_end(&self) -> bool {
        self.current >= self.bytes.len()
    }

    fn span(&self) -> Span {
        Span {
            start: self.start,
            end: self.current,
        }
    }
}
