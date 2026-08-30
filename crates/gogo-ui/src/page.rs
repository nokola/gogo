//! The compiled page: a flat arena of nodes plus pools (strings, props,
//! values, whens, handlers). Needed in this shape so `build.rs` can emit a
//! page as `static` slices and boot copies them in (`Page::from_static`)
//! with no parsing; hot reload builds the same `Page` from text. Every
//! type here is `Copy` + `Debug`, and `Debug` output IS the emitted Rust
//! literal (see `emit.rs`) — keep variant names unique across enums.

pub type NodeId = u32;
pub type StrId = u32;
/// No node (a root's parent).
pub const NO_NODE: NodeId = u32::MAX;
/// The empty string; `name == NO_STR` = unnamed.
pub const NO_STR: StrId = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    View,
    Panel,
    Column,
    Row,
    Flow,
    Layers,
    Scroll,
    Grid,
    Tiles,
    Label,
    Button,
    Image,
    Icon,
    Rectangle,
    Circle,
    Gradient,
    Slider,
    Input,
    Check,
    Toggle,
    Radio,
    Choice,
    List,
    ListItem,
    Repeat,
    /// `gap N` / `gap fill` inside a Column/Row.
    Gap,
    /// `row { }` inside a Grid.
    GridRow,
    /// `skip` inside a Grid row.
    Skip,
    /// A widget's named `slot` region: a free, clipped box the instance
    /// fills (or that shows its default content).
    Slot,
    /// `seat <name>` — an empty socket a `when` moves a node into.
    Seat,
    /// A repeat's `empty { }` content (shown with no items — always, here).
    Empty,
    /// A widget instance whose body was not expanded (unknown type).
    Custom,
}

/// `automatic=` on a slot line, solved at compile time (no code-behind
/// drives `$value`, so the fraction is a number): the geometry a value
/// displays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Auto {
    NoAuto,
    /// Main-axis extent = fraction of the host.
    Length(f32),
    /// Seated along the main axis at the fraction (own width from content).
    Position(f32),
    /// Content exists iff the named state holds.
    Presence(bool),
}

impl Kind {
    pub fn from_word(word: &str) -> Option<Kind> {
        Some(match word {
            "View" => Kind::View,
            "Panel" => Kind::Panel,
            "Column" => Kind::Column,
            "Row" => Kind::Row,
            "Flow" => Kind::Flow,
            "Layers" => Kind::Layers,
            "Scroll" => Kind::Scroll,
            "Grid" => Kind::Grid,
            "Tiles" => Kind::Tiles,
            "Label" => Kind::Label,
            "Button" => Kind::Button,
            "Image" => Kind::Image,
            "Icon" => Kind::Icon,
            "Rectangle" => Kind::Rectangle,
            "Circle" => Kind::Circle,
            "Gradient" => Kind::Gradient,
            "Slider" => Kind::Slider,
            "Input" => Kind::Input,
            "Check" => Kind::Check,
            "Toggle" => Kind::Toggle,
            "Radio" => Kind::Radio,
            "choice" => Kind::Choice,
            "List" => Kind::List,
            "ListItem" => Kind::ListItem,
            "repeat" => Kind::Repeat,
            "gap" => Kind::Gap,
            "row" => Kind::GridRow,
            "skip" => Kind::Skip,
            "slot" => Kind::Slot,
            "seat" => Kind::Seat,
            "empty" => Kind::Empty,
            _ => return None,
        })
    }

    /// Children stack on one axis (Column/Row) — `gap` is legal here.
    pub fn is_stack(self) -> bool {
        matches!(self, Kind::Column | Kind::Row)
    }

    /// Children place freely (halign/valign + margins).
    pub fn is_free(self) -> bool {
        matches!(self, Kind::View | Kind::Layers | Kind::Panel | Kind::Button | Kind::ListItem | Kind::Slot | Kind::Seat | Kind::Empty)
    }

    /// A press surface: `held` / `activate` live here.
    pub fn is_press_surface(self) -> bool {
        matches!(self, Kind::Button | Kind::Check | Kind::Toggle | Kind::Radio | Kind::ListItem)
    }
}

/// One axis of the sizing triad. `Fill`/`Hug` carry a weight (`fill(2)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Len {
    Unset,
    Exact(f32),
    Fill(f32),
    Hug(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    AlignUnset,
    Start,
    Center,
    End,
}

/// Where and how big, per the spec's Placement section. Margins/caps are
/// `None` when the file did not speak them (the difference between "inset
/// 0" and "unbound" matters for pins).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub halign: Align,
    pub valign: Align,
    pub width: Len,
    pub height: Len,
    pub min_width: f32,
    pub min_height: f32,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    /// `max_width=fill` / `max_height=fill`.
    pub max_width_fill: bool,
    pub max_height_fill: bool,
    /// left, top, right, bottom.
    pub margin: [Option<f32>; 4],
    /// left, top, right, bottom — air inside the node's own box.
    pub padding: [f32; 4],
    /// Per edge: `padding_<edge>=safe` — the platform's inset is added.
    pub padding_safe: [bool; 4],
    /// Per edge: `margin_<edge>=safe`.
    pub margin_safe: [bool; 4],
}

impl Placement {
    pub const DEFAULT: Placement = Placement {
        halign: Align::AlignUnset,
        valign: Align::AlignUnset,
        width: Len::Unset,
        height: Len::Unset,
        min_width: 0.0,
        min_height: 0.0,
        max_width: None,
        max_height: None,
        max_width_fill: false,
        max_height_fill: false,
        margin: [None; 4],
        padding: [0.0; 4],
        padding_safe: [false; 4],
        margin_safe: [false; 4],
    };
}

impl Default for Placement {
    fn default() -> Placement {
        Placement::DEFAULT
    }
}

/// A property key. Known keys are variants so layout/draw match on them;
/// anything else is `Other(name)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropKey {
    Text,
    Background,
    Color,
    FontSize,
    Spacing,
    CornerRadius,
    BorderThickness,
    BorderColor,
    Opacity,
    Visibility,
    Disabled,
    /// `source=` on an Image: the file path, resolved by the build against
    /// its asset root (`BuildOptions::res_root`).
    Source,
    IconName,
    Value,
    Min,
    Max,
    Cols,
    RowHeight,
    ColSpan,
    RowSpan,
    Fit,
    TextHalign,
    TextValign,
    Wrap,
    Allow,
    Bars,
    ToggleMode,
    /// `selected=true` (Check/Toggle/Radio). Named apart from `State::Selected`: the codegen glob-imports both enums.
    SelectedProp,
    /// `fill=linear|radial` on a Gradient (apart from `Len::Fill`).
    GradientFill,
    Stops,
    Angle,
    Aspect,
    /// Font file key (`font="fonts/Light.ttf"`), inherited down the tree.
    Font,
    /// Text color (`font_color=`), the text half of `background=`.
    FontColor,
    /// `color.a` — a Rectangle/Circle fill's alpha alone (whens/anims).
    ColorA,
    ClipText,
    ClipContents,
    /// Tiles cell props.
    Span,
    FullRow,
    /// Host-read keys: the compiler accepts them and no gogo pass reads
    /// them; the host program reads them off the page (`UNREAD_BY_DESIGN`
    /// says which and why).
    Loading,
    TouchTarget,
    TapPulse,
    AccessibilityName,
    KeyboardTarget,
    Select,
    Render,
    Step,
    IconSrc,
    Automatic,
    /// `x.seat = <name>` in a `when`: the socket the node moves into (apart from `Kind::Seat`).
    SeatOf,
    /// `overlay=true`: the box sits on its parent without sizing it.
    Overlay,
    /// `interactive=true/false`: claims input / hit-invisible (no restyle).
    Interactive,
    /// Motion words as standing `when` responses.
    Scale,
    ScaleX,
    ScaleY,
    TranslateX,
    TranslateY,
    Rotate,
    /// Placement keys, so `when` blocks can carry them.
    Width,
    Height,
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
    Halign,
    Valign,
    MarginLeft,
    MarginTop,
    MarginRight,
    MarginBottom,
    Margin,
    PaddingLeft,
    PaddingTop,
    PaddingRight,
    PaddingBottom,
    Padding,
    Size,
    MinSize,
    Other(StrId),
}

impl PropKey {
    pub fn from_word(word: &str) -> Option<PropKey> {
        Some(match word {
            "text" => PropKey::Text,
            "background" => PropKey::Background,
            "color" => PropKey::Color,
            "font_size" => PropKey::FontSize,
            "spacing" => PropKey::Spacing,
            "corner_radius" => PropKey::CornerRadius,
            "border_thickness" => PropKey::BorderThickness,
            "border_color" => PropKey::BorderColor,
            "opacity" => PropKey::Opacity,
            "visibility" => PropKey::Visibility,
            "disabled" => PropKey::Disabled,
            "source" => PropKey::Source,
            "name" => PropKey::IconName,
            "value" => PropKey::Value,
            "min" => PropKey::Min,
            "max" => PropKey::Max,
            "cols" => PropKey::Cols,
            "row_height" => PropKey::RowHeight,
            "col_span" => PropKey::ColSpan,
            "row_span" => PropKey::RowSpan,
            "fit" => PropKey::Fit,
            "horizontal_alignment" => PropKey::TextHalign,
            "vertical_alignment" => PropKey::TextValign,
            "wrap" => PropKey::Wrap,
            "allow" => PropKey::Allow,
            "bars" => PropKey::Bars,
            "toggle" => PropKey::ToggleMode,
            "selected" => PropKey::SelectedProp,
            "fill" => PropKey::GradientFill,
            "stops" => PropKey::Stops,
            "angle" => PropKey::Angle,
            "aspect" => PropKey::Aspect,
            "font" => PropKey::Font,
            "font_color" => PropKey::FontColor,
            "color.a" => PropKey::ColorA,
            "modulate.a" => PropKey::Opacity,
            "clip_text" => PropKey::ClipText,
            "clip_contents" => PropKey::ClipContents,
            "span" => PropKey::Span,
            "full_row" => PropKey::FullRow,
            "loading" => PropKey::Loading,
            "touch_target" => PropKey::TouchTarget,
            "tap_pulse" => PropKey::TapPulse,
            "accessibility_name" => PropKey::AccessibilityName,
            "keyboard_target" => PropKey::KeyboardTarget,
            "select" => PropKey::Select,
            "render" => PropKey::Render,
            "step" => PropKey::Step,
            "icon" => PropKey::IconSrc,
            "automatic" => PropKey::Automatic,
            "seat" => PropKey::SeatOf,
            "overlay" => PropKey::Overlay,
            "interactive" => PropKey::Interactive,
            "scale" => PropKey::Scale,
            "scale.x" => PropKey::ScaleX,
            "scale.y" => PropKey::ScaleY,
            "translate.x" => PropKey::TranslateX,
            "translate.y" => PropKey::TranslateY,
            "rotate" => PropKey::Rotate,
            "width" => PropKey::Width,
            "height" => PropKey::Height,
            "min_width" => PropKey::MinWidth,
            "min_height" => PropKey::MinHeight,
            "max_width" => PropKey::MaxWidth,
            "max_height" => PropKey::MaxHeight,
            "halign" => PropKey::Halign,
            "valign" => PropKey::Valign,
            "margin_left" => PropKey::MarginLeft,
            "margin_top" => PropKey::MarginTop,
            "margin_right" => PropKey::MarginRight,
            "margin_bottom" => PropKey::MarginBottom,
            "margin" => PropKey::Margin,
            "padding_left" => PropKey::PaddingLeft,
            "padding_top" => PropKey::PaddingTop,
            "padding_right" => PropKey::PaddingRight,
            "padding_bottom" => PropKey::PaddingBottom,
            "padding" => PropKey::Padding,
            "size" => PropKey::Size,
            "min_size" => PropKey::MinSize,
            _ => return None,
        })
    }

    pub fn is_placement(self) -> bool {
        matches!(
            self,
            PropKey::Width
                | PropKey::Height
                | PropKey::MinWidth
                | PropKey::MinHeight
                | PropKey::MaxWidth
                | PropKey::MaxHeight
                | PropKey::Halign
                | PropKey::Valign
                | PropKey::MarginLeft
                | PropKey::MarginTop
                | PropKey::MarginRight
                | PropKey::MarginBottom
                | PropKey::Margin
                | PropKey::PaddingLeft
                | PropKey::PaddingTop
                | PropKey::PaddingRight
                | PropKey::PaddingBottom
                | PropKey::Padding
                | PropKey::Size
                | PropKey::MinSize
        )
    }
}

/// sRGB bytes, alpha last — the file's `#RRGGBBAA`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    pub const TRANSPARENT: Rgba = Rgba([0, 0, 0, 0]);
    pub fn with_opacity(self, opacity: f32) -> Rgba {
        let mut c = self;
        c.0[3] = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        c
    }
    pub fn to_f32(self) -> [f32; 4] {
        [self.0[0] as f32 / 255.0, self.0[1] as f32 / 255.0, self.0[2] as f32 / 255.0, self.0[3] as f32 / 255.0]
    }
}

/// A property value after compile. Lists/calls point into `Page::values`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    VBool(bool),
    /// A bare number; lengths are epx.
    VNum(f32),
    VColor(Rgba),
    /// A displayed string with an optional translation key (`NO_STR` = none).
    VText { text: StrId, key: StrId },
    /// `$name` — a theme value or widget param, resolved at theme time.
    /// `opacity` overrides the alpha when `Some` (`$accent/0.3`).
    VRef { name: StrId, opacity: Option<f32> },
    /// A bare word: enum member, handler name, `fill`/`hug`, `natural`…
    VIdent(StrId),
    /// `220x110`; a component is `None` when omitted (`220x`).
    VSize(Option<f32>, Option<f32>),
    /// `fill(2)` / `hug(2)` / `go(name)` / `play(name)` / `emit(name)`.
    VCall { name: StrId, args: (u32, u32) },
    /// `[a, b, c]` — a range in `Page::values`.
    VList(u32, u32),
    /// A gradient stop: color at a position (`None` = evenly spaced).
    VStop { color: Rgba, pos: Option<f32> },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prop {
    pub key: PropKey,
    pub value: Value,
}

/// A condition a `when` responds to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Held,
    Hovered,
    Focused,
    FocusWithin,
    Selected,
    StateDisabled,
    /// Index into `Page::arrangements`.
    Arrangement(u32),
    /// `screen.<class>` / `screen.<orientation>` — the class word.
    Screen(StrId),
}

impl State {
    /// Spec priority: arrangements < focus_within < focused < hovered <
    /// selected < held < disabled.
    pub fn priority(self) -> u8 {
        match self {
            State::Arrangement(_) | State::Screen(_) => 0,
            State::FocusWithin => 1,
            State::Focused => 2,
            State::Hovered => 3,
            State::Selected => 4,
            State::Held => 5,
            State::StateDisabled => 6,
        }
    }
}

/// `when <state> prop=...` on one node: the props range in `Page::props`.
/// `of` = the node whose state is watched (`NO_NODE` = this node); a
/// style's `when held face.background=…` lands on `face` watching the
/// Button.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct When {
    pub state: State,
    pub props: (u32, u32),
    pub of: NodeId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// A handler name the app matches on.
    Named(StrId),
    Go(u32),
    ToggleArrangement(u32),
    Play(StrId),
    Emit(StrId),
}

/// `on:<event>=<action>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OnHandler {
    pub event: StrId,
    pub action: Action,
}

/// One transition endpoint; `None` = `any`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    /// `NO_NODE` = a view-level (arrangement) rule.
    pub node: NodeId,
    pub from: Option<State>,
    pub to: Option<State>,
    pub duration: f32,
    pub easing: StrId,
    pub anim: StrId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemeEntry {
    pub name: StrId,
    /// `NO_STR` = the defaults block.
    pub variant: StrId,
    pub value: Value,
}

/// `theme { Button background=... }` — a kind's default props.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KindDefault {
    pub kind: Kind,
    pub props: (u32, u32),
}

/// One track of an `anim`, sequencing already solved at compile time:
/// `node.channel` goes `from` → `to` starting at `start` seconds (the
/// `and`/`then` chain folded in) over `duration` with `easing`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimTrack {
    pub anim: StrId,
    pub node: NodeId,
    /// `translate.x`, `scale`, `rotate`, `opacity`…
    pub channel: StrId,
    /// `None` = `to=` alone: start from the live value when the track starts.
    pub from: Option<f32>,
    pub to: f32,
    pub start: f32,
    pub duration: f32,
    pub easing: StrId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Node {
    pub kind: Kind,
    /// The type word for `Custom`.
    pub type_name: StrId,
    pub name: StrId,
    pub parent: NodeId,
    /// Direct children: first index and count (contiguous).
    pub children: (u32, u32),
    /// One past the last descendant — the subtree is `id+1..subtree_end`.
    pub subtree_end: u32,
    pub placement: Placement,
    pub props: (u32, u32),
    pub whens: (u32, u32),
    pub handlers: (u32, u32),
    /// `gap` value: fixed epx, or weight when `gap_fill`.
    pub gap: f32,
    pub gap_fill: bool,
    /// A slot region's `automatic=` coupling.
    pub auto: Auto,
    /// State bits the file switches on at build (`selected=true`,
    /// `disabled=true`, a `choice`'s `value=`): `runtime::SELECTED` /
    /// `runtime::DISABLED`. The Runtime starts every node from these.
    pub initial_states: u8,
    pub line: u32,
}

impl Node {
    pub const EMPTY: Node = Node {
        kind: Kind::Layers,
        type_name: NO_STR,
        name: NO_STR,
        parent: NO_NODE,
        children: (0, 0),
        subtree_end: 0,
        placement: Placement::DEFAULT,
        props: (0, 0),
        whens: (0, 0),
        handlers: (0, 0),
        gap: 0.0,
        gap_fill: false,
        auto: Auto::NoAuto,
        initial_states: 0,
        line: 0,
    };
}

/// The color words every `.gogo` can use bare (`color=red`), the one
/// table for every renderer: CSS-named, readable on white and black. A
/// theme block overrides a word for the file (`theme { red = #E51400 }`).
pub const PALETTE: &[(&str, u32)] = &[
    ("black", 0x000000),
    ("white", 0xFFFFFF),
    ("red", 0xFF6347),     // tomato
    ("orange", 0xF4A460),  // sandy brown
    ("yellow", 0xDAA520),  // goldenrod
    ("green", 0x3CB371),   // medium sea green
    ("teal", 0x5F9EA0),    // cadet blue
    ("blue", 0x6495ED),    // cornflower
    ("indigo", 0x6A5ACD),  // slate blue
    ("purple", 0x9370DB),  // medium purple
    ("pink", 0xDB7093),    // pale violet red
    ("brown", 0xBC8F8F),   // rosy brown
    ("smoke", 0xF5F4F0),   // warm off-white
    ("silver", 0xDCDCDC),  // gainsboro
    ("gray", 0x708090),    // slate gray
    ("charcoal", 0x2F4F4F), // dark slate
];

/// Words that name a palette word: `accent` follows `blue` (remap one and
/// the other follows).
pub const PALETTE_ALIASES: &[(&str, &str)] = &[
    ("accent", "blue"),
    ("on_accent", "white"),
    ("danger", "red"),
    ("warning", "yellow"),
    ("success", "green"),
    ("info", "teal"),
    ("text_muted", "gray"),
];

/// A palette word's color, aliases followed (`None` = not a palette word).
pub fn palette_color(word: &str) -> Option<Rgba> {
    let word = PALETTE_ALIASES.iter().find(|(a, _)| *a == word).map_or(word, |(_, of)| *of);
    PALETTE.iter().find(|(w, _)| *w == word).map(|(_, hex)| Rgba([(hex >> 16) as u8, (hex >> 8) as u8, *hex as u8, 0xFF]))
}

/// Property words the compiler accepts and no pass reads, each with the
/// reason. Needed because a key that parses and reaches nobody is a
/// silent drop (`selected=`, `disabled=` and a `choice`'s `value=` were
/// dropped for a round, 2026-08-30): the `every_prop_key_is_read` test
/// fails on any key that is neither read by layout/draw/runtime/input/anim
/// nor listed here.
pub const UNREAD_BY_DESIGN: &[(PropKey, &str)] = &[
    (PropKey::Loading, "host-read: `loading=lazy` asks the host to defer that image's decode to its first draw"),
    (PropKey::TouchTarget, "host-read: an enlarged touch plate the host may honor; hit testing here uses the node's own rect"),
    (PropKey::TapPulse, "host-read: the press feedback pulse is drawn by the host; not built here yet"),
    (PropKey::AccessibilityName, "host-read: no screen-reader bridge here yet"),
    (PropKey::KeyboardTarget, "host-read: no keyboard focus routing here yet"),
    (PropKey::Select, "host-read: List selection mode; Lists get their items from the host"),
    (PropKey::Render, "accepted for old files that picked a renderer per node; one renderer here"),
    (PropKey::Step, "Slider steps are not built yet (the drag is not wired)"),
    (PropKey::IconSrc, "accepted for old files; an icon file hint nothing reads"),
    (PropKey::Automatic, "read at compile into `Node.auto`, not as a prop"),
    (PropKey::SeatOf, "read at resolve through `Resolved.seat`, not as a prop"),
    (PropKey::ToggleMode, "`toggle=true` on a Button; the selected flip is not built yet"),
    (PropKey::RowHeight, "Grid `row_height=`: rows size by `row height=` here; the prop form is not built yet"),
    (PropKey::RowSpan, "Grid `row_span=`: only column spans are laid out; not built yet"),
];

/// The build-time shape: the same pools as `&'static` slices.
pub struct StaticPage {
    pub nodes: &'static [Node],
    pub strings: &'static [&'static str],
    pub props: &'static [Prop],
    pub values: &'static [Value],
    pub whens: &'static [When],
    pub handlers: &'static [OnHandler],
    pub theme: &'static [ThemeEntry],
    pub kind_defaults: &'static [KindDefault],
    pub arrangements: &'static [StrId],
    pub transitions: &'static [Transition],
    pub anims: &'static [AnimTrack],
}

#[derive(Clone, Debug, Default)]
pub struct Page {
    pub nodes: Vec<Node>,
    pub strings: Vec<String>,
    pub props: Vec<Prop>,
    pub values: Vec<Value>,
    pub whens: Vec<When>,
    pub handlers: Vec<OnHandler>,
    pub theme: Vec<ThemeEntry>,
    pub kind_defaults: Vec<KindDefault>,
    pub arrangements: Vec<StrId>,
    pub transitions: Vec<Transition>,
    pub anims: Vec<AnimTrack>,
}

impl Page {
    pub fn from_static(s: &StaticPage) -> Page {
        Page {
            nodes: s.nodes.to_vec(),
            strings: s.strings.iter().map(|x| x.to_string()).collect(),
            props: s.props.to_vec(),
            values: s.values.to_vec(),
            whens: s.whens.to_vec(),
            handlers: s.handlers.to_vec(),
            theme: s.theme.to_vec(),
            kind_defaults: s.kind_defaults.to_vec(),
            arrangements: s.arrangements.to_vec(),
            transitions: s.transitions.to_vec(),
            anims: s.anims.to_vec(),
        }
    }

    pub fn str(&self, id: StrId) -> &str {
        &self.strings[id as usize]
    }

    pub fn intern(&mut self, s: &str) -> StrId {
        if self.strings.is_empty() {
            self.strings.push(String::new());
        }
        if let Some(i) = self.strings.iter().position(|x| x == s) {
            return i as StrId;
        }
        self.strings.push(s.to_string());
        (self.strings.len() - 1) as StrId
    }

    pub fn find(&self, name: &str) -> Option<NodeId> {
        self.nodes.iter().position(|n| n.name != NO_STR && self.str(n.name) == name).map(|i| i as NodeId)
    }

    pub fn children(&self, id: NodeId) -> std::ops::Range<usize> {
        let (first, count) = self.nodes[id as usize].children;
        first as usize..(first + count) as usize
    }

    pub fn node_props(&self, id: NodeId) -> &[Prop] {
        let (first, count) = self.nodes[id as usize].props;
        &self.props[first as usize..(first + count) as usize]
    }

    /// The base value of a prop on a node (no states applied).
    pub fn prop(&self, id: NodeId, key: PropKey) -> Option<Value> {
        self.node_props(id).iter().rev().find(|p| p.key == key).map(|p| p.value)
    }

    /// A theme value for the active variant, else the default.
    pub fn theme_value(&self, name: StrId, variant: StrId) -> Option<Value> {
        self.theme
            .iter()
            .filter(|t| t.name == name && t.variant == variant)
            .chain(self.theme.iter().filter(|t| t.name == name && t.variant == NO_STR))
            .map(|t| t.value)
            .next()
    }

    pub fn kind_default(&self, kind: Kind, key: PropKey) -> Option<Value> {
        self.kind_defaults.iter().filter(|d| d.kind == kind).find_map(|d| {
            self.props[d.props.0 as usize..(d.props.0 + d.props.1) as usize].iter().rev().find(|p| p.key == key).map(|p| p.value)
        })
    }

    pub fn list(&self, v: Value) -> &[Value] {
        match v {
            Value::VList(first, count) => &self.values[first as usize..(first + count) as usize],
            _ => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    /// Every word `PropKey::from_word` accepts is read somewhere, or says
    /// why not in `UNREAD_BY_DESIGN`.
    #[test]
    fn every_prop_key_is_read() {
        let page_src = include_str!("page.rs");
        let readers = [include_str!("layout.rs"), include_str!("draw.rs"), include_str!("runtime.rs"), include_str!("input.rs"), include_str!("anim.rs"), include_str!("compile.rs")];
        let from_word_start = page_src.find("pub fn from_word").expect("from_word");
        let from_word = &page_src[from_word_start..];
        let mut unread: Vec<String> = Vec::new();
        for line in from_word.lines() {
            if line.contains("split_once") {
                continue;
            }
            let Some((_, rest)) = line.split_once("=> PropKey::") else { continue };
            let variant: &str = rest.trim_end_matches(',').trim();
            if variant.contains('(') {
                continue;
            }
            let needle = format!("PropKey::{variant}");
            // `compile.rs` reads a key when it matches on it outside `prop_key`
            // (the placement keys, Text, Wrap …); the enum and from_word lines
            // in page.rs do not count.
            let read = readers.iter().any(|src| src.contains(&needle));
            let excused = page_src.contains(&format!("(PropKey::{variant}, \""));
            if !read && !excused {
                unread.push(variant.to_string());
            }
        }
        assert!(unread.is_empty(), "prop keys nobody reads — read them or list them in UNREAD_BY_DESIGN: {unread:?}");
    }
}
