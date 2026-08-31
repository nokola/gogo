//! The parse tree, one struct per spec "Node anatomy": type, name, styles,
//! flags, args, props, params, children, line. Directives (`style`,
//! `theme`, `when`, `transition`, …) are ordinary nodes here; `compile.rs`
//! gives them meaning.

#[derive(Clone, Debug, PartialEq)]
pub enum AstValue {
    Bool(bool),
    Num { value: f32, unit: String },
    Color { rgba: [u8; 4], opacity: Option<f32>, pos: Option<f32> },
    /// `line` is the string's OWN source line — an `areas=` row string
    /// must point its errors at itself, not at the `areas=` word
    /// (GPAR99).
    Str { text: String, key: Option<String>, line: u32 },
    Ref { name: String, opacity: Option<f32> },
    Ident { word: String, opacity: Option<f32>, pos: Option<f32> },
    Size { w: Option<f32>, h: Option<f32> },
    Call { name: String, args: Vec<AstValue> },
    List(Vec<AstValue>),
    /// `a..b` (`in $min..$max`).
    Range(Box<AstValue>, Box<AstValue>),
}

impl AstValue {
    pub fn as_ident(&self) -> Option<&str> {
        match self {
            AstValue::Ident { word, .. } => Some(word),
            _ => None,
        }
    }
    pub fn as_num(&self) -> Option<f32> {
        match self {
            AstValue::Num { value, .. } => Some(*value),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            AstValue::Str { text, .. } => Some(text),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AstProp {
    pub key: String,
    pub value: AstValue,
    pub line: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AstNode {
    pub type_word: String,
    pub name: Option<String>,
    pub styles: Vec<String>,
    pub flags: Vec<String>,
    pub args: Vec<AstValue>,
    pub props: Vec<AstProp>,
    /// `$name=value` declarations (widget params).
    pub params: Vec<AstProp>,
    /// `-> endpoint` words (transition rules).
    pub arrows: Vec<String>,
    /// `as $b` / `by $b.field` (repeat lines).
    pub clauses: Vec<(String, AstValue)>,
    pub children: Vec<AstItem>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AstItem {
    Node(AstNode),
    /// `$name = value` (theme value) or `node.prop = value` (a `when`
    /// response); `dollar` tells which.
    Assign { key: String, dollar: bool, value: AstValue, line: u32 },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub items: Vec<AstItem>,
}
