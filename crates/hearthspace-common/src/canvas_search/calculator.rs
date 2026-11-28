//! Pure, bounded arithmetic for a query-derived canvas-search result.
use std::time::{Duration, Instant};

struct Deadline(Instant);
impl fend_core::Interrupt for Deadline {
    fn should_interrupt(&self) -> bool {
        Instant::now() >= self.0
    }
}

pub const RESULT_ID: &str = "calculation:answer";
const MAX_EXPRESSION_BYTES: usize = 512;
const MAX_NESTING: usize = 32;

fn normalize(query: &str) -> String {
    let mut expression = String::new();
    let mut characters = query.chars().peekable();
    let mut previous_digit = false;
    while let Some(character) = characters.next() {
        if character == '.' && !previous_digit {
            expression.push('0');
        }
        expression.push(match character {
            '×' => '*',
            '÷' => '/',
            '−' => '-',
            other => other,
        });
        if character == '.' && !characters.peek().is_some_and(char::is_ascii_digit) {
            expression.push('0');
        }
        previous_digit = character.is_ascii_digit();
    }
    expression
}

/// Incomplete expressions and ordinary names contribute no calculator result.
pub fn answer(query: &str) -> Option<String> {
    let query = query.trim();
    if query.len() > MAX_EXPRESSION_BYTES || !query.chars().any(|c| "+-*/^%()×÷−".contains(c)) {
        return None;
    }
    if !query
        .chars()
        .all(|c| c.is_alphanumeric() || c.is_whitespace() || ".+-*/^%(),×÷−".contains(c))
    {
        return None;
    }
    let mut depth = 0usize;
    for character in query.chars() {
        match character {
            '(' => {
                depth += 1;
                if depth > MAX_NESTING {
                    return None;
                }
            }
            ')' => depth = depth.checked_sub(1)?,
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    let expression = normalize(query);
    let context = fend_core::Context::new();
    let deadline = Deadline(Instant::now() + Duration::from_millis(50));
    let decimal = format!("({expression}) to 15 sf");
    let result = fend_core::evaluate_preview_with_interrupt(&decimal, &context, &deadline);
    let text = result
        .get_main_result()
        .strip_prefix("approx. ")
        .unwrap_or(result.get_main_result());
    // Accept scalar answers only; units, text, functions and complex values stay out of search.
    let value: f64 = text.parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    // Present 15 significant digits, avoiding binary artifacts such as 0.1 + 0.2.
    // The exact displayed string is also the clipboard payload.
    let rounded: f64 = format!("{value:.14e}").parse().ok()?;
    if !rounded.is_finite() {
        return None;
    }
    if rounded == 0.0 {
        return Some("0".into());
    }
    if rounded.abs() >= 1e15 || rounded.abs() < 1e-6 {
        Some(format!("{rounded:e}"))
    } else {
        Some(rounded.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_precedence_functions_and_decimal_copy_text() {
        for (expression, expected) in [
            ("2 + 3 * 4", "14"),
            ("(2 + 3) * 4", "20"),
            ("7 / 2", "3.5"),
            ("2^10", "1024"),
            ("2^3^2", "512"),
            ("-2^2", "-4"),
            ("8/2*2", "8"),
            ("10-3+2", "9"),
            ("sqrt(81)", "9"),
            ("0.1 + 0.2", "0.3"),
            ("1/3", "0.333333333333333"),
            ("6 × 7", "42"),
            ("9 ÷ 2 − 1", "3.5"),
            ("-0 + 0", "0"),
            ("1e-12 * 2", "2e-12"),
            (".5 + 2.", "2.5"),
            ("10^20", "1e20"),
        ] {
            assert_eq!(
                answer(expression).as_deref(),
                Some(expected),
                "{expression}"
            );
        }
    }

    #[test]
    fn unfinished_undefined_and_non_math_queries_have_no_answer() {
        for expression in [
            "",
            "Terminal",
            "123",
            "notes-2026",
            "2+",
            "(2*3",
            "1/0",
            "sqrt(-1)",
            "10^999",
            "x+2",
        ] {
            assert_eq!(answer(expression), None, "{expression}");
        }
        assert_eq!(answer(&"1+".repeat(MAX_EXPRESSION_BYTES)), None);
        let nested = format!(
            "{}1{}",
            "(".repeat(MAX_NESTING + 1),
            ")".repeat(MAX_NESTING + 1)
        );
        assert_eq!(answer(&nested), None);
    }
}
