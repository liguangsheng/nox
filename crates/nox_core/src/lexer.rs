use crate::{Diagnostic, Span, Token, TokenKind, TokenStringInterpolationPart};

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
                if self.peek() == b'.' && self.peek_next() == b'.' {
                    self.advance();
                    self.advance();
                    self.push(TokenKind::Ellipsis);
                } else if self.match_byte(b'.') {
                    self.push(TokenKind::DotDot);
                } else {
                    self.push(TokenKind::Dot);
                }
            }
            b'&' => {
                if self.match_byte(b'&') {
                    self.push(TokenKind::AndAnd);
                } else {
                    self.push(TokenKind::Ampersand);
                }
            }
            b'|' => {
                if self.match_byte(b'|') {
                    self.push(TokenKind::OrOr);
                } else {
                    self.push(TokenKind::Pipe);
                }
            }
            b'^' => self.push(TokenKind::Caret),
            b'~' => self.push(TokenKind::Tilde),
            b'+' => self.push(TokenKind::Plus),
            b'?' => self.push(TokenKind::Question),
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
                } else if self.match_byte(b'>') {
                    TokenKind::RightShift
                } else {
                    TokenKind::Greater
                };
                self.push(kind);
            }
            b'<' => {
                let kind = if self.match_byte(b'=') {
                    TokenKind::LessEqual
                } else if self.match_byte(b'<') {
                    TokenKind::LeftShift
                } else {
                    TokenKind::Less
                };
                self.push(kind);
            }
            b'"' => {
                if self.peek() == b'"' && self.peek_next() == b'"' {
                    self.advance();
                    self.advance();
                    self.multiline_string()?;
                } else {
                    self.string()?;
                }
            }
            b'\'' => self.character_string()?,
            b' ' | b'\r' | b'\t' | b'\n' => {}
            b'0'..=b'9' => self.number()?,
            b'r' if self.peek() == b'"' => self.raw_string()?,
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
        let string_start = self.start;
        let mut value = String::new();
        let mut parts = Vec::new();
        while !self.is_at_end() {
            match self.peek() {
                b'"' => {
                    self.advance();
                    if parts.is_empty() {
                        self.push(TokenKind::String(value));
                    } else {
                        if !value.is_empty() {
                            parts.push(TokenStringInterpolationPart {
                                text: std::mem::take(&mut value),
                                expression: None,
                                span: self.span(),
                            });
                        }
                        self.push(TokenKind::InterpolatedString(parts));
                    }
                    return Ok(());
                }
                b'$' if self.peek_next() == b'{' => {
                    let part_span = Span {
                        start: self.current,
                        end: self.current + 2,
                    };
                    if !value.is_empty() {
                        parts.push(TokenStringInterpolationPart {
                            text: std::mem::take(&mut value),
                            expression: None,
                            span: part_span,
                        });
                    }
                    self.advance();
                    self.advance();
                    let expression_start = self.current;
                    let expression = self.interpolation_expression(expression_start)?;
                    parts.push(TokenStringInterpolationPart {
                        text: String::new(),
                        expression: Some(expression),
                        span: Span {
                            start: expression_start,
                            end: self.current,
                        },
                    });
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
                        b'$' => '$',
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

        Err(Diagnostic::new(
            "unterminated string",
            Span {
                start: string_start,
                end: self.current,
            },
        ))
    }

    fn interpolation_expression(&mut self, expression_start: usize) -> Result<String, Diagnostic> {
        let mut depth = 1usize;
        while !self.is_at_end() {
            match self.peek() {
                b'{' => {
                    depth += 1;
                    self.advance();
                }
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        let expression_end = self.current;
                        self.advance();
                        let expression = self.source[expression_start..expression_end].trim();
                        if expression.is_empty() {
                            return Err(Diagnostic::new(
                                "string interpolation placeholder cannot be empty",
                                Span {
                                    start: expression_start,
                                    end: expression_end,
                                },
                            )
                            .with_code("string.interpolation"));
                        }
                        return Ok(expression.to_string());
                    }
                    self.advance();
                }
                b'"' => self.skip_nested_string()?,
                b'\n' | b'\r' => {
                    return Err(Diagnostic::new(
                        "multiline strings are not supported",
                        Span {
                            start: self.current,
                            end: self.current + 1,
                        },
                    ));
                }
                _ => {
                    self.advance_char();
                }
            }
        }
        Err(Diagnostic::new(
            "unterminated string interpolation placeholder",
            Span {
                start: expression_start.saturating_sub(2),
                end: self.current,
            },
        )
        .with_code("string.interpolation"))
    }

    fn multiline_string(&mut self) -> Result<(), Diagnostic> {
        let content_start = self.current;
        while !self.is_at_end() {
            if self.peek() == b'"' && self.peek_next() == b'"' && self.peek_next_next() == b'"' {
                let value = self.source[content_start..self.current].to_string();
                self.advance();
                self.advance();
                self.advance();
                self.push(TokenKind::String(value));
                return Ok(());
            }
            self.advance_char();
        }
        Err(Diagnostic::new(
            "unterminated multiline string",
            self.span(),
        ))
    }

    fn raw_string(&mut self) -> Result<(), Diagnostic> {
        self.advance();
        let content_start = self.current;
        while !self.is_at_end() {
            if self.peek() == b'"' {
                let value = self.source[content_start..self.current].to_string();
                self.advance();
                self.push(TokenKind::String(value));
                return Ok(());
            }
            if self.peek() == b'\n' || self.peek() == b'\r' {
                return Err(Diagnostic::new(
                    "raw strings cannot contain newlines",
                    Span {
                        start: self.current,
                        end: self.current + 1,
                    },
                ));
            }
            self.advance_char();
        }
        Err(Diagnostic::new("unterminated raw string", self.span()))
    }

    fn character_string(&mut self) -> Result<(), Diagnostic> {
        let literal_start = self.start;
        if self.is_at_end() {
            return Err(self.invalid_character("unterminated character literal"));
        }
        if self.peek() == b'\'' {
            self.advance();
            return Err(self.invalid_character("character literal cannot be empty"));
        }
        if self.peek() == b'\n' || self.peek() == b'\r' {
            return Err(self.invalid_character("character literal cannot contain newlines"));
        }

        let character = if self.peek() == b'\\' {
            let escape_start = self.current;
            self.advance();
            if self.is_at_end() {
                return Err(self.invalid_character("unterminated character literal"));
            }
            let escaped = self.advance();
            match escaped {
                b'n' => '\n',
                b't' => '\t',
                b'\'' => '\'',
                b'\\' => '\\',
                _ => {
                    return Err(Diagnostic::new(
                        format!("unsupported character escape '\\{}'", escaped as char),
                        Span {
                            start: escape_start,
                            end: self.current,
                        },
                    )
                    .with_code("lex.invalid-character"));
                }
            }
        } else {
            self.advance_char()
        };

        if self.is_at_end() {
            return Err(Diagnostic::new(
                "unterminated character literal",
                Span {
                    start: literal_start,
                    end: self.current,
                },
            )
            .with_code("lex.invalid-character"));
        }
        if self.peek() != b'\'' {
            while !self.is_at_end()
                && self.peek() != b'\''
                && self.peek() != b'\n'
                && self.peek() != b'\r'
            {
                self.advance_char();
            }
            if self.peek() == b'\'' {
                self.advance();
            }
            return Err(
                self.invalid_character("character literal must contain exactly one character")
            );
        }
        self.advance();
        self.push(TokenKind::String(character.to_string()));
        Ok(())
    }

    fn skip_nested_string(&mut self) -> Result<(), Diagnostic> {
        self.advance();
        while !self.is_at_end() {
            match self.peek() {
                b'"' => {
                    self.advance();
                    return Ok(());
                }
                b'\\' => {
                    self.advance();
                    if self.is_at_end() {
                        return Err(Diagnostic::new("unterminated string", self.span()));
                    }
                    self.advance_char();
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
                _ => {
                    self.advance_char();
                }
            }
        }
        Err(Diagnostic::new("unterminated string", self.span()))
    }

    fn number(&mut self) -> Result<(), Diagnostic> {
        if self.source.as_bytes()[self.start] == b'0' {
            match self.peek() {
                b'x' | b'X' => return self.radix_integer(16),
                b'b' | b'B' => return self.radix_integer(2),
                b'o' | b'O' => return self.radix_integer(8),
                _ => {}
            }
        }

        while self.peek().is_ascii_digit() || self.peek() == b'_' {
            self.advance();
        }
        self.validate_integer_separators()?;

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
            let normalized = text.replace('_', "");
            let value = normalized
                .parse::<i64>()
                .map_err(|_| self.invalid_integer("invalid int literal"))?;
            self.push(TokenKind::Int(value));
        }
        Ok(())
    }

    fn radix_integer(&mut self, radix: u32) -> Result<(), Diagnostic> {
        self.advance();
        let digits_start = self.current;
        let mut saw_digit = false;
        let mut last_was_underscore = false;

        while self.peek().is_ascii_alphanumeric() || self.peek() == b'_' {
            let byte = self.advance();
            if byte == b'_' {
                if !saw_digit || last_was_underscore {
                    return Err(self.invalid_integer("invalid integer separator"));
                }
                last_was_underscore = true;
                continue;
            }
            if !is_digit_for_radix(byte, radix) {
                return Err(self.invalid_integer("invalid digit for integer literal"));
            }
            saw_digit = true;
            last_was_underscore = false;
        }

        if !saw_digit {
            return Err(self.invalid_integer("integer literal requires at least one digit"));
        }
        if last_was_underscore {
            return Err(self.invalid_integer("invalid integer separator"));
        }

        let normalized = self.source[digits_start..self.current].replace('_', "");
        let value = i64::from_str_radix(&normalized, radix)
            .map_err(|_| self.invalid_integer("invalid int literal"))?;
        self.push(TokenKind::Int(value));
        Ok(())
    }

    fn validate_integer_separators(&self) -> Result<(), Diagnostic> {
        let text = &self.source[self.start..self.current];
        if text.ends_with('_') || text.contains("__") {
            return Err(self.invalid_integer("invalid integer separator"));
        }
        Ok(())
    }

    fn invalid_integer(&self, message: &'static str) -> Diagnostic {
        Diagnostic::new(message, self.span()).with_code("lex.invalid-integer")
    }

    fn invalid_character(&self, message: &'static str) -> Diagnostic {
        Diagnostic::new(message, self.span()).with_code("lex.invalid-character")
    }

    fn identifier(&mut self) {
        while self.peek().is_ascii_alphanumeric() || self.peek() == b'_' {
            self.advance();
        }

        let text = &self.source[self.start..self.current];
        let kind = match text {
            "let" => TokenKind::Let,
            "const" => TokenKind::Const,
            "type" => TokenKind::Type,
            "enum" => TokenKind::Enum,
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
            "try" | "catch" | "panic" | "defer" | "finally" => {
                TokenKind::Reserved(text.to_string())
            }
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

    fn peek_next_next(&self) -> u8 {
        if self.current + 2 >= self.bytes.len() {
            b'\0'
        } else {
            self.bytes[self.current + 2]
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

fn is_digit_for_radix(byte: u8, radix: u32) -> bool {
    match radix {
        2 => matches!(byte, b'0' | b'1'),
        8 => matches!(byte, b'0'..=b'7'),
        16 => byte.is_ascii_hexdigit(),
        _ => false,
    }
}
