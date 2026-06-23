use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
    process::Command,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug)]
pub struct OcrImage {
    pub width: i32,
    pub height: i32,
    pub rgba_bottom_up: Vec<u8>,
}

pub fn run_tesseract_for_window(window_id: u64, title: Option<String>, image: OcrImage) {
    thread::spawn(move || {
        println!("=== Hearthspace OCR window {window_id} ===");
        if let Some(title) = title.as_deref() {
            println!("title: {title}");
        }

        match run_tesseract(window_id, &image) {
            Ok(text) => {
                let text = text.trim();
                if text.is_empty() {
                    println!("No OCR text detected.");
                } else {
                    println!("{text}");
                }
            }
            Err(error) => eprintln!("OCR failed for window {window_id}: {error}"),
        }
    });
}

fn run_tesseract(window_id: u64, image: &OcrImage) -> Result<String, Box<dyn std::error::Error>> {
    let image_path = temporary_image_path(window_id);
    write_ppm(&image_path, image)?;

    let output = Command::new("tesseract")
        .arg(&image_path)
        .arg("stdout")
        .arg("--psm")
        .arg("6")
        .output();

    let remove_result = fs::remove_file(&image_path);
    if let Err(error) = remove_result {
        eprintln!(
            "Failed to remove temporary OCR image {}: {error}",
            image_path.display()
        );
    }

    let output = output?;
    if !output.status.success() {
        return Err(format!(
            "tesseract exited with {}; stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn write_ppm(path: &PathBuf, image: &OcrImage) -> Result<(), Box<dyn std::error::Error>> {
    if image.width <= 0 || image.height <= 0 {
        return Err("OCR image has no pixels".into());
    }

    let width = image.width as usize;
    let height = image.height as usize;
    let expected_len = width * height * 4;
    if image.rgba_bottom_up.len() != expected_len {
        return Err(format!(
            "OCR image has {} bytes, expected {expected_len}",
            image.rgba_bottom_up.len()
        )
        .into());
    }

    let mut file = File::create(path)?;
    write!(file, "P6\n{} {}\n255\n", image.width, image.height)?;

    for row in (0..height).rev() {
        let row_start = row * width * 4;
        for pixel in image.rgba_bottom_up[row_start..row_start + width * 4].chunks_exact(4) {
            file.write_all(&pixel[0..3])?;
        }
    }

    Ok(())
}

fn temporary_image_path(window_id: u64) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!(
        "hearthspace-ocr-{}-{window_id}-{timestamp}.ppm",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_ppm_outputs_top_to_bottom_rgb_rows() {
        let path = temporary_image_path(1);
        let image = OcrImage {
            width: 2,
            height: 2,
            rgba_bottom_up: vec![1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255],
        };

        write_ppm(&path, &image).unwrap();
        let bytes = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();

        let header = b"P6\n2 2\n255\n";
        assert_eq!(&bytes[..header.len()], header);
        assert_eq!(
            &bytes[header.len()..],
            &[7, 8, 9, 10, 11, 12, 1, 2, 3, 4, 5, 6]
        );
    }
}
