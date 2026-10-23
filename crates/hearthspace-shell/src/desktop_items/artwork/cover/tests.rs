use super::*;

#[test]
fn malformed_images_and_excessive_dimensions_are_rejected() {
    assert!(image_bytes(b"not an image").is_none());
    let mut png = Cursor::new(Vec::new());
    image::RgbaImage::new(MAX_DIMENSION + 1, 1)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    assert!(image_bytes(png.get_ref()).is_none());
}

#[test]
fn metadata_reader_bounds_seeking_repeated_reads_and_parser_iterations() {
    let mut reader = LimitedReader::new(Cursor::new(vec![0; 32]), u64::MAX);
    assert!(
        reader
            .seek(SeekFrom::Start(MAX_METADATA_BYTES + 1))
            .is_err()
    );
    assert!(reader.seek(SeekFrom::End(0)).is_err());
    reader.remaining = 2;
    let mut bytes = [0; 4];
    assert_eq!(reader.read(&mut bytes).unwrap(), 2);
    reader.rewind().unwrap();
    assert!(reader.read(&mut bytes).is_err());
    reader.operations = 0;
    assert!(reader.rewind().is_err());
}

#[test]
fn replaced_special_files_and_corrupt_flac_metadata_cannot_become_artwork() {
    let directory = tempfile::tempdir().unwrap();
    assert!(flac(directory.path()).is_none());
    let path = directory.path().join("corrupt.flac");
    std::fs::write(&path, b"fLaC\0\xff\xff\xff").unwrap();
    assert!(flac(&path).is_none());
    assert!(image_file(&path).is_none());
}
