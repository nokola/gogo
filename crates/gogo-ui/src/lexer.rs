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
    /// `animals[12]` — a word with a whole number in brackets attached
    /// (no space): one cell of a grid atlas.
    Index { word: String, index: u32 },
    /// `.Style` (a dot after whitespace).
    Style(String),
    /// `$name` with an optional attached path (`$task.id`) and `/opacity`.
    Ref { name: String, opacity: Option<f32>, pos: Option<f32> },
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
                    let h = self.digits().unwrap_or(0.0);
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

    /// `[12]` right after `word`, up to and including the `]`.
    fn index(&mut self, word: String) {
        self.pos += 1;
        let start = self.pos;
        while self.peek().is_ascii_digit() {
            self.pos += 1;
        }
        let digits: String = self.chars[start..self.pos].iter().collect();
        if digits.is_empty() || self.peek() != ']' {
            self.error("G2016", format!("`{word}[` — an atlas cell is a whole number in brackets right after the atlas name: `{word}[12]`"));
            // Skip to the `]` on this line so the rest of the line still parses.
            while !matches!(self.peek(), ']' | '\n' | '\0') {
                self.pos += 1;
            }
            if self.peek() == ']' {
                self.pos += 1;
            }
            self.push(Tok::Ident { word, opacity: None, pos: None });
            return;
        }
        self.pos += 1;
        match digits.parse::<u32>() {
            Ok(index) => self.push(Tok::Index { word, index }),
            Err(_) => {
                self.error("G2016", format!("`{word}[{digits}]` — the cell number is too large"));
                self.push(Tok::Ident { word, opacity: None, pos: None });
            }
        }
    }

    /// A word with attached `.path` segments and `:` (prop keys like
    /// `on:activate`), and a closing `.{a, b}` group — the property list
    /// of a `transition` target (`commandBar.{margin, opacity}`), kept in
    /// the word as `commandBar.{margin,opacity}` for the compiler to split.
    fn dotted_word(&mut self) -> String {
        let mut word = self.word();
        loop {
            let c = self.peek();
            if (c == '.' || c == ':') && is_ident_start(self.peek_at(1)) {
                self.pos += 1;
                word.push(c);
                word.push_str(&self.word());
            } else if c == '.' && self.peek_at(1) == '{' {
                self.pos += 2;
                word.push_str(".{");
                self.property_group(&mut word);
                break;
            } else {
                break;
            }
        }
        word
    }

    /// The inside of a `.{a, b}` group, up to and including its `}`: the
    /// names comma-joined without spaces. The group stays on one line —
    /// a `{` that never closes there would swallow the next line's block.
    fn property_group(&mut self, word: &mut String) {
        let mut names: usize = 0;
        loop {
            while self.peek() == ' ' || self.peek() == '\t' {
                self.pos += 1;
            }
            let c = self.peek();
            if is_ident_start(c) {
                if names > 0 && !word.ends_with(',') {
                    self.error("G0001", "missing `,` between the names in `.{…}`".to_string());
                    word.push(',');
                }
                word.push_str(&self.word());
                names += 1;
            } else if c == ',' {
                self.pos += 1;
                word.push(',');
            } else if c == '}' {
                self.pos += 1;
                if names == 0 {
                    self.error("G0001", "`.{}` names no property — write `node.{opacity, margin}`".to_string());
                }
                word.push('}');
                return;
            } else {
                self.error("G0001", "a `.{…}` property group closes with `}` on its own line — `node.{margin, opacity}`".to_string());
                word.push('}');
                return;
            }
        }
    }

    /// The number at the cursor: digits with at most one `.`, a digit
    /// first. `1.2.3` and `.5` are errors (G0009), never a silent 0 — a
    /// length that reads as zero is invisible on screen. `0..1` stops
    /// before the range dots.
    fn digits(&mut self) -> Option<f32> {
        let start = self.pos;
        while self.peek().is_ascii_digit() || (self.peek() == '.' && self.peek_at(1) != '.') {
            self.pos += 1;
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        let shape_ok = text.starts_with(|c: char| c.is_ascii_digit()) && text.matches('.').count() <= 1;
        match text.parse::<f32>() {
            Ok(v) if shape_ok => Some(v),
            _ => {
                let hint = if text.starts_with('.') { format!(" — write `0{text}`") } else { String::new() };
                self.error("G0009", format!("bad number `{text}`{hint}"));
                None
            }
        }
    }

    /// A `/` or `@` suffix's number, which must sit in 0..1: `black/50`
    /// used to read as opaque black and `white@2` as a stop past the end,
    /// both silently. `None` = refused (already reported).
    fn unit_suffix(&mut self, what: &str, example: &str) -> Option<f32> {
        let start = self.pos;
        let mark = self.chars[start - 1];
        let v = self.digits()?;
        if (0.0..=1.0).contains(&v) {
            return Some(v);
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        self.error(if mark == '/' { "G0007" } else { "G0008" }, format!("`{mark}{text}` — {what} is 0..1 after `{mark}` ({example})"));
        None
    }

    /// `/0.5` after a value.
    fn opacity_suffix(&mut self) -> Option<f32> {
        if self.peek() == '/' && (self.peek_at(1).is_ascii_digit() || self.peek_at(1) == '.') {
            self.pos += 1;
            return self.unit_suffix("an opacity", "`black/0.5`");
        }
        None
    }

    /// `@0.24` after a color.
    fn pos_suffix(&mut self) -> Option<f32> {
        if self.peek() == '@' && (self.peek_at(1).is_ascii_digit() || self.peek_at(1) == '.') {
            self.pos += 1;
            return self.unit_suffix("a stop position", "`white@0.24`");
        }
        None
    }

    fn ident(&mut self) {
        let word = self.dotted_word();
        // `animals[12]`: the bracket touching the word makes it an index,
        // never a list (a list always follows `=`, `,` or a space).
        if self.peek() == '[' {
            self.index(word);
            return;
        }
        let opacity = self.opacity_suffix();
        let pos = self.pos_suffix();
        self.push(Tok::Ident { word, opacity, pos });
    }

    fn number(&mut self) {
        let negative = self.peek() == '-';
        if negative {
            self.pos += 1;
        }
        let value = self.digits().unwrap_or(0.0) * if negative { -1.0 } else { 1.0 };
        if self.peek() == 'x' && !negative {
            // `220x110` / `220x`.
            self.pos += 1;
            let h = if self.peek().is_ascii_digit() { Some(self.digits().unwrap_or(0.0)) } else { None };
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
                self.error("G0079", format!("unknown unit `{w}` on a number (epx, px or % only)"));
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
            // One hex form (decided 2026-08-31, E4): `#F00` is refused so a
            // color has one spelling to grep for; the message writes it out.
            3 => {
                let full: String = hex.chars().flat_map(|c| [c, c]).collect();
                self.error("G0005", format!("`#{hex}` — one hex form: `#RRGGBB` or `#RRGGBBAA` (alpha last); write `#{full}`"));
                [0, 0, 0, 0xFF]
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
        // `$edge@0.24` — a theme color as a gradient stop (GPAR86).
        let pos = self.pos_suffix();
        self.push(Tok::Ref { name, opacity, pos });
    }
}
