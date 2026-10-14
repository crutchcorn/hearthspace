use std::time::Instant;

pub(crate) fn check_startup_deadline(
    deadline: Instant,
    shutdown_requested: bool,
    child: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if shutdown_requested {
        return Err(format!("shutdown requested while waiting for {child}").into());
    }
    if Instant::now() >= deadline {
        return Err(format!("absolute startup deadline exceeded waiting for {child}").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn deadline_is_absolute_even_if_child_keeps_logging() {
        let past = Instant::now() - Duration::from_secs(1);
        assert!(check_startup_deadline(past, false, "noisy child").is_err());
    }

    #[test]
    fn cancellation_interrupts_startup() {
        let future = Instant::now() + Duration::from_secs(60);
        assert!(check_startup_deadline(future, true, "child").is_err());
        assert!(check_startup_deadline(future, false, "child").is_ok());
    }
}
