use super::DesktopApp;

pub(super) fn launch_argv(app: &DesktopApp) -> Result<Vec<String>, String> {
    let argv = parse_exec_argv(app)?;
    if argv.is_empty() {
        Err(format!("desktop entry {} produced no command", app.id))
    } else {
        Ok(argv)
    }
}

fn parse_exec_argv(app: &DesktopApp) -> Result<Vec<String>, String> {
    let args = split_exec(&app.exec)?;
    let mut expanded = Vec::new();
    for arg in args {
        if let Some(arg) = expand_exec_field_codes(app, &arg)? {
            expanded.push(arg);
        }
    }
    Ok(expanded)
}

pub(super) fn split_exec(exec: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = exec.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '\\' => match chars.next() {
                Some(next) => current.push(next),
                None => current.push('\\'),
            },
            ch if ch.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if in_quotes {
        return Err(format!("unterminated quote in Exec={exec}"));
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

fn expand_exec_field_codes(app: &DesktopApp, arg: &str) -> Result<Option<String>, String> {
    let mut output = String::new();
    let mut chars = arg.chars();

    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }

        let Some(code) = chars.next() else {
            return Err(format!("dangling field code in {}", app.id));
        };
        match code {
            'f' | 'F' | 'u' | 'U' | 'i' => return Ok(None),
            'c' => output.push_str(&app.name),
            'k' => output.push_str(&app.path.to_string_lossy()),
            '%' => output.push('%'),
            other => return Err(format!("unsupported field code %{other} in {}", app.id)),
        }
    }

    Ok((!output.is_empty()).then_some(output))
}
