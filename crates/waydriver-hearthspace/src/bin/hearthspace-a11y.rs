//! Semantic agent commands on an explicitly selected private desktop bus.
use std::time::Duration;

use serde_json::{Value, json};
use waydriver_hearthspace::accessibility::{ElementInfo, SessionAccessibility};

struct Options {
    bus: String,
    timeout: Duration,
    action: String,
    selector: Option<String>,
}

impl Options {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut bus = None;
        let mut timeout = Duration::from_secs(10);
        let action = loop {
            let argument = args.next().ok_or("an action is required")?;
            match argument.as_str() {
                "--bus" => bus = Some(args.next().ok_or("--bus requires an address")?),
                "--timeout" => {
                    let seconds: f64 = args
                        .next()
                        .ok_or("--timeout requires seconds")?
                        .parse()
                        .map_err(|_| "invalid timeout")?;
                    if !seconds.is_finite() || !(0.0..=300.0).contains(&seconds) || seconds == 0.0 {
                        return Err("timeout must be greater than 0 and at most 300 seconds".into());
                    }
                    timeout = Duration::from_secs_f64(seconds);
                }
                "snapshot" | "find" | "activate" => break argument,
                _ => return Err(format!("unknown argument: {argument}")),
            }
        };
        let selector = if action == "snapshot" {
            None
        } else {
            Some(
                args.next()
                    .ok_or("find/activate requires an XPath selector")?,
            )
        };
        if args.next().is_some() {
            return Err("unexpected trailing arguments".into());
        }
        let bus = bus
            .filter(|bus| !bus.trim().is_empty())
            .ok_or("--bus requires an explicit private session bus address")?;
        Ok(Self {
            bus,
            timeout,
            action,
            selector,
        })
    }
}

fn main() {
    if std::env::args().any(|argument| matches!(argument.as_str(), "--help" | "-h")) {
        println!(
            "hearthspace-a11y --bus ADDRESS [--timeout SECONDS] snapshot|find XPATH|activate XPATH"
        );
        return;
    }
    match run() {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("{}", json!({"ok": false, "error": error}));
            std::process::exit(1);
        }
    }
}

fn run() -> Result<Value, String> {
    let options = Options::parse(std::env::args().skip(1))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(async {
        let api = SessionAccessibility::new(options.bus).map_err(|error| error.to_string())?;
        // Enable and action share one budget, including private bus activation.
        tokio::time::timeout(options.timeout, async {
            api.enable(options.timeout).await?;
            match options.action.as_str() {
                "snapshot" => Ok(json!({"ok": true, "snapshot": api.snapshot(options.timeout).await?})),
                "find" => {
                    let matches = api.find(options.selector.as_deref().unwrap(), options.timeout).await?;
                    Ok(json!({"ok": true, "elements": matches.into_iter().map(element_json).collect::<Vec<_>>()}))
                }
                "activate" => {
                    api.activate(options.selector.as_deref().unwrap(), options.timeout).await?;
                    Ok(json!({"ok": true, "accepted": true}))
                }
                _ => unreachable!(),
            }
        }).await.map_err(|_| "accessibility command timed out".to_string())?
            .map_err(|error: waydriver::Error| error.to_string())
    })
}

fn element_json(element: ElementInfo) -> Value {
    let bounds = element
        .bounds
        .map(|rect| json!({"x": rect.x, "y": rect.y, "width": rect.width, "height": rect.height}));
    json!({"bus_name": element.ref_.0, "object_path": element.ref_.1, "role": element.role,
        "role_raw": element.role_raw,
        "name": element.name, "description": element.description, "states": element.states,
        "attributes": element.attributes, "bounds": bounds})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(args.iter().map(|value| (*value).to_owned()))
    }

    #[test]
    fn requires_explicit_bus_and_exact_action_arguments() {
        assert!(parse(&["snapshot"]).is_err());
        assert!(parse(&["--bus", "", "snapshot"]).is_err());
        assert!(parse(&["--bus", "unix:path=/private", "activate"]).is_err());
        assert!(parse(&["--bus", "unix:path=/private", "snapshot", "extra"]).is_err());
        let options = parse(&[
            "--bus",
            "unix:path=/private",
            "find",
            "//PushButton[@name='Save']",
        ])
        .unwrap();
        assert_eq!(
            options.selector.as_deref(),
            Some("//PushButton[@name='Save']")
        );
    }

    #[test]
    fn deadlines_are_finite_positive_and_bounded() {
        for value in ["NaN", "inf", "-1", "0", "301"] {
            assert!(
                parse(&[
                    "--bus",
                    "unix:path=/private",
                    "--timeout",
                    value,
                    "snapshot"
                ])
                .is_err()
            );
        }
        assert_eq!(
            parse(&[
                "--bus",
                "unix:path=/private",
                "--timeout",
                "0.1",
                "snapshot"
            ])
            .unwrap()
            .timeout,
            Duration::from_millis(100)
        );
    }
}
