use nokhwa::{nokhwa_initialize, pixel_format::RgbFormat, utils::{CameraIndex, RequestedFormat, RequestedFormatType}, Camera, NokhwaError};


fn main() -> Result<(), NokhwaError> {
    #[cfg(target_os = "macos")]
    nokhwa_initialize(| success | {
        println!("Init success: {}", success);
    });

    let request = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution);

    let mut camera = Camera::new(CameraIndex::Index(0), request)?;

    let frame = camera.frame()?;
    println!("Captured Single Frame of {}", frame.buffer().len());

    let decoded = frame.decode_image::<RgbFormat>()?;
    println!("Decoded Frame of {}", decoded.len());

    Ok(())
}
