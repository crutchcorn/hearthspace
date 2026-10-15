use waydriver::{Error, Result};

pub(crate) fn keysym_to_evdev(keysym: u32) -> Result<u32> {
    if let Ok(byte) = u8::try_from(keysym)
        && let Some(keycode) = ascii_keysym_to_evdev(byte)
    {
        return Ok(keycode);
    }

    let keycode = match keysym {
        0xff08 => 14,  // BackSpace
        0xff09 => 15,  // Tab
        0xff0d => 28,  // Return
        0xff1b => 1,   // Escape
        0xffe1 => 42,  // Shift_L
        0xffe2 => 54,  // Shift_R
        0xffe3 => 29,  // Control_L
        0xffe4 => 97,  // Control_R
        0xffe9 => 56,  // Alt_L
        0xffea => 100, // Alt_R
        _ => {
            return Err(Error::process(format!(
                "unsupported keysym 0x{keysym:x}; add a keysym-to-evdev mapping"
            )));
        }
    };
    Ok(keycode)
}

fn ascii_keysym_to_evdev(keysym: u8) -> Option<u32> {
    Some(match keysym.to_ascii_lowercase() {
        b'a' => 30,
        b'b' => 48,
        b'c' => 46,
        b'd' => 32,
        b'e' => 18,
        b'f' => 33,
        b'g' => 34,
        b'h' => 35,
        b'i' => 23,
        b'j' => 36,
        b'k' => 37,
        b'l' => 38,
        b'm' => 50,
        b'n' => 49,
        b'o' => 24,
        b'p' => 25,
        b'q' => 16,
        b'r' => 19,
        b's' => 31,
        b't' => 20,
        b'u' => 22,
        b'v' => 47,
        b'w' => 17,
        b'x' => 45,
        b'y' => 21,
        b'z' => 44,
        b'1' => 2,
        b'2' => 3,
        b'3' => 4,
        b'4' => 5,
        b'5' => 6,
        b'6' => 7,
        b'7' => 8,
        b'8' => 9,
        b'9' => 10,
        b'0' => 11,
        b' ' => 57,
        _ => return None,
    })
}
