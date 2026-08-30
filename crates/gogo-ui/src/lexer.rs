//! Tokens for the gogo grammar (`gogo-spec.md` "Values" + "Grammar").
//! Value suffixes (`/opacity`, `/key`, `@pos`) are attached here so the
//! parser sees one token per value.

use crate::diag::Diagnostic;

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    /// A bare word, possibly dotted (`screen.wide`, `a.b.c`) or with `:`
    /// (`on:activate`). `opacity`/`pos` are the `/`/`@` suffixes a palette
    /// name may carry.
    Ident { word: String, opacity: Option<f32>, pos: Option<f32> },
    /// `.Style` (a dot after whitespace).
    Style(String),
    /// `$name` with an optional attached path (`$task.id`) and `/opacity`.
    Ref { name: String, opacity: Option<f32> },
    Str { text: String, key: Option<String> },
    /// A number with its unit (`""`, `epx`, `px`, `%`).
    Num { value: f32, unit: String },
    Size { w: Option<f32>, h: Option<f32> },
    /// `#RRGGBB[AA]` as bytes, plus suffixes.
    Color { rgba: [u8; 4], opacity: Option<f32>, pos: Option<f32> },
    Arrow,
    /// `..` — a range (`$min..$max`).
    DotDot,
    Comma,
    Eq,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Newline,
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
}

struct Lexer<'a> {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    tokens: Vec<Token>,
    diags: &'a mut Vec<Diagnostic>,
}

pub fn lex(source: &str, diags: &mut Vec<Diagnostic>) -> Vec<Token> {
    // WORD JOINERs are stripped before lexing; NBSP is whitespace (spec).
    let chars: Vec<char> = source.chars().filter(|c| *c != '\u{2060}').collect();
    let mut lx = Lexer { chars, pos: 0, line: 1, tokens: Vec::new(), diags };
    lx.run();
    lx.tokens
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

impl<'a> Lexer<'a> {
    fn peek(&self) -> char {
        self.chars.get(self.pos).copied().unwrap_or('\0')
    }

    fn peek_at(&self, offset: usize) -> char {
        self.chars.get(self.pos + offset).copied().unwrap_or('\0')
    }

    fn push(&mut self, tok: Tok) {
        self.tokens.push(Token { tok, line: self.line });
    }

    fn error(&mut self, code: &str, message: String) {
        self.diags.push(Diagnostic::error(code, self.line, message));
    }

    fn run(&mut self) {
        while self.pos < self.chars.len() {
            let c = self.peek();
            match c {
                '\n' => {
                    self.push(Tok::Newline);
                    self.line += 1;
                    self.pos += 1;
                }
                ' ' | '\t' | '\r' | '\u{00A0}' => self.pos += 1,
                '/' if self.peek_at(1) == '/' => {
                    while self.pos < self.chars.len() && self.peek() != '\n' {
                        self.pos += 1;
                    }
                }
                ';' => {
                    self.push(Tok::Newline);
                    self.pos += 1;
                }
                ',' => {
                    self.push(Tok::Comma);
                    self.pos += 1;
                }
                '=' => {
                    self.push(Tok::Eq);
                    self.pos += 1;
                }
                '{' => {
                    self.push(Tok::LBrace);
                    self.pos += 1;
                }
                '}' => {
                    self.push(Tok::RBrace);
                    self.pos += 1;
                }
                '[' => {
                    self.push(Tok::LBracket);
                    self.pos += 1;
                }
                ']' => {
                    self.push(Tok::RBracket);
                    self.pos += 1;
                }
                '(' => {
                    self.push(Tok::LParen);
                    self.pos += 1;
                }
                ')' => {
                    self.push(Tok::RParen);
                    self.pos += 1;
                }
                '-' if self.peek_at(1) == '>' => {
                    self.push(Tok::Arrow);
                    self.pos += 2;
                }
                '-' if self.peek_at(1).is_ascii_digit() || self.peek_at(1) == '.' => self.number(),
                '"' => self.string(),
                '#' => self.color(),
                '$' => self.reference(),
                '.' if self.peek_at(1) == '.' => {
                    self.push(Tok::DotDot);
                    self.pos += 2;
                }
                '.' if is_ident_start(self.peek_at(1)) => {
                    self.pos += 1;
                    let word = self.word();
                    self.push(Tok::Style(word));
                }
                _ if c.is_ascii_digit() || (c == '.' && self.peek_at(1).is_ascii_digit()) => self.number(),
                'x' if self.peek_at(1).is_ascii_digit() && !self.prev_is_ident() => {
                    // `x110` — a size with only a height.
                    self.pos += 1;
                    let h = self.digits();
                    self.push(Tok::Size { w: None, h: Some(h) });
                }
                _ if is_ident_start(c) => self.ident(),
                _ => {
                    self.error("G0001", format!("unexpected character `{c}`"));
                    self.pos += 1;
                }
            }
        }
        self.push(Tok::Eof);
    }

    fn prev_is_ident(&self) -> bool {
        self.pos > 0 && is_ident_char(self.chars[self.pos - 1])
    }

    fn word(&mut self) -> String {
        let start = self.pos;
        while is_ident_char(self.peek()) {
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect()
    }

    /// A word with attached `.path` segments and `:` (prop keys like
    /// `on:activate`).
    fn dotted_word(&mut self) -> String {
        let mut word = self.word();
        loop {
            let c = self.peek();
            if (c == '.' || c == ':') && is_ident_start(self.peek_at(1)) {
                self.pos += 1;
                word.push(c);
                word.push_str(&self.word());
            } else {
                break;
            }
        }
        word
    }

    fn digits(&mut self) -> f32 {
        let start = self.pos;
        while self.peek().is_ascii_digit() || self.peek() == '.' {
            self.pos += 1;
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse().unwrap_or(0.0)
    }

    /// `/0.5` after a value.
    fn opacity_suffix(&mut self) -> Option<f32> {
        if self.peek() == '/' && (self.peek_at(1).is_ascii_digit() || self.peek_at(1) == '.') {
            self.pos += 1;
            return Some(self.digits());
        }
        None
    }

    /// `@0.24` after a color.
    fn pos_suffix(&mut self) -> Option<f32> {
        if self.peek() == '@' && (self.peek_at(1).is_ascii_digit() || self.peek_at(1) == '.') {
            self.pos += 1;
            return Some(self.digits());
        }
        None
    }

    fn ident(&mut self) {
        let word = self.dotted_word();
        let opacity = self.opacity_suffix();
        let pos = self.pos_suffix();
        self.push(Tok::Ident { word, opacity, pos });
    }

    fn number(&mut self) {
        let negative = self.peek() == '-';
        if negative {
            self.pos += 1;
        }
        let value = self.digits() * if negative { -1.0 } else { 1.0 };
        if self.peek() == 'x' && !negative {
            // `220x110` / `220x`.
            self.pos += 1;
            let h = if self.peek().is_ascii_digit() { Some(self.digits()) } else { None };
            self.push(Tok::Size { w: Some(value), h });
            return;
        }
        let mut unit = String::new();
        if self.peek() == '%' {
            unit.push('%');
            self.pos += 1;
        } else if is_ident_start(self.peek()) {
            let save = self.pos;
            let w = self.word();
            if w == "epx" || w == "px" {
                unit = w;
            } else {
                self.pos = save;
                self.error("G0002", format!("unknown unit `{w}` on a number (epx, px or % only)"));
            }
        }
        self.push(Tok::Num { value, unit });
    }

    fn string(&mut self) {
        self.pos += 1;
        let mut text = String::new();
        loop {
            let c = self.peek();
            match c {
                '\0' | '\n' => {
                    self.error("G0003", "unterminated string".to_string());
                    break;
                }
                '"' => {
                    self.pos += 1;
                    break;
                }
                '\\' => {
                    self.pos += 1;
                    let e = self.peek();
                    text.push(match e {
                        'n' => '\n',
                        't' => '\t',
                        '"' => '"',
                        '\\' => '\\',
                        other => {
                            self.error("G0004", format!("unknown escape `\\{other}`"));
                            other
                        }
                    });
                    self.pos += 1;
                }
                _ => {
                    text.push(c);
                    self.pos += 1;
                }
            }
        }
        // `/key` — a translation term id; may start with a digit.
        let key = if self.peek() == '/' && is_ident_char(self.peek_at(1)) {
            self.pos += 1;
            Some(self.word())
        } else {
            None
        };
        self.push(Tok::Str { text, key });
    }

    fn color(&mut self) {
        self.pos += 1;
        let start = self.pos;
        while self.peek().is_ascii_hexdigit() {
            self.pos += 1;
        }
        let hex: String = self.chars[start..self.pos].iter().collect();
        let rgba = match hex.len() {
            6 | 8 => {
                let v = u32::from_str_radix(&hex, 16).unwrap_or(0);
                if hex.len() == 6 {
                    [(v >> 16) as u8, (v >> 8) as u8, v as u8, 0xFF]
                } else {
                    [(v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8]
                }
            }
            3 => {
                let d: Vec<u8> = hex.chars().map(|c| c.to_digit(16).unwrap_or(0) as u8 * 17).collect();
                [d[0], d[1], d[2], 0xFF]
            }
            _ => {
                self.error("G0005", format!("bad color `#{hex}` (want #RRGGBB or #RRGGBBAA, alpha last)"));
                [0, 0, 0, 0xFF]
            }
        };
        let opacity = self.opacity_suffix();
        let pos = self.pos_suffix();
        self.push(Tok::Color { rgba, opacity, pos });
    }

    fn reference(&mut self) {
        self.pos += 1;
        if !is_ident_start(self.peek()) {
            self.error("G0006", "`$` must be followed by a name".to_string());
            return;
        }
        let name = self.dotted_word();
        let opacity = self.opacity_suffix();
        self.push(Tok::Ref { name, opacity });
    }
}
