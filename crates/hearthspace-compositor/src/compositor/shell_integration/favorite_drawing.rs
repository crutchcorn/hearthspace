//! Private shell drawing commands; normal application launching stays unchanged.
use super::ControlReply;
use crate::compositor::CalloopData;

enum Command {
    Arm {
        generation: u64,
        bounds: [f64; 4],
    },
    Disarm(u64),
    Accept {
        generation: u64,
        stroke: u64,
    },
    Launch {
        generation: u64,
        stroke: u64,
        app_id: String,
    },
}

fn parse(text: &str) -> Option<Command> {
    let mut parts = text.split_whitespace();
    let name = parts.next()?;
    if name == "favorite-gesture-launch" {
        return parse_launch(text);
    }
    let generation = parts
        .next()?
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)?;
    let command = match name {
        "favorite-gesture-arm" => {
            let mut bounds = [0.0; 4];
            for value in &mut bounds {
                *value = parts
                    .next()?
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite())?;
            }
            Command::Arm { generation, bounds }
        }
        "favorite-gesture-disarm" => Command::Disarm(generation),
        "favorite-gesture-accept" => Command::Accept {
            generation,
            stroke: parts
                .next()?
                .parse::<u64>()
                .ok()
                .filter(|value| *value > 0)?,
        },
        _ => return None,
    };
    parts.next().is_none().then_some(command)
}

fn parse_launch(text: &str) -> Option<Command> {
    fn word(text: &str) -> Option<(&str, &str)> {
        let text = text.trim_start();
        let boundary = text.find(char::is_whitespace)?;
        Some((&text[..boundary], text[boundary..].trim_start()))
    }
    let (_, text) = word(text)?;
    let (generation, text) = word(text)?;
    let (stroke, encoded) = word(text)?;
    let generation = generation.parse::<u64>().ok().filter(|n| *n > 0)?;
    let stroke = stroke.parse::<u64>().ok().filter(|n| *n > 0)?;
    let app_id: String = serde_json::from_str(encoded).ok()?;
    if app_id.is_empty() || app_id.len() > 512 || app_id.chars().any(char::is_control) {
        return None;
    }
    Some(Command::Launch {
        generation,
        stroke,
        app_id,
    })
}

impl CalloopData {
    pub(super) fn run_favorite_drawing_command(&mut self, text: &str) -> Option<ControlReply> {
        if !text.starts_with("favorite-gesture-") {
            return None;
        }
        let Some(command) = parse(text) else {
            return Some(ControlReply::Err("invalid drawing command".into()));
        };
        if !self.running || !self.capture_active() || self.state.session_locked() {
            self.state.clear_favorite_drawing();
            return Some(ControlReply::Err("session is inactive or locked".into()));
        }
        let result = match command {
            Command::Arm { generation, bounds } => {
                self.state.arm_favorite_drawing(generation, bounds)
            }
            Command::Disarm(generation) => self.state.disarm_favorite_drawing(generation),
            Command::Accept { generation, stroke } => {
                self.state.accept_favorite_drawing(generation, stroke)
            }
            Command::Launch {
                generation,
                stroke,
                app_id,
            } => {
                return Some(
                    self.state
                        .launch_favorite_drawing(generation, stroke, &app_id),
                );
            }
        };
        Some(result.map_or_else(ControlReply::Err, |_| ControlReply::Ok))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_bounded_finite_exact_commands_parse() {
        assert!(matches!(
            parse("favorite-gesture-arm 9 4.5 8 320 180"),
            Some(Command::Arm { generation: 9, .. })
        ));
        assert!(matches!(
            parse("favorite-gesture-accept 9 2"),
            Some(Command::Accept {
                generation: 9,
                stroke: 2
            })
        ));
        assert!(matches!(
            parse("favorite-gesture-disarm 9"),
            Some(Command::Disarm(9))
        ));
        for text in [
            "favorite-gesture-arm 0 0 0 100 100",
            "favorite-gesture-arm 1 NaN 0 100 100",
            "favorite-gesture-arm 1 0 inf 100 100",
            "favorite-gesture-arm 1 0 0 100",
            "favorite-gesture-arm 1 0 0 100 100 extra",
            "favorite-gesture-accept 1 0",
            "favorite-gesture-accept 1 2 extra",
            "favorite-gesture-disarm 1 extra",
            "favorite-gesture-disarm -1",
            "favorite-gesture-unknown 1",
        ] {
            assert!(parse(text).is_none(), "{text}");
        }
    }

    #[test]
    fn launch_preserves_json_app_ids_and_rejects_malformed_or_extra_input() {
        let id = "org.example.Two  spaces-\"quoted\".desktop";
        let text = format!(
            "favorite-gesture-launch 9 2 {}",
            serde_json::to_string(id).unwrap()
        );
        assert!(
            matches!(parse(&text), Some(Command::Launch { generation: 9, stroke: 2, app_id }) if app_id == id)
        );
        for text in [
            "favorite-gesture-launch 9 2 naked.desktop",
            "favorite-gesture-launch 9 2 \"ok.desktop\" extra",
            "favorite-gesture-launch 9 2 \"\"",
            "favorite-gesture-launch 0 2 \"ok.desktop\"",
            "favorite-gesture-launch 9 0 \"ok.desktop\"",
            "favorite-gesture-launch 9 2 \"a\\nb.desktop\"",
        ] {
            assert!(parse(text).is_none(), "{text}");
        }
        let oversized = format!("favorite-gesture-launch 9 2 \"{}\"", "a".repeat(513));
        assert!(parse(&oversized).is_none());
    }
}
