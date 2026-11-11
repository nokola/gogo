//! The one reader of placement words (`width=`, `margin_left=`,
//! `halign=` …): a node's line at build, and a view-level `when`, a code
//! write and an anim line while the page runs.

use crate::page::{Align, Len, NO_STR, Page, Placement, Prop, PropKey, StrId, Value};

/// What stopped `Placement::write` from reading a placement word.
///
/// Needed so one reader can serve a build and a running page: the
/// compiler turns each of these into its build error, and a code write
/// that slips keeps the reader's fallback (no size, 0, no alignment).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PlacementSlip {
    /// `width=host`, a word that is gone.
    HostAxis,
    /// A size word that is not `fill` or `hug`.
    SizeWord(StrId),
    /// A call in a size that is not `fill(n)` or `hug(n)`.
    SizeCall,
    /// A `$value` in a size that names no number.
    SizeRef,
    /// A size that is no number, word, call or `$value`.
    SizeOther,
    /// A `$value` where a number goes that names no number.
    NumberRef,
    /// Something else where a number goes.
    NumberOther,
    /// No alignment word of this axis.
    Alignment,
}

/// Is `v` the bare word `word`, in any letter case?
fn is_word(page: &Page, v: Value, word: &str) -> bool {
    matches!(v, Value::VIdent(id) if page.str(id).eq_ignore_ascii_case(word))
}

/// A size as `width=` takes it: a number, `fill`, `hug`, `fill(n)`,
/// `hug(n)` or a theme's `$number`.
///
/// Size words fold case like every enum word (`width=FILL`). `hug` alone
/// is weight 0, a box as big as its content with no share of the slot;
/// `hug(n)` is the weighted slot around a hugged box (GP34). A `$number`
/// reads the theme's default entry: a variant block cannot change a size.
fn size_of(page: &Page, v: Value) -> Result<Len, PlacementSlip> {
    match v {
        Value::VNum(n) => Ok(Len::Exact(n)),
        Value::VIdent(id) => {
            let word: &str = page.str(id);
            if word.eq_ignore_ascii_case("fill") {
                Ok(Len::Fill(1.0))
            } else if word.eq_ignore_ascii_case("hug") {
                Ok(Len::Hug(0.0))
            } else if word.eq_ignore_ascii_case("host") {
                Err(PlacementSlip::HostAxis)
            } else {
                Err(PlacementSlip::SizeWord(id))
            }
        }
        Value::VCall { name, args } => {
            let weight: f32 = if args.1 > 0 { page.values.get(args.0 as usize).and_then(|a| a.as_num()).unwrap_or(1.0) } else { 1.0 };
            match page.str(name) {
                "fill" => Ok(Len::Fill(weight)),
                "hug" => Ok(Len::Hug(weight)),
                _ => Err(PlacementSlip::SizeCall),
            }
        }
        Value::VRef { .. } => match crate::runtime::deref(page, NO_STR, v) {
            Value::VNum(n) => Ok(Len::Exact(n)),
            _ => Err(PlacementSlip::SizeRef),
        },
        _ => Err(PlacementSlip::SizeOther),
    }
}

/// A length in epx: a number, or a theme's `$number` (`min_width=$rowFont`,
/// GPAR88; the theme's default entry, as for a size).
fn number_of(page: &Page, v: Value) -> Result<f32, PlacementSlip> {
    match v {
        Value::VNum(n) => Ok(n),
        Value::VRef { .. } => match crate::runtime::deref(page, NO_STR, v) {
            Value::VNum(n) => Ok(n),
            _ => Err(PlacementSlip::NumberRef),
        },
        _ => Err(PlacementSlip::NumberOther),
    }
}

/// An alignment word of one axis: left/center/right, or top/center/bottom.
///
/// The words are per axis and physical: a word of the other axis would
/// quietly mean something else (`halign=top` pinned left), and the logical
/// `start`/`end` wait for right-to-left layouts (G14), where margins and
/// direction turn together.
fn align_of(page: &Page, v: Value, horizontal: bool) -> Result<Align, PlacementSlip> {
    if let Value::VIdent(id) = v {
        match (page.str(id), horizontal) {
            ("left", true) | ("top", false) => return Ok(Align::Start),
            ("center", _) => return Ok(Align::Center),
            ("right", true) | ("bottom", false) => return Ok(Align::End),
            _ => {}
        }
    }
    Err(PlacementSlip::Alignment)
}

impl Placement {
    /// Writes one placement word (`width=`, `margin_left=`, `halign=` …)
    /// into this placement. Every writer comes through here: a node's
    /// line at build, and a view-level `when`, a code write and an anim
    /// line while the page runs.
    ///
    /// Needed so a word means the same wherever it is written:
    /// `q.max_width = fill` in a `when` is the parent's width and
    /// `q.margin_bottom = safe` adds the host's inset, as on `q`'s line.
    ///
    /// The later word is the whole answer for its side: a number written
    /// over a `safe` margin or padding drops the inset, a number replaces
    /// a `fill` cap and `fill` replaces a number. A key that is no
    /// placement word writes nothing.
    ///
    /// Returns what stopped the word from being read, if anything; the
    /// field then holds the fallback: no size, 0, no alignment.
    pub(crate) fn write(&mut self, page: &Page, prop: Prop) -> Option<PlacementSlip> {
        fn or<T>(read: Result<T, PlacementSlip>, fallback: T, slip: &mut Option<PlacementSlip>) -> T {
            match read {
                Ok(x) => x,
                Err(s) => {
                    *slip = Some(s);
                    fallback
                }
            }
        }
        let v: Value = prop.value;
        let mut slip: Option<PlacementSlip> = None;
        match prop.key {
            PropKey::Width => self.width = or(size_of(page, v), Len::Unset, &mut slip),
            PropKey::Height => self.height = or(size_of(page, v), Len::Unset, &mut slip),
            PropKey::MinWidth => self.min_width = or(number_of(page, v), 0.0, &mut slip),
            PropKey::MinHeight => self.min_height = or(number_of(page, v), 0.0, &mut slip),
            PropKey::MaxWidth => {
                self.max_width_fill = is_word(page, v, "fill");
                self.max_width = if self.max_width_fill { None } else { Some(or(number_of(page, v), 0.0, &mut slip)) };
            }
            PropKey::MaxHeight => {
                self.max_height_fill = is_word(page, v, "fill");
                self.max_height = if self.max_height_fill { None } else { Some(or(number_of(page, v), 0.0, &mut slip)) };
            }
            PropKey::Halign => self.halign = or(align_of(page, v, true), Align::AlignUnset, &mut slip),
            PropKey::Valign => self.valign = or(align_of(page, v, false), Align::AlignUnset, &mut slip),
            // One margin side: a number, or `safe` (the host's inset for
            // that side, added at layout time).
            PropKey::MarginLeft | PropKey::MarginTop | PropKey::MarginRight | PropKey::MarginBottom => {
                let i: usize = match prop.key {
                    PropKey::MarginLeft => 0,
                    PropKey::MarginTop => 1,
                    PropKey::MarginRight => 2,
                    _ => 3,
                };
                self.margin_safe[i] = is_word(page, v, "safe");
                self.margin[i] = Some(if self.margin_safe[i] { 0.0 } else { or(number_of(page, v), 0.0, &mut slip) });
            }
            PropKey::Margin => {
                let safe: bool = is_word(page, v, "safe");
                self.margin_safe = [safe; 4];
                self.margin = [Some(if safe { 0.0 } else { or(number_of(page, v), 0.0, &mut slip) }); 4];
            }
            // One padding side: `safe` adds the inset to the number the
            // side already has.
            PropKey::PaddingLeft | PropKey::PaddingTop | PropKey::PaddingRight | PropKey::PaddingBottom => {
                let i: usize = match prop.key {
                    PropKey::PaddingLeft => 0,
                    PropKey::PaddingTop => 1,
                    PropKey::PaddingRight => 2,
                    _ => 3,
                };
                self.padding_safe[i] = is_word(page, v, "safe");
                if !self.padding_safe[i] {
                    self.padding[i] = or(number_of(page, v), 0.0, &mut slip);
                }
            }
            PropKey::Padding => {
                let safe: bool = is_word(page, v, "safe");
                self.padding_safe = [safe; 4];
                if !safe {
                    self.padding = [or(number_of(page, v), 0.0, &mut slip); 4];
                }
            }
            PropKey::Size => match v {
                Value::VSize(w, h) => {
                    if let Some(w) = w {
                        self.width = Len::Exact(w);
                    }
                    if let Some(h) = h {
                        self.height = Len::Exact(h);
                    }
                }
                // One sizing word for both sides (`size=hug`), read as
                // `width=` reads it.
                _ => {
                    let both: Len = or(size_of(page, v), Len::Unset, &mut slip);
                    self.width = both;
                    self.height = both;
                }
            },
            PropKey::MinSize => {
                if let Value::VSize(w, h) = v {
                    self.min_width = w.unwrap_or(0.0);
                    self.min_height = h.unwrap_or(0.0);
                }
            }
            _ => {}
        }
        slip
    }
}
