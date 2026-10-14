use waydriver::{Error, Result};

pub(crate) fn validate_resolution(resolution: &str) -> Result<()> {
    let Some((width, height)) = resolution
        .split_once('x')
        .or_else(|| resolution.split_once('X'))
    else {
        return Err(Error::process(format!(
            "invalid resolution {resolution:?}; expected WIDTHxHEIGHT"
        )));
    };
    for (label, value) in [("width", width), ("height", height)] {
        let value = value
            .parse::<i32>()
            .map_err(|e| Error::process_with(format!("parse {label}"), e))?;
        if value <= 0 {
            return Err(Error::process(format!("{label} must be positive")));
        }
    }
    Ok(())
}

pub(crate) fn scale_to_integer(scale: Option<f64>) -> Result<Option<i32>> {
    let Some(scale) = scale else {
        return Ok(None);
    };
    if !scale.is_finite() || scale <= 0.0 {
        return Err(Error::process("scale must be a positive finite number"));
    }
    let rounded = scale.round();
    if (scale - rounded).abs() > f64::EPSILON {
        return Err(Error::process(format!(
            "Hearthspace headless scale must be an integer, got {scale}"
        )));
    }
    let scale =
        i32::try_from(rounded as i64).map_err(|e| Error::process_with("convert scale", e))?;
    Ok((scale != 1).then_some(scale))
}
