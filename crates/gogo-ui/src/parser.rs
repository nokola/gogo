//! The grammar from `gogo-spec.md` (EBNF). Commas separate members;
//! a trailing comma continues the line; `{` must sit on the node's line.

use crate::ast::{AstItem, AstNode, AstProp, AstValue, Document};
use crate::diag::Diagnostic;
use crate::lexer::{Tok, Token, lex};

/// Reserved words that are never a node name.
const FLAGS: &[&str] = &["signals"];
const CLAUSES: &[&str] = &["as", "by", "in"];

pub fn parse(source: &str, diags: &mut Vec<Diagnostic>) -> Document {
    let tokens = lex(source, diags);
    let mut p = Parser { tokens, pos: 0, diags };
    let items = p.items(false);
    Document { items }
}

struct Parser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    diags: &'a mut Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &Tok {
        &self.tokens[self.pos.min(self.tokens.len() - 1)].tok
    }

    fn line(&self) -> u32 {
        self.tokens[self.pos.min(self.tokens.len() - 1)].line
    }

    fn next(&mut self) -> Tok {
        let t = self.tokens[self.pos.min(self.tokens.len() - 1)].tok.clone();
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn error(&mut self, code: &str, message: String) {
        let line = self.line();
        self.diags.push(Diagnostic::error(code, line, message));
    }

    fn skip_newlines(&mut self) {
        while *self.peek() == Tok::Newline {
            self.pos += 1;
        }
    }

    /// Items until `}` (inside a block) or EOF.
    fn items(&mut self, in_block: bool) -> Vec<AstItem> {
        let mut items = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                Tok::Eof => {
                    if in_block {
                        self.error("G0010", "missing `}`".to_string());
                    }
                    return items;
                }
                Tok::RBrace => {
                    if in_block {
                        self.pos += 1;
                    } else {
                        self.error("G0011", "unexpected `}`".to_string());
                        self.pos += 1;
                        continue;
                    }
                    return items;
                }
                _ => {}
            }
            if let Some(item) = self.item() {
                items.push(item);
            }
        }
    }

    fn item(&mut self) -> Option<AstItem> {
        let line = self.line();
        match self.next() {
            Tok::Ref { name, .. } => {
                // `$name = value`
                if self.next() != Tok::Eq {
                    self.error("G0012", format!("`${name}` needs `= value` here"));
                    self.recover();
                    return None;
                }
                let value = self.value()?;
                self.terminator();
                Some(AstItem::Assign { key: name, dollar: true, value, line })
            }
            Tok::Ident { word, .. } => {
                if *self.peek() == Tok::Eq {
                    // `node.prop = value` (a when response) or `prop = value`.
                    self.pos += 1;
                    let value = self.value()?;
                    self.terminator();
                    return Some(AstItem::Assign { key: word, dollar: false, value, line });
                }
                self.node(word, line).map(AstItem::Node)
            }
            other => {
                self.error("G0013", format!("expected a node type or `$value`, found {}", describe(&other)));
                self.recover();
                None
            }
        }
    }

    /// Skips to the end of the line (error recovery).
    fn recover(&mut self) {
        loop {
            match self.peek() {
                Tok::Newline | Tok::Eof => return,
                Tok::RBrace => return,
                Tok::LBrace => {
                    // Skip the whole block so its children don't parse as siblings.
                    let mut depth = 0;
                    loop {
                        match self.next() {
                            Tok::LBrace => depth += 1,
                            Tok::RBrace => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            Tok::Eof => return,
                            _ => {}
                        }
                    }
                    return;
                }
                _ => self.pos += 1,
            }
        }
    }

    fn terminator(&mut self) {
        match self.peek() {
            Tok::Newline => self.pos += 1,
            Tok::Eof | Tok::RBrace => {}
            other => {
                let msg = format!("expected end of line, found {}", describe(other));
                self.error("G0014", msg);
                self.recover();
            }
        }
    }

    fn node(&mut self, type_word: String, line: u32) -> Option<AstNode> {
        let mut node = AstNode { type_word, line, ..Default::default() };
        let mut need_comma = false;
        loop {
            match self.peek().clone() {
                Tok::Comma => {
                    self.pos += 1;
                    need_comma = false;
                    // A trailing comma continues onto the next line.
                    if *self.peek() == Tok::Newline {
                        self.skip_newlines();
                        if *self.peek() == Tok::LBrace {
                            self.error("G0015", "a comma before `{` is not allowed".to_string());
                        }
                    }
                    continue;
                }
                Tok::Newline if self.continues_after_name(&node) => {
                    self.skip_newlines();
                }
                Tok::Newline | Tok::Eof | Tok::RBrace => break,
                Tok::LBrace => {
                    self.pos += 1;
                    node.children = self.items(true);
                    break;
                }
                Tok::Style(s) => {
                    self.pos += 1;
                    node.styles.push(s);
                    need_comma = false;
                }
                Tok::Arrow => {
                    self.pos += 1;
                    match self.next() {
                        Tok::Ident { word, .. } => node.arrows.push(word),
                        other => {
                            self.error("G0016", format!("`->` needs a state name, found {}", describe(&other)));
                        }
                    }
                    need_comma = false;
                }
                Tok::Ident { word, opacity, pos } => {
                    if CLAUSES.contains(&word.as_str()) {
                        self.pos += 1;
                        let Some(v) = self.value() else { break };
                        node.clauses.push((word, v));
                        need_comma = false;
                        continue;
                    }
                    // `then group drop …` / `and play appear`: the word after the
                    // connective's step word is that step's own name.
                    let step_name = matches!(node.type_word.as_str(), "and" | "then") && matches!(node.name.as_deref(), Some("group") | Some("each") | Some("play")) && node.args.is_empty() && node.props.is_empty() && self.tokens.get(self.pos + 1).map(|t| t.tok != Tok::Eq).unwrap_or(true);
                    if need_comma && !step_name {
                        self.error("G0017", format!("missing `,` before `{word}`"));
                    }
                    self.pos += 1;
                    if step_name {
                        node.args.push(AstValue::Ident { word, opacity, pos });
                        need_comma = false;
                        continue;
                    }
                    if *self.peek() == Tok::Eq {
                        self.pos += 1;
                        let Some(value) = self.value() else { break };
                        node.props.push(AstProp { key: word, value, line: self.line() });
                    } else if FLAGS.contains(&word.as_str()) {
                        node.flags.push(word);
                    } else if *self.peek() == Tok::LParen {
                        node.args.push(self.call(word));
                    } else if node.name.is_none() && node.args.is_empty() && node.props.is_empty() && opacity.is_none() && pos.is_none() && !is_value_word(&node.type_word, &word) {
                        // The name: no comma after it (`Button ok .Tool text="x"`).
                        node.name = Some(word);
                        continue;
                    } else {
                        node.args.push(AstValue::Ident { word, opacity, pos });
                    }
                    need_comma = true;
                }
                Tok::Ref { name, opacity } => {
                    if need_comma {
                        self.error("G0017", format!("missing `,` before `${name}`"));
                    }
                    self.pos += 1;
                    if *self.peek() == Tok::Eq {
                        self.pos += 1;
                        let Some(value) = self.value() else { break };
                        node.params.push(AstProp { key: name, value, line: self.line() });
                    } else {
                        node.args.push(AstValue::Ref { name, opacity });
                    }
                    need_comma = true;
                }
                _ => {
                    if need_comma {
                        let d = describe(self.peek());
                        self.error("G0017", format!("missing `,` before {d}"));
                    }
                    let Some(v) = self.value() else { break };
                    node.args.push(v);
                    need_comma = true;
                }
            }
        }
        if *self.peek() == Tok::Newline {
            self.pos += 1;
        }
        // Duplicate property on one node = parse error.
        for i in 0..node.props.len() {
            if node.props[..i].iter().any(|p| p.key == node.props[i].key) {
                let key = node.props[i].key.clone();
                self.diags.push(Diagnostic::error("G0018", line, format!("duplicate property `{key}`")));
            }
        }
        Some(node)
    }

    /// `Layers wheelTouchTarget` alone on its line, members on the next
    /// (the Godot files write this): the line continues when the next line
    /// opens with a size or a `key=` member — never with a node type.
    fn continues_after_name(&self, node: &AstNode) -> bool {
        if node.name.is_none() || !node.args.is_empty() || !node.props.is_empty() || !node.styles.is_empty() {
            return false;
        }
        let mut i = self.pos;
        while self.tokens.get(i).map(|t| t.tok == Tok::Newline).unwrap_or(false) {
            i += 1;
        }
        match self.tokens.get(i).map(|t| &t.tok) {
            Some(Tok::Size { .. }) => true,
            Some(Tok::Ident { .. }) => self.tokens.get(i + 1).map(|t| t.tok == Tok::Eq).unwrap_or(false),
            _ => false,
        }
    }

    fn call(&mut self, name: String) -> AstValue {
        self.pos += 1; // (
        let mut args = Vec::new();
        loop {
            match self.peek() {
                Tok::RParen => {
                    self.pos += 1;
                    break;
                }
                Tok::Comma => self.pos += 1,
                Tok::Eof | Tok::Newline => {
                    self.error("G0019", format!("missing `)` in `{name}(`"));
                    break;
                }
                _ => match self.value() {
                    Some(v) => args.push(v),
                    None => break,
                },
            }
        }
        AstValue::Call { name, args }
    }

    fn value(&mut self) -> Option<AstValue> {
        let v = self.value_one()?;
        if *self.peek() == Tok::DotDot {
            self.pos += 1;
            let end = self.value_one()?;
            return Some(AstValue::Range(Box::new(v), Box::new(end)));
        }
        Some(v)
    }

    fn value_one(&mut self) -> Option<AstValue> {
        let t = self.next();
        Some(match t {
            Tok::Ident { word, opacity, pos } => {
                if word == "true" {
                    AstValue::Bool(true)
                } else if word == "false" {
                    AstValue::Bool(false)
                } else if *self.peek() == Tok::LParen {
                    self.call(word)
                } else {
                    AstValue::Ident { word, opacity, pos }
                }
            }
            Tok::Num { value, unit } => AstValue::Num { value, unit },
            Tok::Size { w, h } => AstValue::Size { w, h },
            Tok::Color { rgba, opacity, pos } => AstValue::Color { rgba, opacity, pos },
            Tok::Str { text, key } => AstValue::Str { text, key },
            Tok::Ref { name, opacity } => AstValue::Ref { name, opacity },
            Tok::LBracket => {
                let mut items = Vec::new();
                loop {
                    self.skip_newlines();
                    match self.peek() {
                        Tok::RBracket => {
                            self.pos += 1;
                            break;
                        }
                        Tok::Comma => self.pos += 1,
                        Tok::Eof => {
                            self.error("G0020", "missing `]`".to_string());
                            break;
                        }
                        _ => match self.value() {
                            Some(v) => items.push(v),
                            None => break,
                        },
                    }
                }
                AstValue::List(items)
            }
            other => {
                self.error("G0021", format!("expected a value, found {}", describe(&other)));
                self.recover();
                return None;
            }
        })
    }
}

/// Words that are values, never node names, in a positional slot
/// (`gap fill`, `transition any`). `fill`/`hug` are contextual: after a
/// control type they are a plain name (`Rectangle fill 26x26`).
fn is_value_word(type_word: &str, word: &str) -> bool {
    matches!(word, "any" | "base") || (type_word == "gap" && matches!(word, "fill" | "hug"))
}

fn describe(tok: &Tok) -> String {
    match tok {
        Tok::Ident { word, .. } => format!("`{word}`"),
        Tok::Style(s) => format!("`.{s}`"),
        Tok::Ref { name, .. } => format!("`${name}`"),
        Tok::Str { text, .. } => format!("\"{text}\""),
        Tok::Num { value, .. } => format!("`{value}`"),
        Tok::Size { .. } => "a size".to_string(),
        Tok::Color { .. } => "a color".to_string(),
        Tok::Arrow => "`->`".to_string(),
        Tok::DotDot => "`..`".to_string(),
        Tok::Comma => "`,`".to_string(),
        Tok::Eq => "`=`".to_string(),
        Tok::LBrace => "`{`".to_string(),
        Tok::RBrace => "`}`".to_string(),
        Tok::LBracket => "`[`".to_string(),
        Tok::RBracket => "`]`".to_string(),
        Tok::LParen => "`(`".to_string(),
        Tok::RParen => "`)`".to_string(),
        Tok::Newline => "end of line".to_string(),
        Tok::Eof => "end of file".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(src: &str) -> Document {
        let mut diags = Vec::new();
        let d = parse(src, &mut diags);
        assert!(diags.is_empty(), "{diags:?}");
        d
    }

    fn first(d: &Document) -> &AstNode {
        match &d.items[0] {
            AstItem::Node(n) => n,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn spec_example_parses_to_the_expected_shape() {
        let d = doc(r#"
// styles/theme.gogo
$accent        = #E51400FF
theme dark {
    $accent = #FF6B4AFF   // active via set_theme_variant("dark")
}
style ToolButton min_height=36, tap_pulse=true
widget LabeledSlider $label="", $min=0, $max=100, $value=0 {
    Column spacing=4 {
        Label text=$label
        Slider min=$min, max=$max, value=$value
    }
}
Panel visibility=gone, halign=center, valign=bottom, margin_bottom=72 {
    Column spacing=8, min_width=240 {
        LabeledSlider sizeSlider label="Size"/size, min=1, max=300, value=80
        Layers preview size=220x110
    }
}
"#);
        assert_eq!(d.items.len(), 5);
        assert!(matches!(&d.items[0], AstItem::Assign { key, dollar: true, .. } if key == "accent"));
        let theme = match &d.items[1] {
            AstItem::Node(n) => n,
            _ => panic!(),
        };
        assert_eq!(theme.type_word, "theme");
        assert_eq!(theme.name.as_deref(), Some("dark"));
        assert_eq!(theme.children.len(), 1);
        let widget = match &d.items[3] {
            AstItem::Node(n) => n,
            _ => panic!(),
        };
        assert_eq!(widget.params.len(), 4);
        let panel = match &d.items[4] {
            AstItem::Node(n) => n,
            _ => panic!(),
        };
        assert_eq!(panel.type_word, "Panel");
        assert_eq!(panel.name, None);
        assert_eq!(panel.props.len(), 4);
        let column = match &panel.children[0] {
            AstItem::Node(n) => n,
            _ => panic!(),
        };
        let slider = match &column.children[0] {
            AstItem::Node(n) => n,
            _ => panic!(),
        };
        assert_eq!(slider.name.as_deref(), Some("sizeSlider"));
        assert_eq!(slider.props[0].value, AstValue::Str { text: "Size".into(), key: Some("size".into()) });
        let preview = match &column.children[1] {
            AstItem::Node(n) => n,
            _ => panic!(),
        };
        assert_eq!(preview.props[0].value, AstValue::Size { w: Some(220.0), h: Some(110.0) });
    }

    #[test]
    fn members_in_canonical_order_and_line_continuation() {
        let d = doc("Button okButton .ToolButton halign=center, text=\"OK\",\n    min_width=120, on:activate=save { Label x text=\"a\" }\n");
        let b = first(&d);
        assert_eq!(b.name.as_deref(), Some("okButton"));
        assert_eq!(b.styles, vec!["ToolButton"]);
        assert_eq!(b.props.len(), 4);
        assert_eq!(b.props[3].key, "on:activate");
        assert_eq!(b.children.len(), 1);
    }

    #[test]
    fn structure_words_and_transitions() {
        let d = doc("Column {\n gap 12\n gap fill\n transition any -> held duration=0.1, easing=cubic_out\n when held background=#FFFFFF22\n}\n");
        let c = first(&d);
        assert_eq!(c.children.len(), 4);
        let gap = match &c.children[0] {
            AstItem::Node(n) => n,
            _ => panic!(),
        };
        assert_eq!(gap.args[0], AstValue::Num { value: 12.0, unit: String::new() });
        let gapfill = match &c.children[1] {
            AstItem::Node(n) => n,
            _ => panic!(),
        };
        assert_eq!(gapfill.args[0].as_ident(), Some("fill"));
        let tr = match &c.children[2] {
            AstItem::Node(n) => n,
            _ => panic!(),
        };
        assert_eq!(tr.args[0].as_ident(), Some("any"));
        assert_eq!(tr.arrows, vec!["held"]);
        let when = match &c.children[3] {
            AstItem::Node(n) => n,
            _ => panic!(),
        };
        assert_eq!(when.name.as_deref(), Some("held"));
    }

    #[test]
    fn when_block_assignments_and_lists() {
        let d = doc("when wide {\n toolbar.width = 300\n}\nGrid cols=[hug, fill(2), 60] {\n row { Label a; skip }\n}\n");
        let w = first(&d);
        assert!(matches!(&w.children[0], AstItem::Assign { key, dollar: false, .. } if key == "toolbar.width"));
        let g = match &d.items[1] {
            AstItem::Node(n) => n,
            _ => panic!(),
        };
        match &g.props[0].value {
            AstValue::List(items) => {
                assert_eq!(items.len(), 3);
                assert!(matches!(&items[1], AstValue::Call { name, args } if name == "fill" && args.len() == 1));
            }
            other => panic!("{other:?}"),
        }
        let row = match &g.children[0] {
            AstItem::Node(n) => n,
            _ => panic!(),
        };
        assert_eq!(row.children.len(), 2);
    }

    #[test]
    fn duplicate_prop_and_bad_char_are_errors() {
        let mut diags = Vec::new();
        parse("Label a text=\"x\", text=\"y\"\nLabel b min-width=3\n", &mut diags);
        assert!(diags.iter().any(|d| d.code == "G0018"));
        assert!(diags.iter().any(|d| d.code == "G0001"), "{diags:?}");
    }
}
