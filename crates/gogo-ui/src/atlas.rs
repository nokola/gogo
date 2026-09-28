//! Build-time helpers for `atlas` lines: a picture's size from its file
//! header, a small JSON reader, the TexturePacker "JSON (Hash)" / "JSON
//! (Array)" import, and the rule that turns a packer's file names into
//! gogo names. Nothing here runs on the phone: the build bakes every
//! slice into the `SLICES` table.
//!
//! Needed because gogo stays free of other crates (it compiles into
//! every host), and an atlas must be checked at build time — a grid's
//! cell count and a slice's bounds come from the picture's real size.

/// Width and height from a PNG or JPEG header, without decoding pixels.
/// `None` for any other format or a damaged header.
pub fn picture_size(bytes: &[u8]) -> Option<(u32, u32)> {
    // PNG: the 8-byte signature, then the IHDR chunk (length, "IHDR",
    // width, height — big-endian).
    if bytes.len() >= 24 && bytes[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] && &bytes[12..16] == b"IHDR" {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return Some((w, h));
    }
    // JPEG: walk the marker segments to the first start-of-frame
    // (SOF0..SOF15 except DHT C4, JPG C8, DAC CC), whose body holds
    // precision, height, width.
    if bytes.len() >= 4 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        let mut i: usize = 2;
        while i + 4 <= bytes.len() {
            if bytes[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            // Fill bytes and standalone markers carry no length.
            if marker == 0xFF {
                i += 1;
                continue;
            }
            if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
                i += 2;
                continue;
            }
            let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                if i + 9 > bytes.len() {
                    return None;
                }
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                return Some((w, h));
            }
            i += 2 + len;
        }
    }
    None
}

/// A parsed JSON value. Objects keep their keys in file order.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// The member `key` of an object; `None` for a missing key or a
    /// value that is not an object.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// A whole number (`12` or `12.0`); `None` for a fraction, a
    /// negative number or anything that is not a number.
    pub fn as_whole(&self) -> Option<u32> {
        match self {
            Json::Num(n) if *n >= 0.0 && n.fract() == 0.0 && *n <= u32::MAX as f64 => Some(*n as u32),
            _ => None,
        }
    }
}

/// Parses `text` as one JSON value. The error names the line.
pub fn parse_json(text: &str) -> Result<Json, String> {
    let mut p = JsonParser { bytes: text.as_bytes(), pos: 0 };
    p.skip_space();
    let v = p.value()?;
    p.skip_space();
    if p.pos != p.bytes.len() {
        return Err(p.fail("text after the JSON value"));
    }
    Ok(v)
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl JsonParser<'_> {
    fn fail(&self, what: &str) -> String {
        let line = 1 + self.bytes[..self.pos.min(self.bytes.len())].iter().filter(|b| **b == b'\n').count();
        format!("line {line}: {what}")
    }

    fn skip_space(&mut self) {
        while self.pos < self.bytes.len() && matches!(self.bytes[self.pos], b' ' | b'\t' | b'\n' | b'\r') {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn expect(&mut self, b: u8) -> Result<(), String> {
        if self.peek() == Some(b) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.fail(&format!("expected `{}`", b as char)))
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.word("true", Json::Bool(true)),
            Some(b'f') => self.word("false", Json::Bool(false)),
            Some(b'n') => self.word("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.fail("expected a value")),
            None => Err(self.fail("the file ended where a value was expected")),
        }
    }

    fn word(&mut self, word: &str, v: Json) -> Result<Json, String> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(v)
        } else {
            Err(self.fail("expected a value"))
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        while matches!(self.peek(), Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')) {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or("");
        text.parse::<f64>().map(Json::Num).map_err(|_| self.fail(&format!("bad number `{text}`")))
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(b) = self.peek() else { return Err(self.fail("a string never closes")) };
            self.pos += 1;
            match b {
                b'"' => break,
                b'\\' => {
                    let Some(e) = self.peek() else { return Err(self.fail("a string never closes")) };
                    self.pos += 1;
                    match e {
                        b'"' | b'\\' | b'/' => out.push(e),
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'u' => {
                            let hex = self.bytes.get(self.pos..self.pos + 4).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u32::from_str_radix(h, 16).ok());
                            let Some(code) = hex else { return Err(self.fail("bad `\\u` escape")) };
                            self.pos += 4;
                            let c = char::from_u32(code).unwrap_or('\u{FFFD}');
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return Err(self.fail("unknown escape in a string")),
                    }
                }
                _ => out.push(b),
            }
        }
        String::from_utf8(out).map_err(|_| self.fail("a string is not UTF-8"))
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_space();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            self.skip_space();
            items.push(self.value()?);
            self.skip_space();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(self.fail("expected `,` or `]`")),
            }
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut members = Vec::new();
        self.skip_space();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Obj(members));
        }
        loop {
            self.skip_space();
            let key = self.string()?;
            self.skip_space();
            self.expect(b':')?;
            self.skip_space();
            let v = self.value()?;
            members.push((key, v));
            self.skip_space();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Obj(members));
                }
                _ => return Err(self.fail("expected `,` or `}`")),
            }
        }
    }
}

/// One frame of a packer's JSON, in picture pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// The packer's name for it, as written (`buttons/play.png`).
    pub file: String,
    /// Where the frame's pixels sit in the picture: x, y, w, h.
    pub rect: [u32; 4],
    /// A trimmed frame: its size before trimming and where `rect` sat in
    /// that box (w, h, x, y). `None` = not trimmed, or trimmed to the
    /// same box.
    pub untrimmed: Option<[u32; 4]>,
}

/// What a packer's JSON says.
#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    /// `meta.image`: the picture's file name, relative to the JSON.
    pub image: String,
    /// `meta.size`, when written: the picture's size the packer made.
    pub size: Option<(u32, u32)>,
    pub frames: Vec<Frame>,
}

/// Reads a TexturePacker "JSON (Hash)" or "JSON (Array)" file (also what
/// Free Texture Packer, ShoeBox and Aseprite write). Rotated frames are
/// refused: the packer can be told not to rotate.
pub fn import_texture_packer(text: &str) -> Result<Import, String> {
    let json = parse_json(text)?;
    let meta = json.get("meta").ok_or("no `meta` object — this does not look like a TexturePacker/Aseprite JSON export")?;
    let image = meta.get("image").and_then(Json::as_str).ok_or("`meta.image` is missing — the export must name its picture")?.to_string();
    let size = meta.get("size").and_then(|s| Some((s.get("w")?.as_whole()?, s.get("h")?.as_whole()?)));
    let entries: Vec<(String, &Json)> = match json.get("frames") {
        Some(Json::Obj(members)) => members.iter().map(|(k, v)| (k.clone(), v)).collect(),
        Some(Json::Arr(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let name = item.get("filename").and_then(Json::as_str).ok_or("a frame in the `frames` list has no `filename`")?;
                out.push((name.to_string(), item));
            }
            out
        }
        _ => return Err("no `frames` object or list".to_string()),
    };
    let mut frames = Vec::with_capacity(entries.len());
    for (file, v) in entries {
        if v.get("rotated").and_then(Json::as_bool) == Some(true) {
            return Err(format!("`{file}` is rotated — turn off rotation in the packer (gogo draws slices upright)"));
        }
        let rect = rect4(v.get("frame")).ok_or_else(|| format!("`{file}`: `frame` needs whole-number x, y, w, h"))?;
        let untrimmed = if v.get("trimmed").and_then(Json::as_bool) == Some(true) {
            let source = v.get("sourceSize").ok_or_else(|| format!("`{file}` is trimmed but has no `sourceSize`"))?;
            let (sw, sh) = (source.get("w").and_then(Json::as_whole), source.get("h").and_then(Json::as_whole));
            let placed = rect4(v.get("spriteSourceSize")).ok_or_else(|| format!("`{file}` is trimmed but has no whole-number `spriteSourceSize`"))?;
            match (sw, sh) {
                (Some(sw), Some(sh)) if placed[0] + rect[2] <= sw && placed[1] + rect[3] <= sh => {
                    // Trimmed to the same box is no trim at all.
                    if sw == rect[2] && sh == rect[3] { None } else { Some([sw, sh, placed[0], placed[1]]) }
                }
                _ => return Err(format!("`{file}`: its trimmed pixels do not fit inside its `sourceSize`")),
            }
        } else {
            None
        };
        frames.push(Frame { file, rect, untrimmed });
    }
    Ok(Import { image, size, frames })
}

fn rect4(v: Option<&Json>) -> Option<[u32; 4]> {
    let v = v?;
    Some([v.get("x")?.as_whole()?, v.get("y")?.as_whole()?, v.get("w")?.as_whole()?, v.get("h")?.as_whole()?])
}

/// The gogo name for a packer's file name: the extension dropped,
/// lowercase, every character other than a letter, digit or `_` turned
/// into `_`, and a `_` in front when it would start with a digit.
/// `buttons/Play-Hover.png` → `buttons_play_hover`, `01.png` → `_01`.
pub fn slice_name(file: &str) -> String {
    let last_part = file.rsplit('/').next().unwrap_or(file);
    let stem_len = match last_part.rfind('.') {
        Some(dot) if dot > 0 && last_part[dot + 1..].chars().all(|c| c.is_ascii_alphanumeric()) => file.len() - (last_part.len() - dot),
        _ => file.len(),
    };
    let mut name: String = file[..stem_len].chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert(0, '_');
    }
    name
}

/// The folder part of a `res://` path, with its trailing `/`
/// (`res://ui/icons.json` → `res://ui/`).
pub fn folder_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..=i],
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_and_jpeg_sizes_come_from_the_header() {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&480u32.to_be_bytes());
        png.extend_from_slice(&720u32.to_be_bytes());
        assert_eq!(picture_size(&png), Some((480, 720)));
        // SOI, an APP0 segment, then SOF0 (len, precision, h, w).
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x50, 0x01, 0xE0];
        assert_eq!(picture_size(&jpeg), Some((480, 80)));
        assert_eq!(picture_size(b"GIF89a"), None);
    }

    #[test]
    fn json_values_parse_with_escapes_and_keep_key_order() {
        let v = parse_json(r#"{"b": [1, 2.5, -3e2], "a": "x\"yé", "t": true, "n": null}"#).unwrap();
        assert_eq!(v.get("a").and_then(Json::as_str), Some("x\"y\u{e9}"));
        assert_eq!(v.get("b"), Some(&Json::Arr(vec![Json::Num(1.0), Json::Num(2.5), Json::Num(-300.0)])));
        if let Json::Obj(members) = &v {
            assert_eq!(members.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), ["b", "a", "t", "n"]);
        }
        assert!(parse_json("{\"a\": 1,\n \"b\" 2}").unwrap_err().starts_with("line 2:"));
    }

    #[test]
    fn packer_names_become_gogo_names() {
        assert_eq!(slice_name("buttons/Play-Hover.png"), "buttons_play_hover");
        assert_eq!(slice_name("01.png"), "_01");
        assert_eq!(slice_name("hero 0.aseprite"), "hero_0");
        assert_eq!(slice_name("no_extension"), "no_extension");
        assert_eq!(slice_name(".hidden"), "_hidden");
    }
}
