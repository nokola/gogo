//! The compiled page: a flat arena of nodes plus pools (strings, props,
//! values, whens, handlers). Needed in this shape so `build.rs` can emit a
//! page as `static` slices and boot copies them in (`Page::from_static`)
//! with no parsing; hot reload builds the same `Page` from text. Every
//! type here is `Copy` + `Debug`, and `Debug` output IS the emitted Rust
//! literal (see `emit.rs`) — keep variant names unique across enums.

/// Declares an id over `u32` as its own type: `Copy`, ordered, hashable,
/// and an index into any `Vec<T>` / `[T]` (`page.nodes[node]`,
/// `rects[node]`, `page.strings[text]`).
///
/// Needed so a node id, a string id, a key id and an arrangement id never
/// mix: handing one where another belongs is a rustc error, not a wrong
/// slot read at runtime. The raw number stays private to this crate — the
/// pools it indexes live here; every other crate passes the id whole.
///
/// `Debug` prints the constructor call (`NodeId::new(3)`) because the
/// emitted page literal (`emit.rs`) is `Debug` output.
macro_rules! id_newtype {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(transparent)]
        pub struct $name(u32);

        impl $name {
            pub const fn new(raw: u32) -> $name {
                $name(raw)
            }

            /// The id as a pool index.
            pub const fn index(self) -> usize {
                self.0 as usize
            }

            pub const fn from_index(i: usize) -> $name {
                $name(i as u32)
            }

            /// Every id from `from` up to (not including) `to`, in order.
            pub fn range(from: $name, to: $name) -> impl DoubleEndedIterator<Item = $name> + ExactSizeIterator + Clone {
                (from.0..to.0).map($name)
            }

            /// The id `by` slots on (a clone's shift, a bind site's offset
            /// from its row root) — an arena fact, so crate-private.
            pub(crate) const fn offset(self, by: u32) -> $name {
                $name(self.0 + by)
            }

            /// This id's twin under another root: the same place in a
            /// subtree cloned from `from` to `to` (`Page::clone_subtree`
            /// keeps the layout, so a repeat slot's node is the template's
            /// node moved by the roots' distance). The build's
            /// `<Repeat>Slot::at` uses it, so code names a slot's parts
            /// as typed fields, never by string path.
            pub const fn rebase(self, from: $name, to: $name) -> $name {
                $name(self.0 - from.0 + to.0)
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, concat!(stringify!($name), "::new({})"), self.0)
            }
        }

        impl<T> std::ops::Index<$name> for Vec<T> {
            type Output = T;
            fn index(&self, id: $name) -> &T {
                &self[id.0 as usize]
            }
        }

        impl<T> std::ops::IndexMut<$name> for Vec<T> {
            fn index_mut(&mut self, id: $name) -> &mut T {
                &mut self[id.0 as usize]
            }
        }

        impl<T> std::ops::Index<$name> for [T] {
            type Output = T;
            fn index(&self, id: $name) -> &T {
                &self[id.0 as usize]
            }
        }

        impl<T> std::ops::IndexMut<$name> for [T] {
            fn index_mut(&mut self, id: $name) -> &mut T {
                &mut self[id.0 as usize]
            }
        }
    };
}

id_newtype! {
    /// A node: its index in the page's arena (`Page::nodes`) and in every
    /// per-node runtime table (rects, motion, scroll, states).
    NodeId
}
id_newtype! {
    /// An interned string: its index in `Page::strings`. Anim names, node
    /// names, theme words and text all live there.
    StrId
}
id_newtype! {
    /// A translation key (`text="x"/key`), one id space for the WHOLE
    /// build: keys are global — `/ok` in two files is one key — so the id
    /// indexes the build's `KEYS` table and a `Locale`, not a page's
    /// strings.
    KeyId
}
id_newtype! {
    /// An arrangement: its index in `Page::arrangements`, or `BASE` (the
    /// rest pose). The page build emits `enum Arrangement` over these
    /// (`ArrangementName`), so code names a pose, never a number.
    ArrangementId
}
id_newtype! {
    /// An image the pages show (`Image source="…"`): its index in the
    /// build's `IMAGES` table — one id space for the WHOLE build — and in
    /// every host table over images (file bytes, header sizes, GPU
    /// slots). A draw and the renderer pass this, never the path, so
    /// showing an image hashes no string. `Images` maps a path to it.
    ImageKey
}
impl Default for StrId {
    fn default() -> StrId {
        NO_STR
    }
}

/// The rest pose — no `arrangement` word in force.
pub const BASE: ArrangementId = ArrangementId(u32::MAX);
/// No node (a root's parent).
pub const NO_NODE: NodeId = NodeId(u32::MAX);
/// Content of a `disabled` control dims to this unless the theme or a
/// `when disabled` says otherwise (`Page::disabled_opacity`).
pub const DISABLED_OPACITY: f32 = 0.3;

/// A bare ListItem row's injected state fills (D122): selection, the
/// press and the keyboard cursor show out of the box. The addon reads
/// the engine theme's ItemList `selected` color with white@0.3 as the
/// fallback (list_item.gd); with no engine theme here the fallback IS
/// the color — held at half strength, the cursor echo at a quarter.
pub const LIST_SELECTED_FILL: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([255, 255, 255, 77]);
pub const LIST_HELD_FILL: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([255, 255, 255, 38]);
pub const LIST_FOCUS_FILL: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([255, 255, 255, 19]);
/// The Button family's injected default `when focused` ring (D129: the
/// engine's always-on ring retired, re-owned as a default response) —
/// visible focus shows it for key-driven focus only, and authoring
/// `when focused` replaces it (the D122 per-state doctrine).
pub const BUTTON_FOCUS_RING: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([255, 255, 255, 85]);
pub const BUTTON_FOCUS_RING_EPX: f32 = 2.0;
/// The empty string; `name == NO_STR` = unnamed.
pub const NO_STR: StrId = StrId(0);
/// No translation key: the text is a literal, shown as written.
pub const NO_KEY: KeyId = KeyId(u32::MAX);
/// No image: an Image whose `source=""` — drawn as nothing.
pub const NO_IMAGE: ImageKey = ImageKey(u32::MAX);

/// The image paths a build (or one standalone compile) names, by
/// `ImageKey`.
///
/// Needed because an image id is one space for the whole build: every
/// page's `source=` indexes the same `IMAGES` table, and a host picture
/// the pages never named (a brush face thumbnail, `face://3`) interns
/// here and takes the next id — the renderer's slot table simply grows.
#[derive(Clone, Debug, Default)]
pub struct Images {
    /// By id: the path as the file wrote it (`res://assets/x.png`).
    pub names: Vec<String>,
    index: std::collections::HashMap<String, ImageKey>,
    /// A runtime table seeded from the build (`from_names`): every
    /// `res://` file it will ever show is already in it — the phone has
    /// no file system to fetch a late one from. A build-time table (the
    /// compiler's) is not sealed.
    sealed: bool,
}

impl Images {
    /// A table over `names` in id order — the host seeds one from the
    /// build's `IMAGES` so its ids match what the pages carry. The table
    /// is SEALED for `res://` names: interning a file the build did not
    /// embed is the `unembedded_image` lint (a debug panic; release
    /// interns a key that shows nothing).
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Images {
        let mut images = Images::default();
        for name in names {
            images.intern(name);
        }
        images.sealed = true;
        images
    }

    /// The id for `name`, new or existing. A host picture (`face://3`,
    /// `preset://default`) is any non-`res://` name and is always welcome;
    /// a `res://` file a sealed table has never heard of is the
    /// `unembedded_image` lint — name the file in a `.gogo` `source=` or
    /// wrap the path in `gogo_image!()` so the build embeds it.
    pub fn intern(&mut self, name: &str) -> ImageKey {
        if let Some(&key) = self.index.get(name) {
            return key;
        }
        debug_assert!(!(self.sealed && name.starts_with("res://")), "unembedded_image: `{name}` is not in the build's image table — name it in a .gogo `source=` or wrap it in gogo_image!() so the build embeds it (a runtime res:// string shows nothing on the phone)");
        let key = ImageKey::from_index(self.names.len());
        self.names.push(name.to_string());
        self.index.insert(name.to_string(), key);
        key
    }

    /// The id `name` already has — a lookup, never a new id.
    pub fn get(&self, name: &str) -> Option<ImageKey> {
        self.index.get(name).copied()
    }

    /// The path behind `key`; `""` for `NO_IMAGE` or an id this table
    /// never gave.
    pub fn name(&self, key: ImageKey) -> &str {
        self.names.get(key.index()).map(String::as_str).unwrap_or("")
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

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
    Rectangle,
    Circle,
    /// A circular stroke segment: `from=` (degrees, compass), `sweep=`
    /// (degrees, clockwise), `thickness=` (the stroke — not `width`).
    Arc,
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
    /// The region's angular wedge [from, from + frac x sweep] reveals its
    /// content -- drawing and hits agree (spec 4.6). Degrees, Godot's
    /// screen convention: 0 = right, clockwise positive.
    Arc { from: f32, sweep: f32, frac: f32 },
    /// The region SEATS on the circle centered in its host, at
    /// from + frac x sweep (an angular position, never a rotation).
    Angle { from: f32, sweep: f32, frac: f32 },
}

/// The shown form of a `live()` number in a text position: snapped to
/// the display step (0.1 unless `round($v, step)` picked one), trailing
/// zeros trimmed — "10.2", "11", "80" (the addon's display rounding,
/// user-caught when a drag printed the raw float).
pub(crate) fn live_text(v: f32, step: f32) -> String {
    let snapped = if step > 0.0 { (v / step).round() * step } else { v };
    let mut d = 0usize;
    let mut s = step;
    while d < 4 && (s - s.round()).abs() > 1e-4 {
        s *= 10.0;
        d += 1;
    }
    let mut out = format!("{snapped:.d$}");
    if out.contains('.') {
        while out.ends_with('0') {
            out.pop();
        }
        if out.ends_with('.') {
            out.pop();
        }
    }
    out
}

/// One `automatic=` coupling, baked so the runtime can re-derive the
/// fraction when code writes the control's value (GP33 -- the addon's
/// GogoDriver). `state != NO_STR` = a presence coupling following that
/// custom state; else the value coupling normalizes over min..max.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutoDriver {
    pub node: NodeId,
    /// The control instance root whose `$value` (or state) drives it.
    pub root: NodeId,
    pub min: f32,
    pub max: f32,
    pub state: StrId,
}

/// A control's `behavior slider` line, baked per instance (GP52,
/// user-approved 2026-09-03): the stock slider machine answers the
/// instance's input — a drag or tap anywhere on it maps bare x over the
/// `track` slot's width to `$value` (shipped SliderPhone drew no thumb,
/// so no knob compensation), arrows/Home/End/wheel step it. The machine
/// OWNS the arrow and Home/End key groups — a `keys` claim on them
/// refuses at compile (one owner per group).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderBehavior {
    /// The instance root: value writes, `edit`/`change` events, `held`
    /// and focus all land here.
    pub root: NodeId,
    /// The `slot track` the finger maps over.
    pub track: NodeId,
    pub min: f32,
    pub max: f32,
    /// `step=`: 0 = continuous; a positive step also serves as the key
    /// unit (the stock law — `key_unit=` never overrides a real step).
    pub step: f32,
    /// The key/wheel unit for a CONTINUOUS slider, in value units
    /// (`key_unit=5%` bakes as 0.05 × the range); 0 = unset — the
    /// stock 1.0.
    pub key_unit: f32,
    /// `$value` at build — what the machine reads before any write
    /// lands (the instance root carries no live Value prop until one
    /// does).
    pub rest: f32,
    /// The contract's declared `drag_started` signal (`NO_STR` when
    /// undeclared): raised as the gesture opens, before the first
    /// `edit` — the addon's behaviors emitted, then applied.
    pub drag_started: StrId,
    /// The closing edge — raised after the release commit AND on a
    /// cancelled or reclaimed gesture (a host arming UI on
    /// `drag_started` must always hear the end). `NO_STR` when
    /// undeclared.
    pub drag_ended: StrId,
}

/// A `text=live($value)` label inside a control body: a code write of the
/// control's value rewrites the shown string at the baked display step
/// (`Runtime::set_value` -- the addon's live-label follow).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveText {
    pub node: NodeId,
    pub root: NodeId,
    pub step: f32,
}

/// Every node-type word and its kind, in the order the docs list them.
/// `Custom` has no word (a code control or an unknown type).
pub const KIND_WORDS: &[(&str, Kind)] = &[
    ("View", Kind::View),
    ("Panel", Kind::Panel),
    ("Column", Kind::Column),
    ("Row", Kind::Row),
    ("Flow", Kind::Flow),
    ("Layers", Kind::Layers),
    ("Scroll", Kind::Scroll),
    ("Grid", Kind::Grid),
    ("Tiles", Kind::Tiles),
    ("Label", Kind::Label),
    ("Button", Kind::Button),
    ("Image", Kind::Image),
    ("Rectangle", Kind::Rectangle),
    ("Circle", Kind::Circle),
    ("Arc", Kind::Arc),
    ("Gradient", Kind::Gradient),
    ("Slider", Kind::Slider),
    ("Input", Kind::Input),
    ("Check", Kind::Check),
    ("Toggle", Kind::Toggle),
    ("Radio", Kind::Radio),
    ("choice", Kind::Choice),
    ("List", Kind::List),
    ("ListItem", Kind::ListItem),
    ("repeat", Kind::Repeat),
    ("gap", Kind::Gap),
    ("row", Kind::GridRow),
    ("skip", Kind::Skip),
    ("slot", Kind::Slot),
    ("seat", Kind::Seat),
    ("empty", Kind::Empty),
];

impl Kind {
    pub fn from_word(word: &str) -> Option<Kind> {
        KIND_WORDS.iter().find(|(w, _)| *w == word).map(|(_, k)| *k)
    }

    /// The word a kind is written as (`Kind::GridRow` → `row`); `Custom`
    /// gives "" — the compiler names the type word from the file instead.
    pub fn word(self) -> &'static str {
        KIND_WORDS.iter().find(|(_, k)| *k == self).map_or("", |(w, _)| *w)
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
    /// Per-corner radii — they join the plate family; the uniform
    /// `corner_radius=` writes all four, later on the line wins.
    CornerRadiusTopLeft,
    CornerRadiusTopRight,
    CornerRadiusBottomRight,
    CornerRadiusBottomLeft,
    /// Gradient `repeat=mirror`: the ramp runs 0..0.5 and mirrors back.
    GradientRepeat,
    /// `from=` / `to=`: a Gradient's 0..1 box points, or an Arc's degrees.
    From,
    To,
    /// Arc words: the swept angle (degrees, clockwise) and the stroke.
    Sweep,
    Thickness,
    /// How much of the box shows along an axis (0..1): layout space
    /// grows with the fraction — a disclosure reveal, not a clip mask.
    RevealX,
    RevealY,
    BorderThickness,
    BorderColor,
    /// `border_dash=[4, 1]`: dash and gap lengths in `border_thickness`
    /// units, repeated along the plate's edge (WPF StrokeDashArray; an odd
    /// list repeats to even out). Drawn as crisp axis-aligned quads —
    /// never antialiased line ends, which feather into a 1-epx gap and
    /// erase it (D257). Corners stay square while dashed.
    BorderDash,
    /// `dash=[4, 3]` on an Arc: dash and gap lengths in `thickness` units
    /// along the stroke (an Arc has no border — its stroke is
    /// `thickness=`). Same list rules as `border_dash=`.
    Dash,
    Opacity,
    Visibility,
    Disabled,
    /// `source=` on an Image: the file path, resolved by the build against
    /// its asset root (`BuildOptions::res_root`).
    Source,
    Value,
    /// `placeholder_text=` on an Input: drawn dimmed while the field is
    /// empty (Godot's LineEdit word, GPAR112).
    PlaceholderText,
    /// `max_length=` on an Input: typing stops at this many characters;
    /// absent or 0 = no cap (Godot's LineEdit word, GPAR112).
    MaxLength,
    Min,
    Max,
    Cols,
    /// `areas=` on a Grid: the named-region form, expanded at compile into
    /// the same `row { }` structure the row form builds.
    Areas,
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
    /// `selection_color=` — an Input's selection highlight (and its
    /// drag handles, drawn opaque). Unset = white at 25%.
    SelectionColor,
    /// `color.a` — a Rectangle/Circle fill's alpha alone (whens/anims).
    ColorA,
    /// `stop_<n>_color` — a Gradient's declared stop, kept LIVE after
    /// declaration (GP40 step 4, user 2026-09-02: parametric keys, the
    /// `stop_1_color` spelling). 1-based, `stops=` declaration order.
    StopColor(u8),
    /// `stop_<n>_offset` — the live position half; a driven write
    /// clamps to stay non-decreasing (declared `@` order is G2005's).
    StopOffset(u8),
    /// `theme { disabled_opacity = … }`: how far a `disabled` node's
    /// content dims when no `when disabled` on it says otherwise.
    DisabledOpacity,
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
    /// `Flow orientation=vertical` — items run DOWN and wrap into
    /// columns (WrapPanel's other axis; GPAR97).
    Orientation,
    Step,
    IconSrc,
    /// `icon_size=` on a Button: the icon's box (accepted-unread beside `icon=`).
    IconSize,
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
    /// `pivot=` — the 0..1 box point `scale` and `rotate` transform
    /// about (`0x0` = the top-left corner; unset = the center). A look
    /// word, not a motion channel: it places the fixed point, the
    /// channels do the moving.
    Pivot,
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

/// Every property word and its key — the one spelling table: `from_word`
/// reads it, `word` spells a key back for a did-you-mean, and the per-kind
/// table below says where each is legal.
pub const WORDS: &[(&str, PropKey)] = &[
    ("text", PropKey::Text),
    ("background", PropKey::Background),
    ("color", PropKey::Color),
    ("font_size", PropKey::FontSize),
    ("spacing", PropKey::Spacing),
    ("corner_radius", PropKey::CornerRadius),
    ("corner_radius_top_left", PropKey::CornerRadiusTopLeft),
    ("corner_radius_top_right", PropKey::CornerRadiusTopRight),
    ("corner_radius_bottom_right", PropKey::CornerRadiusBottomRight),
    ("corner_radius_bottom_left", PropKey::CornerRadiusBottomLeft),
    ("repeat", PropKey::GradientRepeat),
    ("from", PropKey::From),
    ("to", PropKey::To),
    ("sweep", PropKey::Sweep),
    ("thickness", PropKey::Thickness),
    ("reveal_x", PropKey::RevealX),
    ("reveal_y", PropKey::RevealY),
    ("border_thickness", PropKey::BorderThickness),
    ("border_color", PropKey::BorderColor),
    ("border_dash", PropKey::BorderDash),
    ("dash", PropKey::Dash),
    ("opacity", PropKey::Opacity),
    ("visibility", PropKey::Visibility),
    ("disabled", PropKey::Disabled),
    ("source", PropKey::Source),
    ("value", PropKey::Value),
    ("placeholder_text", PropKey::PlaceholderText),
    ("max_length", PropKey::MaxLength),
    ("min", PropKey::Min),
    ("max", PropKey::Max),
    ("cols", PropKey::Cols),
    ("areas", PropKey::Areas),
    ("row_height", PropKey::RowHeight),
    ("col_span", PropKey::ColSpan),
    ("row_span", PropKey::RowSpan),
    ("fit", PropKey::Fit),
    ("horizontal_alignment", PropKey::TextHalign),
    ("vertical_alignment", PropKey::TextValign),
    ("wrap", PropKey::Wrap),
    ("allow", PropKey::Allow),
    ("bars", PropKey::Bars),
    ("toggle", PropKey::ToggleMode),
    ("selected", PropKey::SelectedProp),
    ("fill", PropKey::GradientFill),
    ("stops", PropKey::Stops),
    ("angle", PropKey::Angle),
    ("aspect", PropKey::Aspect),
    ("font", PropKey::Font),
    ("font_color", PropKey::FontColor),
    ("selection_color", PropKey::SelectionColor),
    ("color.a", PropKey::ColorA),
    ("disabled_opacity", PropKey::DisabledOpacity),
    ("clip_text", PropKey::ClipText),
    ("clip_contents", PropKey::ClipContents),
    ("span", PropKey::Span),
    ("full_row", PropKey::FullRow),
    ("loading", PropKey::Loading),
    ("touch_target", PropKey::TouchTarget),
    ("tap_pulse", PropKey::TapPulse),
    ("accessibility_name", PropKey::AccessibilityName),
    ("keyboard_target", PropKey::KeyboardTarget),
    ("select", PropKey::Select),
    ("orientation", PropKey::Orientation),
    ("step", PropKey::Step),
    ("icon", PropKey::IconSrc),
    ("icon_size", PropKey::IconSize),
    ("automatic", PropKey::Automatic),
    ("seat", PropKey::SeatOf),
    ("overlay", PropKey::Overlay),
    ("interactive", PropKey::Interactive),
    ("scale", PropKey::Scale),
    ("scale.x", PropKey::ScaleX),
    ("scale.y", PropKey::ScaleY),
    ("translate.x", PropKey::TranslateX),
    ("translate.y", PropKey::TranslateY),
    ("rotate", PropKey::Rotate),
    ("pivot", PropKey::Pivot),
    ("width", PropKey::Width),
    ("height", PropKey::Height),
    ("min_width", PropKey::MinWidth),
    ("min_height", PropKey::MinHeight),
    ("max_width", PropKey::MaxWidth),
    ("max_height", PropKey::MaxHeight),
    ("halign", PropKey::Halign),
    ("valign", PropKey::Valign),
    ("margin_left", PropKey::MarginLeft),
    ("margin_top", PropKey::MarginTop),
    ("margin_right", PropKey::MarginRight),
    ("margin_bottom", PropKey::MarginBottom),
    ("margin", PropKey::Margin),
    ("padding_left", PropKey::PaddingLeft),
    ("padding_top", PropKey::PaddingTop),
    ("padding_right", PropKey::PaddingRight),
    ("padding_bottom", PropKey::PaddingBottom),
    ("padding", PropKey::Padding),
    ("size", PropKey::Size),
    ("min_size", PropKey::MinSize),
];

impl PropKey {
    pub fn from_word(word: &str) -> Option<PropKey> {
        if let Some(k) = WORDS.iter().find(|(w, _)| *w == word).map(|(_, k)| *k) {
            return Some(k);
        }
        // The numbered stop family (`stop_2_color`): the number rides
        // IN the key — no cap, the real check is against the gradient's
        // declared count.
        let rest = word.strip_prefix("stop_")?;
        let (n, prop) = rest.split_once('_')?;
        let n: u8 = n.parse().ok()?;
        if n == 0 {
            return None;
        }
        match prop {
            "color" => Some(PropKey::StopColor(n)),
            "offset" => Some(PropKey::StopOffset(n)),
            _ => None,
        }
    }

    /// Keys holding ONE color — the keys whose `.a` anim lane (GP40)
    /// moves alpha alone, and whose anim layer stores `VNum` alphas for
    /// `compose_anim` to put on the ladder's hue.
    pub fn is_color(self) -> bool {
        matches!(self, PropKey::Background | PropKey::Color | PropKey::BorderColor | PropKey::FontColor | PropKey::SelectionColor | PropKey::StopColor(_))
    }

    /// The word a key is written as (`PropKey::FontColor` → `font_color`);
    /// `Other` has no fixed spelling and gives "".
    pub fn word(self) -> &'static str {
        WORDS.iter().find(|(_, k)| *k == self).map_or("", |(w, _)| *w)
    }

    /// Keys that take a number: `opacity=red` or `scale=big` is refused
    /// (G0058) instead of reading as "no value" — a bare word here is a
    /// typo that would otherwise do nothing on screen. `value=` stays open
    /// (a Slider's number, an Input's text).
    pub fn is_number(self) -> bool {
        matches!(
            self,
            PropKey::FontSize
                | PropKey::Spacing
                | PropKey::CornerRadius
                | PropKey::BorderThickness
                | PropKey::Opacity
                | PropKey::Min
                | PropKey::Max
                | PropKey::Step
                | PropKey::RowHeight
                | PropKey::ColSpan
                | PropKey::RowSpan
                | PropKey::Angle
                | PropKey::DisabledOpacity
                | PropKey::Span
                | PropKey::Scale
                | PropKey::ScaleX
                | PropKey::ScaleY
                | PropKey::TranslateX
                | PropKey::TranslateY
                | PropKey::Rotate
                | PropKey::ColorA
                | PropKey::StopOffset(_)
        )
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

// ----- which words each kind takes (E8, 2026-08-31) ------------------------
//
// One table, read three ways: the compiler refuses a word that is not in
// its kind's row (an error naming the nearest legal word), the docs list
// each kind's words from it (`kind_words`), and `every_prop_key_is_read`
// checks every key has a row somewhere.
//
// Needed because a word the file spells and no pass reads is a silent
// drop (`Column value=3` lays out nothing, `Label cols=2` too) — with the
// table every such word is a build error instead (docs/gogo-design.md §8,
// E8 2026-08-31).

/// Placement, presence and motion — every box carries these.
const BOX: &[PropKey] = &[
    PropKey::Width,
    PropKey::Height,
    PropKey::MinWidth,
    PropKey::MinHeight,
    PropKey::MaxWidth,
    PropKey::MaxHeight,
    PropKey::Size,
    PropKey::MinSize,
    PropKey::Halign,
    PropKey::Valign,
    PropKey::Margin,
    PropKey::MarginLeft,
    PropKey::MarginTop,
    PropKey::MarginRight,
    PropKey::MarginBottom,
    PropKey::Aspect,
    PropKey::Opacity,
    PropKey::Visibility,
    PropKey::Overlay,
    PropKey::Interactive,
    PropKey::Font,
    PropKey::Scale,
    PropKey::ScaleX,
    PropKey::ScaleY,
    PropKey::TranslateX,
    PropKey::TranslateY,
    PropKey::Rotate,
    PropKey::Pivot,
    PropKey::SeatOf,
    PropKey::Disabled,
    PropKey::TouchTarget,
    PropKey::RevealX,
    PropKey::RevealY,
    PropKey::AccessibilityName,
    PropKey::KeyboardTarget,
];
/// The plate behind a box's content. Any box has one (E8): a Label with
/// `background=` gets a plate under its text, padding included, the same
/// as a Panel — no wrapping Panel needed.
const PLATE: &[PropKey] = &[PropKey::Background, PropKey::CornerRadius, PropKey::CornerRadiusTopLeft, PropKey::CornerRadiusTopRight, PropKey::CornerRadiusBottomRight, PropKey::CornerRadiusBottomLeft, PropKey::BorderThickness, PropKey::BorderColor, PropKey::BorderDash, PropKey::ClipContents];
/// Air inside the box around its children or text.
const PADDING: &[PropKey] = &[PropKey::Padding, PropKey::PaddingLeft, PropKey::PaddingTop, PropKey::PaddingRight, PropKey::PaddingBottom];
/// The words of a box that shows text of its own.
const TEXT: &[PropKey] = &[PropKey::Text, PropKey::FontSize, PropKey::FontColor, PropKey::Wrap, PropKey::ClipText, PropKey::TextHalign, PropKey::TextValign];
/// A press surface's words (`disabled=` is every box's: it cascades down
/// a subtree, so a Panel can switch off everything in it).
const PRESS: &[PropKey] = &[PropKey::TapPulse];
/// The shared row: a code control (drawn by the host) takes the box, its
/// plate and padding, nothing of its own — declaring host-read words per
/// control is docs/todo-gogo.md G13 (LATER).
const SHARED: &[&[PropKey]] = &[BOX, PLATE, PADDING];

/// A Grid cell's words — legal on any box whose parent is a Grid row.
pub const GRID_CELL: &[PropKey] = &[PropKey::ColSpan, PropKey::RowSpan];
/// A Tiles cell's words — legal on any box whose parent is a Tiles.
pub const TILES_CELL: &[PropKey] = &[PropKey::Span, PropKey::FullRow];

/// The rows: which groups (and own words) each kind takes.
fn kind_row(kind: Kind) -> &'static [&'static [PropKey]] {
    match kind {
        Kind::View => &[BOX, PLATE, PADDING, &[PropKey::DisabledOpacity]],
        Kind::Panel | Kind::Layers | Kind::Custom => SHARED,
        Kind::Column | Kind::Row => &[BOX, PLATE, PADDING, &[PropKey::Spacing]],
        Kind::Flow => &[BOX, PLATE, PADDING, &[PropKey::Spacing, PropKey::Orientation]],
        Kind::Scroll => &[BOX, PLATE, PADDING, &[PropKey::Allow, PropKey::Bars]],
        Kind::Grid => &[BOX, PLATE, PADDING, &[PropKey::Cols, PropKey::Areas, PropKey::RowHeight, PropKey::Spacing]],
        Kind::Tiles => &[BOX, PLATE, PADDING, &[PropKey::Cols, PropKey::Spacing]],
        Kind::Label => &[BOX, PLATE, PADDING, TEXT],
        Kind::Button => &[BOX, PLATE, PADDING, TEXT, PRESS, &[PropKey::ToggleMode, PropKey::SelectedProp, PropKey::IconSrc, PropKey::IconSize]],
        Kind::Image => &[BOX, PLATE, &[PropKey::Source, PropKey::Fit, PropKey::Loading]],
        Kind::Rectangle => &[BOX, &[PropKey::Color, PropKey::ColorA, PropKey::CornerRadius, PropKey::CornerRadiusTopLeft, PropKey::CornerRadiusTopRight, PropKey::CornerRadiusBottomRight, PropKey::CornerRadiusBottomLeft, PropKey::BorderThickness, PropKey::BorderColor, PropKey::BorderDash]],
        Kind::Circle => &[BOX, &[PropKey::Color, PropKey::ColorA, PropKey::BorderThickness, PropKey::BorderColor, PropKey::BorderDash]],
        // `corner_radius` (Q9, user 2026-09-05): a rounded or disc-shaped
        // ramp — the brush-options preview ball. The uniform key only: the
        // per-corner radii ride the draw's uv slot, which a gradient's band
        // already uses.
        Kind::Gradient => &[BOX, &[PropKey::Stops, PropKey::GradientFill, PropKey::Angle, PropKey::GradientRepeat, PropKey::From, PropKey::To, PropKey::CornerRadius]],
        Kind::Arc => &[BOX, &[PropKey::Color, PropKey::ColorA, PropKey::From, PropKey::Sweep, PropKey::Thickness, PropKey::Dash]],
        // A Slider's `background=` is its track, `color=` the filled part.
        Kind::Slider => &[BOX, &[PropKey::Background, PropKey::CornerRadius, PropKey::Color, PropKey::Value, PropKey::Min, PropKey::Max, PropKey::Step]],
        Kind::Input => &[BOX, PLATE, PADDING, TEXT, &[PropKey::Value, PropKey::PlaceholderText, PropKey::MaxLength, PropKey::SelectionColor]],
        Kind::Check | Kind::Radio => &[BOX, PLATE, PADDING, TEXT, PRESS, &[PropKey::SelectedProp]],
        Kind::Toggle => &[BOX, PLATE, PADDING, TEXT, PRESS, &[PropKey::SelectedProp, PropKey::Value]],
        Kind::Choice => &[BOX, &[PropKey::Value, PropKey::Spacing]],
        Kind::List => &[BOX, PLATE, PADDING, &[PropKey::Select, PropKey::Spacing, PropKey::Bars]],
        Kind::ListItem => &[BOX, PLATE, PADDING, PRESS, &[PropKey::SelectedProp]],
        Kind::Slot => &[BOX, PLATE, PADDING, &[PropKey::Automatic]],
        Kind::Seat | Kind::GridRow => &[BOX],
        // Structure words: their line takes clauses or one size word, no
        // properties (`gap 8`, `repeat … as $item`, `skip`, `empty { }`).
        Kind::Repeat | Kind::Gap | Kind::Skip | Kind::Empty => &[],
    }
}

/// Is `key` a word `kind` takes? Cell words (`span=`, `col_span=`) count as
/// legal on every box here — the compiler checks the PARENT for those.
pub fn legal(kind: Kind, key: PropKey) -> bool {
    // Parametric keys can't sit in the const rows: a numbered stop word
    // is a Gradient's, whatever the number.
    if matches!(key, PropKey::StopColor(_) | PropKey::StopOffset(_)) {
        return kind == Kind::Gradient;
    }
    let row = kind_row(kind);
    if row.iter().any(|g| g.contains(&key)) {
        return true;
    }
    !row.is_empty() && (GRID_CELL.contains(&key) || TILES_CELL.contains(&key))
}

/// The words `kind` takes, in table order — the docs list and the
/// did-you-mean pool.
pub fn kind_words(kind: Kind) -> Vec<&'static str> {
    kind_row(kind).iter().flat_map(|g| g.iter()).map(|k| k.word()).collect()
}

/// The words `kind` takes that not every box takes: the short list an
/// error shows ("a Scroll's own words: allow, bars").
pub fn kind_own_words(kind: Kind) -> Vec<&'static str> {
    kind_row(kind).iter().flat_map(|g| g.iter()).filter(|k| !BOX.contains(k)).map(|k| k.word()).collect()
}

/// The one authored-color type — every color a `.gogo` file names, a
/// `when` writes or a draw command carries: the file's `#RRGGBBAA`.
/// STRAIGHT alpha, sRGB-encoded bytes, RGBA order — nothing in gogo
/// holds a premultiplied byte color (premultiplying happens in the
/// shader at output, and in `color::OklabPm`, its own type). The name
/// duplicates imaging's color_types spelling on purpose — same concept,
/// but gogo stays dependency-free (asset-store bound).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[allow(non_camel_case_types)]
pub struct Color_Straight_Srgb_4xU8_RGBA(pub [u8; 4]);

impl Color_Straight_Srgb_4xU8_RGBA {
    pub const TRANSPARENT: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([0, 0, 0, 0]);
    /// The opaque color of `0xRRGGBB` — how the palette tables spell bytes.
    pub const fn opaque(hex: u32) -> Color_Straight_Srgb_4xU8_RGBA {
        Color_Straight_Srgb_4xU8_RGBA([(hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 0xFF])
    }
    pub fn with_opacity(self, opacity: f32) -> Color_Straight_Srgb_4xU8_RGBA {
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
    VColor(Color_Straight_Srgb_4xU8_RGBA),
    /// Both halves of a variant-flipping color word (`surface`, `text`,
    /// … — `page::MODE_COLORS`): compile keeps the pair, `runtime::deref`
    /// picks the half the active variant reads.
    VColorPair { light: Color_Straight_Srgb_4xU8_RGBA, dark: Color_Straight_Srgb_4xU8_RGBA },
    /// A displayed string: `text` is the literal as the file wrote it (the
    /// English), `key` its translation key or `NO_KEY`. With a key, what
    /// shows is `Locale::text(key)` when the runtime has a locale, else the
    /// literal (`locale::display`). A code-set key (`Runtime::set_key`)
    /// carries `text: NO_STR` — its English lives in the build's `KEYS`.
    VText { text: StrId, key: KeyId },
    /// An image (`source=`): the build-wide id of its path (`Images`).
    /// Its own kind, not text, so a path is never translated and a draw
    /// hands the renderer an index, not a string to hash. `NO_IMAGE` for
    /// `source=""`.
    VImage(ImageKey),
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
    VStop { color: Color_Straight_Srgb_4xU8_RGBA, pos: Option<f32> },
    /// `$item.<field>` inside a keyed repeat's template — the id is the
    /// field path after the binding (the empty string = the item itself).
    /// `Runtime::set_items` replaces it with the item's value on each
    /// clone, so no later pass ever meets one on a live node (GPAR108).
    VItem(StrId),
}

impl Value {
    /// The number inside a `VNum`, if that's what this value is.
    pub fn as_num(self) -> Option<f32> {
        match self {
            Value::VNum(n) => Some(n),
            _ => None,
        }
    }}

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
    Arrangement(ArrangementId),
    /// `screen.<orientation>` — `portrait` or `landscape` (the word).
    ScreenOrientation(StrId),
    /// `screen.<class>` — one of `SCREEN_CLASSES` (the word).
    ScreenClass(StrId),
    /// `when <arrangement> screen.<class|orientation>`: both must hold.
    ArrangementScreen(ArrangementId, StrId),
    /// `when screen.<orientation> screen.<class>`: both must hold.
    ScreenPair(StrId, StrId),
    /// A control's declared state (`state a, b` in its body), raised and
    /// cleared by the host through `Runtime::set_custom` — the control
    /// script's own vocabulary (GPAR116, the addon's declared_states).
    Custom(StrId),
}

/// The five screen classes a `when screen.<class>` may pose for — a CLOSED
/// set, so a file reads the same in every project; the host names the
/// live one (`Runtime::set_screen`), usually from gogo's own ruler
/// (`ScreenClass::from_view`, below); a host with its own policy can
/// classify itself and hand the result in instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenClass {
    PortraitSmall,
    PortraitMedium,
    PortraitLarge,
    LandscapeSmall,
    LotsOfSpace,
}

/// At or under this width (within `EPSILON`) an upright screen is
/// `portraitSmall`.
const SMALL_MAX_WIDTH_EPX: f32 = 350.0;
/// Past this a dimension has room for a second pane of content: an
/// upright screen wider than this is `portraitLarge`, a sideways screen
/// TALLER than this has `lotsOfSpace`.
const MEDIUM_MAX_EPX: f32 = 568.0;
const TABLET_MIN_WIDTH_EPX: f32 = 620.0;
/// Soaks up float noise in view sizes (a 349.99-epx width is small).
const EPSILON: f32 = 0.1;

impl ScreenClass {
    /// The word as written in a `when screen.<class>`.
    pub(crate) const fn word(self) -> &'static str {
        match self {
            ScreenClass::PortraitSmall => "portraitSmall",
            ScreenClass::PortraitMedium => "portraitMedium",
            ScreenClass::PortraitLarge => "portraitLarge",
            ScreenClass::LandscapeSmall => "landscapeSmall",
            ScreenClass::LotsOfSpace => "lotsOfSpace",
        }
    }

    /// The class a view size (epx) falls in — gogo's ruler, made for
    /// general-purpose apps. A sideways screen is graded by its HEIGHT
    /// (is there room under the content?), an upright one by its WIDTH.
    /// A host with its own policy can still classify itself and hand the
    /// result to `Runtime::set_screen`.
    pub fn from_view(view_width_epx: f32, view_height_epx: f32) -> ScreenClass {
        if view_width_epx > view_height_epx {
            if view_height_epx > MEDIUM_MAX_EPX - EPSILON {
                return ScreenClass::LotsOfSpace;
            }
            return ScreenClass::LandscapeSmall;
        }
        if view_width_epx < SMALL_MAX_WIDTH_EPX + EPSILON {
            return ScreenClass::PortraitSmall;
        }
        if view_width_epx < MEDIUM_MAX_EPX - EPSILON {
            return ScreenClass::PortraitMedium;
        }
        if view_width_epx < TABLET_MIN_WIDTH_EPX - EPSILON {
            return ScreenClass::PortraitLarge;
        }
        ScreenClass::LotsOfSpace
    }
}

/// The second, closed axis: which way the screen runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

impl Orientation {
    /// The word as written in a `when screen.<orientation>`.
    pub(crate) const fn word(self) -> &'static str {
        match self {
            Orientation::Portrait => "portrait",
            Orientation::Landscape => "landscape",
        }
    }

    /// Landscape = wider than tall; a square screen is portrait.
    pub fn from_view(view_width_epx: f32, view_height_epx: f32) -> Orientation {
        if view_width_epx > view_height_epx { Orientation::Landscape } else { Orientation::Portrait }
    }
}

/// Every class word, for the compiler's `when screen.*` check (G0036) —
/// derived from `ScreenClass` so the enum stays the one source.
pub const SCREEN_CLASSES: &[&str] = &[
    ScreenClass::PortraitSmall.word(),
    ScreenClass::PortraitMedium.word(),
    ScreenClass::PortraitLarge.word(),
    ScreenClass::LandscapeSmall.word(),
    ScreenClass::LotsOfSpace.word(),
];
/// Every orientation word, same purpose, derived from `Orientation`.
pub const SCREEN_ORIENTATIONS: &[&str] = &[Orientation::Portrait.word(), Orientation::Landscape.word()];

impl State {
    /// Rank when several `when` blocks pose the same key. Placement poses
    /// rank by how many conditions must hold: a composed pair (two) beats
    /// any single (arrangement, screen class or orientation — all one).
    /// Equal rank → the later line in the file wins (`When::line`), so the
    /// winner can be read off the file, CSS-style. Above the poses the
    /// built-in input states stack: focus_within < focused < hovered <
    /// selected < held < disabled.
    pub fn priority(self) -> u8 {
        match self {
            State::ScreenOrientation(_) | State::ScreenClass(_) | State::Arrangement(_) => 0,
            State::ArrangementScreen(..) | State::ScreenPair(..) => 1,
            State::FocusWithin => 2,
            State::Focused => 3,
            State::Hovered => 4,
            State::Selected => 5,
            State::Held => 6,
            // A control's own state implies held-plus-meaning, so it
            // beats `held` — but `disabled` beats everything, always: a
            // behavior can't out-rank the input blocker (the addon's
            // §5.3 law; declared states carry prio 100+, disabled 1000).
            State::Custom(_) => 7,
            State::StateDisabled => 8,
        }
    }
}

/// `when <state> prop=...` on one node: the props range in `Page::props`.
/// `of` = the node whose state is watched (`NO_NODE` = this node); a
/// style's `when held face.background=…` lands on `face` watching the
/// Button. `line` = the `when` block's source line — the tie-break between
/// blocks of equal `State::priority` (later line wins).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct When {
    pub state: State,
    pub props: (u32, u32),
    pub of: NodeId,
    pub line: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// A handler name the app matches on.
    Named(StrId),
    Go(ArrangementId),
    ToggleArrangement(ArrangementId),
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
    /// The file line, for the one-writer guard's message.
    pub line: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemeEntry {
    pub name: StrId,
    /// `NO_STR` = the defaults block.
    pub variant: StrId,
    pub value: Value,
}

/// `theme { Button background=... }` — a kind's default props. `variant`
/// gates the entry: NO_STR entries always apply, a named one only while
/// that variant is active (`theme fancy { font = b }` — GPAR88).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KindDefault {
    pub kind: Kind,
    pub props: (u32, u32),
    pub variant: StrId,
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
    /// `None` = `to=` alone: start from the live value when the track
    /// starts. A motion channel's endpoints are `VNum`; a prop lane's
    /// (GP40 color/non-motion lanes) are whatever its key takes
    /// (`VColor`, `VNum`…).
    pub from: Option<Value>,
    pub to: Value,
    pub start: f32,
    pub duration: f32,
    pub easing: StrId,
    /// `from=current`: the live value is captured at PLAY start (and held
    /// through the track's delay), not when the track begins.
    pub current: bool,
    /// `autoreverse=true`: out and back, the return leg with the same easing.
    pub autoreverse: bool,
}

/// An `anim` header: how the whole chain plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimHead {
    pub anim: StrId,
    /// `repeat=forever`.
    pub repeat: bool,
    /// `speed=` (1 = as written).
    pub speed: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Node {
    pub kind: Kind,
    /// The type word for `Custom`.
    pub type_name: StrId,
    pub name: StrId,
    /// The widget this node is an instance ROOT of (`SliderPhone` for
    /// `SliderPhone sliderSize { … }`), `NO_STR` for every other node.
    /// Needed so the page build can emit one `<Widget>Names` struct per
    /// widget and a typed entry per instance (`NAMES.slider_size.track`)
    /// — a widget code-behind takes the struct instead of finding body
    /// nodes by string path.
    pub widget: StrId,
    pub parent: NodeId,
    /// Direct children: first index and count (contiguous).
    pub children: (u32, u32),
    /// One past the last descendant. NOT a contiguous subtree: a parent's
    /// children are pushed together (so `children` is one range) before
    /// their own children, so later siblings and THEIR subtrees sit inside
    /// `id+1..subtree_end` too. Ask `Page::is_under` for membership; the
    /// range only bounds a scan. (A slider press landed on the picker
    /// beside it through a range test, 2026-09-05.)
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
    /// A Radio's `choice` group (index into `Page::choices`); `NO_CHOICE` = none.
    pub choice: u32,
    /// Keyboard-reachable (GP26), baked at compile: a press surface,
    /// Slider or Input, or a control root whose contract declares
    /// `state_builtin focused`; `keyboard_target=off` opts out.
    /// Disabled/hidden are runtime facts, checked at traversal.
    pub focusable: bool,
    pub line: u32,
}

pub const NO_CHOICE: u32 = u32::MAX;

/// A keyed `repeat` (the `by`-form, no `max=`): where its rows come
/// from and how an item's key is read. Needed because the rows exist
/// only at runtime — `Runtime::set_items` clones `template` per item
/// and reconciles by key (the shipped gogo_repeat.gd path).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RepeatInfo {
    pub node: NodeId,
    /// The item binding's name (`item`, or `as $brush`).
    pub binding: StrId,
    /// `by $item.<field>` — the key field; `NO_STR` = the item IS its
    /// key (`by $item`).
    pub by: StrId,
    /// The hidden template subtree's root (one root; a clone per slot).
    pub template: NodeId,
    /// `capacity=` — the row reservation the first growth jumps to.
    /// Never a ceiling: rows grow past it (a max that refused would
    /// truncate a user's data on screen; user design 2026-08-31).
    pub capacity: u32,
    /// `by` present: rows come from `set_items` (reconciled by key);
    /// else counted — `set_count` + `set_text` drive the slots.
    pub keyed: bool,
    /// The template's `$item.<field>` sites in `Page::bind_sites`
    /// (first, count) — recorded once so a reused slot can be re-bound
    /// in place with no `VItem` marker left on it.
    pub sites: (u32, u32),
}

/// One `$item.<field>` site inside a repeat template, template-relative
/// (so it addresses any slot by delta): which node (offset from the
/// template root), which prop (offset in that node's range — clones
/// share the range), and the field it binds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BindSite {
    pub node_off: u32,
    pub prop_off: u32,
    pub field: StrId,
}

/// A `choice { Radio … }` group: the one-of-N set every Radio inside
/// belongs to, and what its pick reports (`on:change=`, or the choice's
/// own name as the handler).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChoiceGroup {
    pub name: StrId,
    pub action: Option<Action>,
}

impl Node {
    pub const EMPTY: Node = Node {
        kind: Kind::Layers,
        type_name: NO_STR,
        name: NO_STR,
        widget: NO_STR,
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
        choice: NO_CHOICE,
        focusable: false,
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
];

/// Colors that follow the theme variant named `dark` (the well-known
/// name — any other variant reads the light half, the addon's rule).
/// Literal colors and the plain palette words never flip. `text_muted`'s
/// light half is `gray`'s bytes (GPAR111, user 2026-08-31).
pub const MODE_COLORS: &[(&str, u32, u32)] = &[
    ("surface", 0xFBFAF7, 0x1E1F22),
    ("panel", 0xFFFFFF, 0x27282C),
    ("border", 0xE4E2DC, 0x3A3B40),
    ("text", 0x2E3338, 0xE8E6E1),
    ("text_muted", 0x708090, 0x9AA3AD),
];

/// A palette word's color, aliases followed (`None` = not a palette word).
pub fn palette_color(word: &str) -> Option<Color_Straight_Srgb_4xU8_RGBA> {
    let word = PALETTE_ALIASES.iter().find(|(a, _)| *a == word).map_or(word, |(_, of)| *of);
    PALETTE.iter().find(|(w, _)| *w == word).map(|(_, hex)| Color_Straight_Srgb_4xU8_RGBA([(hex >> 16) as u8, (hex >> 8) as u8, *hex as u8, 0xFF]))
}

/// Property words the compiler accepts and no pass reads, each with the
/// reason. Needed because a key that parses and reaches nobody is a
/// silent drop (`selected=`, `disabled=` and a `choice`'s `value=` were
/// dropped for a round, 2026-08-30): the `every_prop_key_is_read` test
/// fails on any key that is neither read by layout/draw/runtime/input/anim
/// nor listed here.
pub const UNREAD_BY_DESIGN: &[(PropKey, &str)] = &[
    (PropKey::Loading, "host-read: `loading=lazy` asks the host to defer that image's decode to its first draw"),
    (PropKey::TouchTarget, "accepted: `touch_target=off` silences the 44-epx finger-target lint; the lint is not built here yet (GP41), and hit testing always uses the node's own rect"),
    (PropKey::AccessibilityName, "host-read: no screen-reader bridge here yet"),
    (PropKey::KeyboardTarget, "host-read: no keyboard focus routing here yet"),
    (PropKey::IconSrc, "accepted for old files; an icon file hint nothing reads"),
    (PropKey::IconSize, "accepted for old files beside `icon=`; icons are not drawn here"),
    (PropKey::Automatic, "read at compile into `Node.auto`, not as a prop"),
    (PropKey::SeatOf, "read at resolve through `Resolved.seat`, not as a prop"),
    (PropKey::Areas, "read at compile: `Compiler::grid_areas` expands it into `row { }` structure before the node builds"),
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
    pub anim_heads: &'static [AnimHead],
    pub choices: &'static [ChoiceGroup],
    pub repeats: &'static [RepeatInfo],
    pub bind_sites: &'static [BindSite],
    /// Control instance roots and the states their `state` lines declare
    /// — the `Runtime::set_custom` gate (GPAR116).
    pub custom_states: &'static [(NodeId, StrId)],
    /// Control instance roots and the key groups their contract's
    /// `keys` line claims (GP26) — `KEYS_*` bits. A focused instance
    /// takes a claimed key as a `Key` event for its code-behind;
    /// traversal never sees it.
    pub key_claims: &'static [(NodeId, u8)],
    pub drivers: &'static [AutoDriver],
    /// See `SliderBehavior` (GP52).
    pub behaviors: &'static [SliderBehavior],
    pub live_texts: &'static [LiveText],
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
    pub anim_heads: Vec<AnimHead>,
    pub choices: Vec<ChoiceGroup>,
    pub repeats: Vec<RepeatInfo>,
    pub bind_sites: Vec<BindSite>,
    /// See `StaticPage::custom_states`.
    pub custom_states: Vec<(NodeId, StrId)>,
    /// See `StaticPage::key_claims`.
    pub key_claims: Vec<(NodeId, u8)>,
    pub drivers: Vec<AutoDriver>,
    /// See `SliderBehavior` (GP52).
    pub behaviors: Vec<SliderBehavior>,
    pub live_texts: Vec<LiveText>,
}

/// The claimable key GROUPS of a `control`'s `keys` line (GP26) —
/// semantic groups, never raw key names, so a future remap or gamepad
/// layer leaves the claims standing. Tab has no bit: it is the way OFF
/// a control (the shipped router's law), never claimable.
pub const KEYS_ARROWS: u8 = 1;
pub const KEYS_HOME_END: u8 = 2;
pub const KEYS_ENTER: u8 = 4;
pub const KEYS_SPACE: u8 = 8;

impl Page {
    /// The `keys` claim on an instance root — 0 when nothing claims.
    pub fn key_claim(&self, node: NodeId) -> u8 {
        self.key_claims.iter().find(|(n, _)| *n == node).map(|(_, m)| *m).unwrap_or(0)
    }

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
            anim_heads: s.anim_heads.to_vec(),
            choices: s.choices.to_vec(),
            repeats: s.repeats.to_vec(),
            bind_sites: s.bind_sites.to_vec(),
            custom_states: s.custom_states.to_vec(),
            key_claims: s.key_claims.to_vec(),
            drivers: s.drivers.to_vec(),
            behaviors: s.behaviors.to_vec(),
            live_texts: s.live_texts.to_vec(),
        }
    }

    /// Appends a copy of `src`'s subtree at the arena's end — a repeat
    /// slot cloned from the template. Whens are copied per slot (a
    /// child-aimed response must watch the SLOT's nodes, not the
    /// template's); props and handlers are shared — a slot's bound
    /// values live in the Runtime's write layer, never in the arena.
    /// Node ids only ever grow: a freed slot is reused, never removed,
    /// so no id ever re-means.
    pub fn clone_subtree(&mut self, src: NodeId) -> NodeId {
        let end: NodeId = NodeId(self.nodes[src].subtree_end);
        let base: NodeId = NodeId::from_index(self.nodes.len());
        let delta: u32 = base.0 - src.0;
        let shift = |n: NodeId| NodeId(n.0 + delta);
        for i in src.0..end.0 {
            let i = NodeId(i);
            let mut n = self.nodes[i];
            if i != src {
                n.parent = shift(n.parent);
            }
            n.children.0 += delta;
            n.subtree_end += delta;
            let (w0, wc) = n.whens;
            let nw0 = self.whens.len() as u32;
            for wi in w0..w0 + wc {
                let mut w = self.whens[wi as usize];
                if w.of != NO_NODE && w.of >= src && w.of < end {
                    w.of = shift(w.of);
                }
                self.whens.push(w);
            }
            n.whens = (nw0, wc);
            self.nodes.push(n);
        }
        // A cloned control root declares the same states as its template
        // (a control inside a repeat row keeps its `set_custom` gate).
        let declared: Vec<(NodeId, StrId)> = self.custom_states.iter().filter(|(n, _)| *n >= src && *n < end).map(|(n, s)| (shift(*n), *s)).collect();
        self.custom_states.extend(declared);
        let claims: Vec<(NodeId, u8)> = self.key_claims.iter().filter(|(n, _)| *n >= src && *n < end).map(|(n, m)| (shift(*n), *m)).collect();
        self.key_claims.extend(claims);
        let behaviors: Vec<SliderBehavior> = self.behaviors.iter().filter(|b| b.root >= src && b.root < end).map(|b| SliderBehavior { root: shift(b.root), track: shift(b.track), ..*b }).collect();
        self.behaviors.extend(behaviors);
        base
    }

    pub fn str(&self, id: StrId) -> &str {
        &self.strings[id]
    }

    pub fn intern(&mut self, s: &str) -> StrId {
        if self.strings.is_empty() {
            self.strings.push(String::new());
        }
        if let Some(i) = self.strings.iter().position(|x| x == s) {
            return StrId::from_index(i);
        }
        self.strings.push(s.to_string());
        StrId::from_index(self.strings.len() - 1)
    }

    pub fn find(&self, name: &str) -> Option<NodeId> {
        self.nodes.iter().position(|n| n.name != NO_STR && self.str(n.name) == name).map(NodeId::from_index)
    }

    /// The direct children of `id`, in order (one contiguous arena run).
    pub fn children(&self, id: NodeId) -> impl DoubleEndedIterator<Item = NodeId> + ExactSizeIterator + Clone {
        let (first, count) = self.nodes[id].children;
        (first..first + count).map(NodeId::new)
    }

    /// Every node id in arena order.
    pub fn node_ids(&self) -> impl DoubleEndedIterator<Item = NodeId> + ExactSizeIterator + Clone {
        (0..self.nodes.len()).map(NodeId::from_index)
    }

    /// One past the last id `Node::subtree_end` bounds for `id` — the
    /// scan `id+1..subtree_end(id)` covers every descendant (and later
    /// siblings; ask `is_under` for membership).
    pub fn subtree_end(&self, id: NodeId) -> NodeId {
        NodeId(self.nodes[id].subtree_end)
    }

    /// The ids after `id` up to its `subtree_end` — a scan that covers
    /// every descendant (see `Node::subtree_end`: later siblings too).
    pub fn subtree_scan(&self, id: NodeId) -> impl DoubleEndedIterator<Item = NodeId> + ExactSizeIterator + Clone {
        NodeId::range(id.offset(1), self.subtree_end(id))
    }

    /// Whether `node` is `root` or sits anywhere under it — a parent walk,
    /// because the arena's index ranges are not subtrees (see
    /// `Node::subtree_end`). `NO_NODE` is under nothing.
    pub fn is_under(&self, node: NodeId, root: NodeId) -> bool {
        let mut n: NodeId = node;
        loop {
            if n == root {
                return true;
            }
            if n == NO_NODE || n.index() >= self.nodes.len() {
                return false;
            }
            n = self.nodes[n].parent;
        }
    }

    pub fn node_props(&self, id: NodeId) -> &[Prop] {
        let (first, count) = self.nodes[id].props;
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

    /// A `disabled` node dims like a built-in `when disabled opacity=N` at
    /// the bottom of the ladder: an authored `when disabled opacity=` on
    /// the node replaces it (`opacity=1` opts out). N is the theme's
    /// `disabled_opacity` (kept on the root kind's defaults), else 0.3.
    pub fn disabled_opacity(&self) -> f32 {
        match self.kind_default(Kind::View, PropKey::DisabledOpacity, NO_STR) {
            Some(Value::VNum(n)) => n,
            _ => DISABLED_OPACITY,
        }
    }

    /// A kind's default for `key`: an entry gated on the ACTIVE variant
    /// wins over an always-on one (GPAR88).
    pub fn kind_default(&self, kind: Kind, key: PropKey, variant: StrId) -> Option<Value> {
        let find = |want: StrId| {
            self.kind_defaults.iter().filter(|d| d.kind == kind && d.variant == want).find_map(|d| {
                self.props[d.props.0 as usize..(d.props.0 + d.props.1) as usize].iter().rev().find(|p| p.key == key).map(|p| p.value)
            })
        };
        if variant != NO_STR {
            if let Some(v) = find(variant) {
                return Some(v);
            }
        }
        find(NO_STR)
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
    use super::*;

    /// Every property word is read somewhere (or says why not in
    /// `UNREAD_BY_DESIGN`) AND has a row in the per-kind table — a word
    /// with no row can never be written, a word with no reader is a silent
    /// drop.
    #[test]
    fn every_prop_key_is_read() {
        let readers = [include_str!("layout.rs"), include_str!("draw.rs"), include_str!("runtime.rs"), include_str!("input.rs"), include_str!("anim.rs"), include_str!("compile.rs")];
        let mut unread: Vec<&str> = Vec::new();
        let mut homeless: Vec<&str> = Vec::new();
        for (word, key) in WORDS {
            let needle = format!("PropKey::{key:?}");
            // `compile.rs` reads a key when it matches on it outside `prop_key`
            // (the placement keys, Text, Wrap …); page.rs lines do not count.
            let read = readers.iter().any(|src| src.contains(&needle));
            let excused = UNREAD_BY_DESIGN.iter().any(|(k, _)| k == key);
            if !read && !excused {
                unread.push(word);
            }
            if !KIND_WORDS.iter().any(|(_, k)| legal(*k, *key)) && !legal(Kind::Custom, *key) {
                homeless.push(word);
            }
        }
        assert!(unread.is_empty(), "prop keys nobody reads — read them or list them in UNREAD_BY_DESIGN: {unread:?}");
        assert!(homeless.is_empty(), "prop keys no kind takes — give them a row in `kind_row`: {homeless:?}");
    }

    /// The per-kind word lists in docs/gogo-design.md are generated from
    /// the table; on a mismatch the test prints the block to paste.
    #[test]
    fn kind_words_doc_is_current() {
        let doc = include_str!("../../../docs/gogo-design.md");
        let start = "<!-- kind-words:start -->";
        let end = "<!-- kind-words:end -->";
        let generated = kind_words_markdown();
        let (a, b) = (doc.find(start).expect("start marker"), doc.find(end).expect("end marker"));
        let current = doc[a + start.len()..b].trim();
        assert!(current == generated.trim(), "docs/gogo-design.md kind-words block is stale — replace it with:\n\n{generated}");
    }

    /// `kind_words` per kind as a markdown table, one row per kind: the
    /// words every box takes once at the top, then each kind's own.
    fn kind_words_markdown() -> String {
        let mut out = String::new();
        out.push_str("Every box takes: ");
        out.push_str(&BOX.iter().map(|k| format!("`{}`", k.word())).collect::<Vec<_>>().join(", "));
        out.push_str(".\nA Grid cell adds `col_span`, `row_span`; a Tiles cell adds `span`, `full_row`.\n\n| kind | own words |\n|---|---|\n");
        let kinds = KIND_WORDS.iter().map(|(w, k)| (*w, *k)).chain(std::iter::once(("<code control>", Kind::Custom)));
        for (word, kind) in kinds {
            let own = kind_own_words(kind);
            let cell = if own.is_empty() { "(none — the line takes clauses, not properties)".to_string() } else { own.iter().map(|w| format!("`{w}`")).collect::<Vec<_>>().join(", ") };
            out.push_str(&format!("| `{word}` | {cell} |\n"));
        }
        out
    }
}

#[cfg(test)]
mod id_tests {
    use super::*;

    /// An id indexes a `Vec` and a slice by value; the raw number never
    /// shows outside this crate, so this is where the plumbing is pinned.
    #[test]
    fn ids_index_pools() {
        let v: Vec<u8> = vec![10, 20, 30];
        assert_eq!(v[NodeId::new(1)], 20);
        assert_eq!(v.as_slice()[StrId::from_index(2)], 30);
        let mut w: Vec<u8> = v.clone();
        w[KeyId::new(0)] = 7;
        assert_eq!(w[0], 7);
        assert_eq!(format!("{:?}", NodeId::new(3)), "NodeId::new(3)");
        assert_eq!(NodeId::range(NodeId::new(2), NodeId::new(5)).collect::<Vec<NodeId>>(), vec![NodeId::new(2), NodeId::new(3), NodeId::new(4)]);
        assert!(NodeId::new(2) < NO_NODE);
    }
}
