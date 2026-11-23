//! Praat treats field names and bracketed indices as comments in both text formats.
use super::{Interval, Point, TextGrid, Tier};
use std::fmt::{self, Write};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextGridError(String);

impl fmt::Display for TextGridError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TextGridError {}

fn error(message: impl Into<String>) -> TextGridError {
    TextGridError(message.into())
}

#[derive(Debug)]
enum Token<'a> {
    Quoted(String),
    Bare(&'a str),
}

struct Scanner<'a> {
    rest: &'a str,
    consumed: usize,
}

impl<'a> Scanner<'a> {
    fn new(rest: &'a str) -> Self {
        Self { rest, consumed: 0 }
    }

    fn advance(&mut self, bytes: usize) {
        self.rest = &self.rest[bytes..];
        self.consumed += bytes;
    }

    fn next(&mut self) -> Result<Option<Token<'a>>, TextGridError> {
        loop {
            self.advance(self.rest.len() - self.rest.trim_start().len());
            let Some(first) = self.rest.chars().next() else { return Ok(None) };
            if first == '!' {
                self.advance(self.rest.find(['\r', '\n']).unwrap_or(self.rest.len()));
                continue;
            }
            if first == '"' {
                let start = self.consumed;
                self.advance(1);
                let mut value = String::new();
                loop {
                    let pos = self.rest.find('"').ok_or_else(|| {
                        error(format!("Unterminated quoted text at byte {start}"))
                    })?;
                    value.push_str(&self.rest[..pos]);
                    self.advance(pos + 1);
                    if !self.rest.starts_with('"') {
                        if self.rest.chars().next().is_some_and(|c| !c.is_whitespace() && c != '!') {
                            return Err(error(format!("Quoted text at byte {start} is not separated from the next field")));
                        }
                        return Ok(Some(Token::Quoted(value)));
                    }
                    value.push('"');
                    self.advance(1);
                }
            }
            let end = self.rest.find(|c: char| c.is_whitespace() || c == '!').unwrap_or(self.rest.len());
            let word = &self.rest[..end];
            self.advance(end);
            if word.starts_with('<') || word.parse::<f64>().is_ok() {
                return Ok(Some(Token::Bare(word)));
            }
        }
    }

    fn required(&mut self, context: &str) -> Result<Token<'a>, TextGridError> {
        self.next()?.ok_or_else(|| error(format!("Missing {context}")))
    }

    fn quoted(&mut self, context: &str) -> Result<String, TextGridError> {
        match self.required(context)? {
            Token::Quoted(value) => Ok(value),
            _ => Err(error(format!("Expected quoted text for {context}"))),
        }
    }

    fn number<T: std::str::FromStr>(&mut self, context: &str) -> Result<T, TextGridError> {
        if let Token::Bare(value) = self.required(context)? {
            if let Ok(number) = value.parse() {
                return Ok(number);
            }
        }
        Err(error(format!("Invalid number for {context}")))
    }
}

impl TextGrid {
    /// Parse UTF-8 or BOM-marked UTF-16 Praat text, without guessing legacy encodings.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TextGridError> {
        if let Some(payload) = bytes.strip_prefix(b"\xff\xfe").or_else(|| bytes.strip_prefix(b"\xfe\xff")) {
            let (pairs, remainder) = payload.as_chunks::<2>();
            if !remainder.is_empty() {
                return Err(error("Truncated UTF-16 TextGrid"));
            }
            let little_endian = bytes.starts_with(b"\xff\xfe");
            let units: Vec<_> = pairs.iter().copied().map(|pair| {
                if little_endian { u16::from_le_bytes(pair) } else { u16::from_be_bytes(pair) }
            }).collect();
            let text = String::from_utf16(&units).map_err(|_| error("Invalid UTF-16 TextGrid"))?;
            Self::parse(&text)
        } else {
            let text = std::str::from_utf8(bytes)
                .map_err(|_| error("Expected UTF-8 or BOM-marked UTF-16; resave this file as Praat text"))?;
            Self::parse(text)
        }
    }

    /// Parse Praat long or short text. Quoted strings retain their literal text,
    /// including newlines and Praat backslash trigraphs; doubled quotes are decoded.
    pub fn parse(text: &str) -> Result<Self, TextGridError> {
        let mut reader = Scanner::new(text.strip_prefix('\u{feff}').unwrap_or(text));
        let file_type = reader.quoted("file type")?;
        if !matches!(file_type.as_str(), "ooTextFile" | "ooTextFile short") {
            return Err(error("Expected a Praat text TextGrid (binary is not supported)"));
        }
        if reader.quoted("object class")? != "TextGrid" {
            return Err(error("The Praat object is not a TextGrid"));
        }
        let xmin = reader.number("grid xmin")?;
        let xmax = reader.number("grid xmax")?;
        let tier_count: usize = match reader.required("tiers flag")? {
            Token::Bare("<exists>") => reader.number("tier count")?,
            Token::Bare("<absent>") => 0,
            _ => return Err(error("Expected <exists> or <absent> for the tiers flag")),
        };
        // Do not allocate using untrusted counts. A truncated file fails on the
        // first missing field rather than attempting to allocate declared entries.
        let mut tiers = Vec::new();
        for index in 1..=tier_count {
            let class = reader.quoted(&format!("tier {index} class"))?;
            let name = reader.quoted("tier name")?;
            let xmin = reader.number("tier xmin")?;
            let xmax = reader.number("tier xmax")?;
            let count = reader.number::<usize>("tier entry count")?;
            let tier = match class.as_str() {
                "IntervalTier" => {
                    let mut intervals = Vec::new();
                    for _ in 0..count {
                        intervals.push(Interval {
                            start: reader.number("interval xmin")?,
                            end: reader.number("interval xmax")?,
                            label: reader.quoted("interval text")?,
                        });
                    }
                    Tier::Interval { name, xmin, xmax, intervals }
                }
                "TextTier" => {
                    let mut points = Vec::new();
                    for _ in 0..count {
                        points.push(Point {
                            time: reader.number("point time")?,
                            label: reader.quoted("point mark")?,
                        });
                    }
                    Tier::Point { name, xmin, xmax, points }
                }
                _ => return Err(error(format!("Unsupported tier {index} class: {class}"))),
            };
            tiers.push(tier);
        }
        if reader.next()?.is_some() {
            return Err(error("Unexpected data after the final TextGrid tier"));
        }
        let grid = Self { xmin, xmax, tiers };
        grid.validate()?;
        Ok(grid)
    }

    /// Check strong Praat invariants. Tier domains and interval coverage are
    /// intentionally independent of one another and of the grid's domain.
    pub fn validate(&self) -> Result<(), TextGridError> {
        fn domain(xmin: f64, xmax: f64) -> bool {
            xmin.is_finite() && xmax.is_finite() && xmin <= xmax
        }
        if !domain(self.xmin, self.xmax) {
            return Err(error("Grid domain must have finite, ordered bounds"));
        }
        for (index, tier) in self.tiers.iter().enumerate() {
            let n = index + 1;
            match tier {
                Tier::Interval { xmin, xmax, intervals, .. } => {
                    if !domain(*xmin, *xmax) {
                        return Err(error(format!("Tier {n} domain must have finite, ordered bounds")));
                    }
                    let mut previous_end = None;
                    for (index, interval) in intervals.iter().enumerate() {
                        if !domain(interval.start, interval.end) || interval.start == interval.end {
                            return Err(error(format!("Tier {n}, interval {} must have finite bounds and positive duration", index + 1)));
                        }
                        if previous_end.is_some_and(|end| end != interval.start) {
                            return Err(error(format!("Tier {n}, interval {} must start where the previous interval ends", index + 1)));
                        }
                        previous_end = Some(interval.end);
                    }
                }
                Tier::Point { xmin, xmax, points, .. } => {
                    if !domain(*xmin, *xmax) {
                        return Err(error(format!("Tier {n} domain must have finite, ordered bounds")));
                    }
                    let mut previous_time = None;
                    for (index, point) in points.iter().enumerate() {
                        if !point.time.is_finite() || previous_time.is_some_and(|time| time >= point.time) {
                            return Err(error(format!("Tier {n}, point {} must have a finite time and follow the previous point", index + 1)));
                        }
                        previous_time = Some(point.time);
                    }
                }
            }
        }
        Ok(())
    }

    /// Serialize canonical Praat long text. Formatting/comments and original encoding
    /// are not retained, but annotation data and time domains are.
    pub fn to_praat_long(&self) -> Result<String, TextGridError> {
        self.validate()?;
        let mut out = String::new();
        fn quoted(s: &str) -> String { format!("\"{}\"", s.replace('"', "\"\"")) }
        // Writing to String is infallible.
        writeln!(out, "File type = \"ooTextFile\"\nObject class = \"TextGrid\"\n\nxmin = {}\nxmax = {}\ntiers? <exists>\nsize = {}\nitem []:", self.xmin, self.xmax, self.tiers.len()).unwrap();
        for (index, tier) in self.tiers.iter().enumerate() {
            writeln!(out, "    item [{}]:", index + 1).unwrap();
            match tier {
                Tier::Interval { name, xmin, xmax, intervals } => {
                    writeln!(out, "        class = \"IntervalTier\"\n        name = {}\n        xmin = {xmin}\n        xmax = {xmax}\n        intervals: size = {}", quoted(name), intervals.len()).unwrap();
                    for (i, interval) in intervals.iter().enumerate() {
                        writeln!(out, "        intervals [{}]:\n            xmin = {}\n            xmax = {}\n            text = {}", i + 1, interval.start, interval.end, quoted(&interval.label)).unwrap();
                    }
                }
                Tier::Point { name, xmin, xmax, points } => {
                    writeln!(out, "        class = \"TextTier\"\n        name = {}\n        xmin = {xmin}\n        xmax = {xmax}\n        points: size = {}", quoted(name), points.len()).unwrap();
                    for (i, point) in points.iter().enumerate() {
                        writeln!(out, "        points [{}]:\n            number = {}\n            mark = {}", i + 1, point.time, quoted(&point.label)).unwrap();
                    }
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHORT: &str = r#"File type = "ooTextFile short"
"TextGrid"

-0.25 2.3 ! grid domain
<exists>
2
"IntervalTier" "phonèmes"
0 2.3
2
0 1 "říkej ""ahoj!"""
1 2 ""
"TextTier"
"bell ""1"""
-0.5 3
2
0.9 "diŋ"
1.3 "\ct
second line"
"#;

    const LONG: &str = r#"File type = "ooTextFile"
Object class = "TextGrid"

xmin = -0.25
xmax = 2.3
tiers? <exists>
size = 2
item []:
    item [7]:
        class = "IntervalTier"
        name = "phonèmes"
        xmin = 0
        xmax = 2.3
        intervals: size = 2
        intervals [14]:
            xmin = 0
            xmax = 1
            text = "říkej ""ahoj!"""
        intervals [2]:
            xmin = 1
            xmax = 2
            text = ""
    item [2]:
        class = "TextTier"
        name = "bell ""1"""
        xmin = -0.5
        xmax = 3
        points: size = 2
        points [1]:
            time = 0.9
            mark = "diŋ"
        points [2]:
            number = 1.3
            mark = "\ct
second line"
"#;

    #[test]
    fn long_and_short_preserve_annotation_data_and_domains() {
        let grid = TextGrid::parse(SHORT).unwrap();
        assert_eq!(TextGrid::parse(LONG).unwrap(), grid);
        assert_eq!((grid.xmin, grid.xmax), (-0.25, 2.3));
        assert!(matches!(&grid.tiers[0], Tier::Interval { xmin, xmax, intervals, .. }
            if (*xmin, *xmax) == (0.0, 2.3) && intervals[0].label == "říkej \"ahoj!\"" && intervals[1].label.is_empty()));
        assert!(matches!(&grid.tiers[1], Tier::Point { xmin, xmax, name, points }
            if (*xmin, *xmax) == (-0.5, 3.0) && name == "bell \"1\"" && points[1].label == "\\ct\nsecond line"));
        let emitted = grid.to_praat_long().unwrap();
        assert!(emitted.contains("number = 1.3"));
        assert!(emitted.contains("říkej \"\"ahoj!\"\""));
        assert_eq!(TextGrid::parse(&emitted).unwrap(), grid);
    }

    #[test]
    fn byte_encodings_and_line_endings() {
        let grid = TextGrid::parse(SHORT).unwrap();
        assert_eq!(TextGrid::from_bytes(format!("\u{feff}{SHORT}").as_bytes()).unwrap(), grid);
        for little_endian in [true, false] {
            let mut bytes = if little_endian { vec![0xff, 0xfe] } else { vec![0xfe, 0xff] };
            for unit in SHORT.encode_utf16() {
                bytes.extend(if little_endian { unit.to_le_bytes() } else { unit.to_be_bytes() });
            }
            assert_eq!(TextGrid::from_bytes(&bytes).unwrap(), grid);
        }
        let simple = "File type = \"ooTextFile\"\n\"TextGrid\"\n0 ! comment 5\n1\n<absent>\n";
        for sep in ["\n", "\r", "\r\n"] {
            assert!(TextGrid::parse(&simple.replace('\n', sep)).unwrap().tiers.is_empty());
        }
    }

    #[test]
    fn malformed_or_unsupported_files_are_errors() {
        for invalid in [
            SHORT.replace("\"TextGrid\"", "\"Sound\""),
            SHORT.replace("\"TextTier\"", "\"MysteryTier\""),
            SHORT.replace("0 2.3\n2", "0 2.3\n999999999"),
            SHORT.replace("1 2 \"\"", "1.1 2 \"\""),
            SHORT.replace("1 2 \"\"", "0.9 2 \"\""),
            SHORT.replace("1 2 \"\"", "1 1 \"\""),
            SHORT.replace("1.3", "0.9"),
            SHORT.replace("1.3", "NaN"),
            SHORT.replace("-0.25 2.3", "2.4 2.3"),
            format!("{SHORT}\"unexpected trailing text\""),
            SHORT.trim_end().trim_end_matches('"').to_owned(),
        ] {
            assert!(TextGrid::parse(&invalid).is_err(), "accepted {invalid}");
        }
        assert!(TextGrid::from_bytes(b"ooBinaryFile\x08TextGrid").is_err());
        assert!(TextGrid::from_bytes(&[0xff, 0xfe, 0]).is_err());
        assert!(TextGrid::from_bytes(&[0xff]).is_err());
    }

    #[test]
    fn writer_rejects_invalid_in_memory_grid() {
        let mut grid = TextGrid::parse(SHORT).unwrap();
        grid.xmax = f64::INFINITY;
        assert!(grid.to_praat_long().is_err());
    }
}
