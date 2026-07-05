use super::DesktopApp;

pub(super) fn search_apps(apps: &[DesktopApp], query: &str, limit: usize) -> Vec<DesktopApp> {
    let query_tokens = tokenize_search(query);
    if query_tokens.is_empty() {
        return apps.iter().take(limit).cloned().collect();
    }

    let mut scored = apps
        .iter()
        .filter_map(|app| search_score(app, &query_tokens).map(|score| (score, app)))
        .collect::<Vec<_>>();
    scored.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .cmp(left_score)
            .then(left.name.cmp(&right.name))
            .then(left.id.cmp(&right.id))
    });

    if scored.is_empty() {
        return apps.iter().take(limit).cloned().collect();
    }

    scored
        .into_iter()
        .take(limit)
        .map(|(_, app)| app.clone())
        .collect()
}

fn tokenize_search(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|token| !token.is_empty())
        .collect()
}

fn search_score(app: &DesktopApp, query_tokens: &[String]) -> Option<u32> {
    let mut total = 0;
    for token in query_tokens {
        let score = token_score(app, token)?;
        total += score;
    }
    Some(total)
}

pub(super) fn token_score(app: &DesktopApp, token: &str) -> Option<u32> {
    let name = app.name.to_lowercase();
    if name == token {
        return Some(1000);
    }
    if name.starts_with(token) {
        return Some(900);
    }
    if name.split_whitespace().any(|word| word.starts_with(token)) {
        return Some(800);
    }
    if app
        .keywords
        .iter()
        .any(|keyword| keyword.to_lowercase().contains(token))
    {
        return Some(700);
    }
    if name.contains(token) {
        return Some(600);
    }
    if app
        .generic_name
        .as_ref()
        .is_some_and(|value| value.to_lowercase().contains(token))
        || app
            .comment
            .as_ref()
            .is_some_and(|value| value.to_lowercase().contains(token))
    {
        return Some(500);
    }
    if app.id.to_lowercase().contains(token)
        || app.exec.to_lowercase().contains(token)
        || app
            .categories
            .iter()
            .any(|category| category.to_lowercase().contains(token))
    {
        return Some(400);
    }
    None
}
