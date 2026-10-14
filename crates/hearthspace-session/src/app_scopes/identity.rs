pub const CLOSING_FILE: &str = "app-scopes-closing";
pub const READY_FILE: &str = "app-scope-guard-ready";

pub fn process_start_time(stat: &str) -> Option<&str> {
    // comm may contain spaces or parentheses. Field 22 follows the final ')'.
    stat.rsplit_once(')')?.1.split_whitespace().nth(19)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_identity_survives_spaces_and_parentheses_in_comm() {
        let suffix = (3..=30)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            process_start_time(&format!("42 (weird ) name) {suffix}")),
            Some("22")
        );
        assert_eq!(process_start_time("invalid"), None);
    }
}
