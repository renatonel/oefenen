//! Which multiplication facts a session asks, and in which order.
//!
//! The available range lives in [`TABLE_MIN`] and [`TABLE_MAX`] only, so
//! offering tables above ten is a one-line change. The same pair also fixes how
//! many facts each table contributes ([`MULTIPLIER_MAX`]), which is what makes a
//! "tafel" a fixed set of ten products.

use std::collections::HashSet;

use crate::i18n;

/// Lowest table that can be practised.
pub const TABLE_MIN: u32 = 1;
/// Highest table that can be practised.
pub const TABLE_MAX: u32 = 10;
/// Highest multiplier in every table: table *t* asks *t* × 1 up to *t* × 10.
pub const MULTIPLIER_MAX: u32 = 10;
/// Number of selectable tables, `TABLE_MAX - TABLE_MIN + 1`.
pub const TABLE_COUNT: usize = (TABLE_MAX - TABLE_MIN + 1) as usize;

/// One multiplication fact, asked exactly once per session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Question {
    /// Left factor: the table number when `reverse` is off.
    pub a: u32,
    /// Right factor.
    pub b: u32,
}

impl Question {
    pub fn new(a: u32, b: u32) -> Self {
        Self { a, b }
    }

    /// The product the learner has to type.
    pub fn expected(&self) -> u32 {
        self.a * self.b
    }

    /// `7 × 8`, as it is shown on screen and in the results list.
    ///
    /// The spaces around the sign are part of the label: they are what the
    /// [`LONGEST_QUESTION`] budget and the big font are measured against.
    pub fn label(&self) -> String {
        format!("{} {} {}", self.a, i18n::tafels::TIMES, self.b)
    }

    /// `7 × 8 = 56`.
    pub fn equation(&self) -> String {
        format!("{} = {}", self.label(), self.expected())
    }
}

/// Builds the questions for a set of tables.
///
/// With `reverse` off, every selected table *t* contributes *t* × 1 up to
/// *t* × [`MULTIPLIER_MAX`], table number first. With it on, the reversed form
/// *n* × *t* is added too, and identical pairs are dropped, so a fact never
/// appears twice.
///
/// The result is deterministic (tables ascending, multipliers ascending), which
/// is what lets the session shuffle it reproducibly.
pub fn generate(tables: &[u32], reverse: bool) -> Vec<Question> {
    let mut seen: HashSet<(u32, u32)> = HashSet::new();
    let mut questions = Vec::new();

    let mut tables: Vec<u32> = tables
        .iter()
        .copied()
        .filter(|table| (TABLE_MIN..=TABLE_MAX).contains(table))
        .collect();
    tables.sort_unstable();
    tables.dedup();

    let push = |a: u32, b: u32, questions: &mut Vec<Question>, seen: &mut HashSet<(u32, u32)>| {
        if seen.insert((a, b)) {
            questions.push(Question::new(a, b));
        }
    };

    for table in tables {
        for multiplier in 1..=MULTIPLIER_MAX {
            push(table, multiplier, &mut questions, &mut seen);
            if reverse {
                push(multiplier, table, &mut questions, &mut seen);
            }
        }
    }
    questions
}

/// Longest question the module can ever show, cursor included.
///
/// Used to pick the font size once per session instead of per question, so the
/// digits do not change size halfway through a round.
pub const LONGEST_QUESTION: &str = "10 × 10 = 100_";

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// How often each pair occurs; every value must be 1.
    fn counts(questions: &[Question]) -> HashMap<(u32, u32), usize> {
        let mut map = HashMap::new();
        for question in questions {
            *map.entry((question.a, question.b)).or_insert(0) += 1;
        }
        map
    }

    #[test]
    fn one_table_asks_ten_questions_table_number_first() {
        let questions = generate(&[7], false);
        assert_eq!(questions.len(), 10);
        let expected: Vec<Question> = (1..=10).map(|n| Question::new(7, n)).collect();
        assert_eq!(questions, expected);
        assert_eq!(questions[5].expected(), 42);
    }

    #[test]
    fn several_tables_are_stacked_in_ascending_order() {
        let questions = generate(&[7, 3], false);
        assert_eq!(questions.len(), 20);
        // Sorted, so table 3 comes first whatever order the checkboxes were in.
        assert_eq!(questions[0], Question::new(3, 1));
        assert_eq!(questions[10], Question::new(7, 1));
        assert_eq!(questions[19], Question::new(7, 10));
    }

    #[test]
    fn reverse_adds_the_other_order_but_never_a_duplicate() {
        let questions = generate(&[7], true);
        let counts = counts(&questions);

        // 7 × n and n × 7 for n = 1..=10, with 7 × 7 shared by both.
        assert_eq!(questions.len(), 19);
        assert!(counts.values().all(|count| *count == 1));
        assert!(counts.contains_key(&(7, 3)));
        assert!(counts.contains_key(&(3, 7)));
        assert_eq!(counts.get(&(7, 7)), Some(&1));
    }

    #[test]
    fn reverse_on_two_tables_deduplicates_both_ways() {
        let questions = generate(&[3, 7], true);
        let counts = counts(&questions);

        assert!(counts.values().all(|count| *count == 1));
        // 19 per table, minus the two facts that belong to both: 3 × 7 and
        // 7 × 3 are asked by table 3 as 7 × 3 and by table 7 as 3 × 7.
        assert_eq!(questions.len(), 36);
        for &(a, b) in &[(3, 7), (7, 3), (3, 3), (7, 7), (10, 7), (7, 10)] {
            assert_eq!(counts.get(&(a, b)), Some(&1), "{a} × {b} hoort erbij");
        }
    }

    #[test]
    fn no_tables_gives_no_questions() {
        assert!(generate(&[], false).is_empty());
        assert!(generate(&[], true).is_empty());
    }

    #[test]
    fn tables_outside_the_range_are_ignored() {
        // The range is one constant pair, so a stored file from a future build
        // cannot smuggle in a table of 99.
        assert_eq!(generate(&[0, TABLE_MAX + 1], false).len(), 0);
        assert_eq!(generate(&[0, 3, TABLE_MAX + 1], false).len(), 10);
        assert_eq!(generate(&[TABLE_MIN - 1, 0], true).len(), 0);
    }

    #[test]
    fn a_repeated_table_is_asked_once() {
        assert_eq!(generate(&[5, 5, 5], false).len(), 10);
        assert_eq!(generate(&[5, 5], true).len(), 19);
    }

    #[test]
    fn the_generation_order_does_not_depend_on_the_input_order() {
        assert_eq!(generate(&[3, 5, 8], true), generate(&[8, 3, 5], true));
    }

    #[test]
    fn every_product_fits_and_every_question_is_covered_by_the_range() {
        for table in TABLE_MIN..=TABLE_MAX {
            for multiplier in 1..=MULTIPLIER_MAX {
                let question = Question::new(table, multiplier);
                assert_eq!(question.expected(), table * multiplier);
                assert!(question.expected() <= 1_000);
            }
        }
    }

    #[test]
    fn a_question_is_shown_as_an_equation() {
        assert_eq!(Question::new(7, 8).label(), "7 × 8");
        assert_eq!(Question::new(7, 8).equation(), "7 × 8 = 56");
    }

    #[test]
    fn the_longest_question_covers_every_question_that_can_be_asked() {
        let questions = generate(&(TABLE_MIN..=TABLE_MAX).collect::<Vec<_>>(), true);
        for question in questions {
            // The answer plus the cursor is what actually reaches the screen.
            let shown = format!("{}{}", question.equation(), '_');
            assert!(
                shown.chars().count() <= LONGEST_QUESTION.chars().count(),
                "{shown} is langer dan {LONGEST_QUESTION}"
            );
        }
    }
}
