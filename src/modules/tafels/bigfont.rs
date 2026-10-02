//! A block-character font, big enough for a seven-year-old to read from across
//! the room.
//!
//! Every glyph is drawn on a grid of [`GLYPH_ROWS`] by [`GLYPH_COLS`] cells, each
//! cell either a full block or a space. [`Scale`] repeats every cell, which gives
//! three sizes out of one font. The digits stay recognisable at every size,
//! because they are the same font, just bigger.

/// Height of one glyph, in cells.
pub const GLYPH_ROWS: usize = 5;
/// Width of one glyph, in cells.
pub const GLYPH_COLS: usize = 4;

/// The multiplication sign the question uses.
pub const TIMES: char = '×';
/// The cursor drawn behind the learner's answer.
pub const CURSOR: char = '_';
/// Fallback for any character the font does not have.
pub const FALLBACK: char = '?';

/// How often every cell of a glyph is repeated, horizontally and vertically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Scale {
    pub rows: u16,
    pub cols: u16,
}

impl Scale {
    const fn new(rows: u16, cols: u16) -> Self {
        Self { rows, cols }
    }

    /// Height of one rendered glyph.
    pub fn height(self) -> u16 {
        self.rows.saturating_mul(GLYPH_ROWS as u16)
    }

    /// Width of one rendered glyph, without the gap to the next glyph.
    pub fn width(self) -> u16 {
        self.cols.saturating_mul(GLYPH_COLS as u16)
    }
}

/// The available sizes, smallest first.
pub const SCALES: [Scale; 3] = [Scale::new(1, 1), Scale::new(2, 2), Scale::new(3, 3)];

/// The block characters, one string per glyph row.
const BLOCK: &str = "█";
/// What an empty cell looks like.
const BLANK: &str = " ";

/// Every glyph, as [`GLYPH_ROWS`] rows of [`GLYPH_COLS`] cells.
///
/// The question mark doubles as the fallback for characters that are not in the
/// font, so [`glyph`] can never come back empty-handed.
const FALLBACK_ROWS: [&str; GLYPH_ROWS] = [" ██ ", "█  █", "  █ ", " █  ", " █  "];

const GLYPHS: &[(char, [&str; GLYPH_ROWS])] = &[
    ('0', [" ██ ", "█  █", "█  █", "█  █", " ██ "]),
    ('1', ["  █ ", " ██ ", "  █ ", "  █ ", "████"]),
    ('2', [" ██ ", "█  █", "  █ ", " █  ", "████"]),
    ('3', [" ██ ", "   █", " ██ ", "   █", " ██ "]),
    ('4', ["█  █", "█  █", "████", "   █", "   █"]),
    ('5', ["████", "█   ", "███ ", "   █", "███ "]),
    ('6', [" ██ ", "█   ", "███ ", "█  █", " ██ "]),
    ('7', ["████", "   █", "   █", "  █ ", " █  "]),
    ('8', [" ██ ", "█  █", " ██ ", "█  █", " ██ "]),
    ('9', [" ██ ", "█  █", " ███", "   █", " ██ "]),
    (TIMES, ["    ", "█  █", " ██ ", "█  █", "    "]),
    ('=', ["    ", "████", "    ", "████", "    "]),
    (FALLBACK, FALLBACK_ROWS),
    (CURSOR, ["    ", "    ", "    ", "    ", "████"]),
    (' ', ["    ", "    ", "    ", "    ", "    "]),
];

/// The rows of `character`, or the fallback glyph.
pub fn glyph(character: char) -> [&'static str; GLYPH_ROWS] {
    GLYPHS
        .iter()
        .find(|(key, _)| *key == character)
        .map(|(_, rows)| *rows)
        .unwrap_or(FALLBACK_ROWS)
}

/// Width in columns of `text` drawn at `scale`.
pub fn text_width(text: &str, scale: Scale) -> u16 {
    let count = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
    if count == 0 {
        return 0;
    }
    // Every glyph plus a gap of one cell, itself scaled.
    count
        .saturating_mul(scale.width())
        .saturating_add(count.saturating_sub(1).saturating_mul(scale.cols))
}

/// Height in rows of `text` drawn at `scale`; every glyph has the same height.
pub fn text_height(scale: Scale) -> u16 {
    scale.height()
}

/// Draws `text` at `scale`, one string per row.
///
/// Unknown characters become the fallback glyph, so a stray character can never
/// produce a blank space in the middle of a question.
pub fn render(text: &str, scale: Scale) -> Vec<String> {
    let rows = usize::from(scale.rows);
    let mut lines = vec![String::new(); GLYPH_ROWS * rows];
    let gap = scale.cols as usize;

    for (index, character) in text.chars().enumerate() {
        if index > 0 {
            for line in &mut lines {
                line.push_str(&BLANK.repeat(gap));
            }
        }
        for (row, pattern) in glyph(character).iter().enumerate() {
            let mut cell = String::new();
            for character in pattern.chars() {
                let block = if character == '█' { BLOCK } else { BLANK };
                cell.push_str(&block.repeat(usize::from(scale.cols)));
            }
            // Every glyph row covers `rows` lines: a stretched digit is never
            // squeezed, only repeated.
            for repetition in 0..rows {
                lines[row * rows + repetition].push_str(&cell);
            }
        }
    }
    lines
}

/// The largest scale at which `text` still fits, or `None` for plain text.
///
/// Plain text is the fallback, not a failure: a terminal too small for even the
/// smallest font still has to show the question somehow.
pub fn choose(text: &str, width: u16, height: u16) -> Option<Scale> {
    SCALES
        .iter()
        .rev()
        .copied()
        .find(|scale| text_width(text, *scale) <= width && text_height(*scale) <= height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_has_the_same_height_and_width() {
        for (character, rows) in GLYPHS {
            assert_eq!(rows.len(), GLYPH_ROWS, "hoogte van {character:?}");
            for row in rows {
                assert_eq!(
                    row.chars().count(),
                    GLYPH_COLS,
                    "breedte van {character:?} (rij {row:?})"
                );
                for cell in row.chars() {
                    assert!(
                        cell == '█' || cell == ' ',
                        "{character:?} gebruikt geen blokje maar {cell:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_digit_and_the_operators_have_a_glyph() {
        for character in ('0'..='9').chain([TIMES, '=', FALLBACK, CURSOR]) {
            let rows = glyph(character);
            assert_eq!(rows.len(), GLYPH_ROWS);
            assert!(
                rows.iter().any(|row| row.contains('█')),
                "{character:?} is leeg"
            );
        }
    }

    #[test]
    fn the_space_gives_the_digits_room_to_breathe() {
        // The space is the only glyph that draws nothing: it separates the
        // equation from the answer the learner is typing.
        let rows = glyph(' ');
        assert_eq!(rows.len(), GLYPH_ROWS);
        for row in rows {
            assert_eq!(row, " ".repeat(GLYPH_COLS), "{row:?}");
        }
    }

    #[test]
    fn an_unknown_character_becomes_the_fallback() {
        assert_eq!(glyph('é'), glyph(FALLBACK));
        assert_eq!(glyph('Z'), glyph(FALLBACK));
    }

    #[test]
    fn rendering_a_row_matches_the_glyph_at_scale_one() {
        let lines = render("7", SCALES[0]);
        assert_eq!(lines.len(), GLYPH_ROWS);
        assert_eq!(lines, glyph('7').map(str::to_string));
    }

    #[test]
    fn a_larger_scale_repeats_every_cell() {
        let small = render("7", SCALES[0]);
        let large = render("7", SCALES[1]);

        assert_eq!(large.len(), small.len() * 2);
        assert_eq!(large[0].chars().count(), small[0].chars().count() * 2);
        // Each original row appears twice, each cell stretched twice.
        for (index, row) in small.iter().enumerate() {
            let mut expected = String::new();
            for cell in row.chars() {
                expected.push_str(&cell.to_string().repeat(2));
            }
            assert_eq!(large[index * 2], expected);
            assert_eq!(large[index * 2 + 1], expected);
        }
    }

    #[test]
    fn rendered_text_keeps_the_glyphs_separated() {
        let lines = render("78", SCALES[0]);
        assert_eq!(lines.len(), GLYPH_ROWS);
        // Two glyphs plus one gap between them, and no trailing gap.
        assert_eq!(text_width("78", SCALES[0]), GLYPH_COLS as u16 * 2 + 1);
        for line in &lines {
            assert_eq!(line.chars().count(), GLYPH_COLS * 2 + 1, "{line:?}");
        }

        // A third glyph adds its own width and a second gap.
        assert_eq!(
            text_width("7×8", SCALES[0]),
            GLYPH_COLS as u16 * 3 + 2,
            "drie tekens en twee tussenruimtes"
        );
        for line in render("7×8", SCALES[0]) {
            assert_eq!(line.chars().count(), GLYPH_COLS * 3 + 2, "{line:?}");
        }
    }

    #[test]
    fn the_width_and_height_match_what_is_rendered() {
        let text = "10 × 10 = 100_";
        for scale in SCALES {
            let lines = render(text, scale);
            assert_eq!(lines.len() as u16, text_height(scale));
            for line in &lines {
                assert_eq!(line.chars().count() as u16, text_width(text, scale));
            }
        }
    }

    #[test]
    fn an_empty_string_renders_nothing() {
        assert!(render("", SCALES[0]).iter().all(String::is_empty));
        assert_eq!(text_width("", SCALES[0]), 0);
        assert_eq!(text_height(SCALES[0]), GLYPH_ROWS as u16);
    }

    #[test]
    fn the_largest_font_that_fits_is_chosen() {
        let text = "10 × 10 = 100_";
        let needed = text_width(text, SCALES[1]);

        // Exactly wide and tall enough for the middle size: not the smallest.
        let scale = choose(text, needed, text_height(SCALES[1])).expect("de middelste past");
        assert_eq!(scale, SCALES[1]);

        // One row short of the middle size: fall back to the smallest.
        let scale = choose(text, needed, text_height(SCALES[1]) - 1).expect("de kleinste past");
        assert_eq!(scale, SCALES[0]);

        // Room for the biggest: it wins.
        let scale = choose(text, text_width(text, SCALES[2]), text_height(SCALES[2]))
            .expect("de grootste past");
        assert_eq!(scale, SCALES[2]);
    }

    #[test]
    fn a_terminal_too_small_for_the_font_falls_back_to_plain_text() {
        let text = "10 × 10 = 100_";
        assert_eq!(choose(text, text_width(text, SCALES[0]) - 1, 24), None);
        assert_eq!(choose(text, 0, 0), None);
        assert_eq!(choose(text, 200, 3), None);
    }

    #[test]
    fn scales_grow() {
        let mut previous = 0;
        for scale in SCALES {
            assert!(
                scale.height() > previous,
                "schaal {scale:?} wordt niet groter"
            );
            previous = scale.height();
        }
    }
}
