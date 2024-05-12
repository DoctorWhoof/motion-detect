use std::time::{ Duration, Instant };
use eye::hal::{ PlatformContext, traits::{Context, Device, Stream} };
use image::{ imageops::FilterType, DynamicImage, GenericImageView, RgbImage };


fn main() -> Result<(), Box<dyn std::error::Error>> {

    // Settings
    let frame_capture_interval = Duration::from_secs_f32(0.25); 
    let motion_tail_length = Duration::from_secs(2);
    let camera_warm_up = Duration::from_secs(2);
    
    let capture_width = 640;
    let capture_height = 480;
    let downsample = 8;

    let pixel_threshold:f32 = 10.0;   // The percentage a pixel must change for it to count as an actual change.
    let image_threshold:f32 = 20.0;   // The percentage of pixels in an image needed to change to to trigger movement detection.

    // Create a context
    let ctx = PlatformContext::default();

    // Query for available devices.
    let devices = ctx.devices()?;

    // First, we need a capture device to read images from. For this example, let's just choose
    // whatever device is first in the list.
    let device = ctx.open_device(&devices[0].uri)?;

    // Query for available streams and just choose the first one.
    let streams = device.streams()?;
    let mut stream_desc = streams[0].clone();
    stream_desc.interval = frame_capture_interval;
    stream_desc.width = capture_width;
    stream_desc.height = capture_height;
    println!("Stream pixel bits: {:?}", stream_desc.pixfmt.bits());
    println!("Stream: {:?}", stream_desc);

    // Since we want to capture images, we need to access the native image stream of the device.
    // The backend will internally select a suitable implementation for the platform stream. On
    // Linux for example, most devices support memory-mapped buffers.
    let mut stream = device.start_stream(&stream_desc)?;
    
    // Capture single frame and resize to a thumbnail size
    let mut capture_thumbnail = || {
        let frame = stream
            .next()
            .expect("Stream is dead")               // Unwraps result
            .expect("Failed to capture frame");     // Unwraps option
    
        let Some(buffer) = 
            RgbImage::from_raw(stream_desc.width, stream_desc.height, frame.into())
        else {
            panic!("Image buffer creation failed")
        };
        
        DynamicImage::from(buffer)
            .brighten(25)
            .resize(
                stream_desc.width / downsample,
                stream_desc.height / downsample,
                FilterType::Triangle                // "Triangle" is a linear filter, fast and without sharpening artifacts
            )
    };

    // Bookkeeping
    let app_time = std::time::Instant::now();
    let mut last_frame_time = app_time;
    let mut prev_thumbnail:Option<DynamicImage> = None;
    let mut latest_movement_time:Option<Instant> = None;
    
    // Convert pixel_threshold from a percentage to an integer amount with a max value of 255
    let pixel_threshold = ((pixel_threshold * (255.0 / 100.0)) as i32).clamp(0, 255);
    
    // Normalize and clamp image threshold from its original percentage value
    let image_threshold = (image_threshold / 100.0f32).clamp(0.0, 1.0); 

    // Loop forever until interrupted.
    loop {
        // Start motion detection if first frame is initialized
        if let Some(ref mut prev_thumbnail) = prev_thumbnail {
            // Capture new thumbnail for current frame
            let thumbnail = capture_thumbnail();

            // Ensures processing will actually wait for the desired capture interval,
            // since a camewra may refuse to record at very low frame rates
            let processing_time = last_frame_time.elapsed();
            let wait_time = (frame_capture_interval - processing_time).as_secs_f32();
            if wait_time > 0.001 {
                std::thread::sleep(Duration::from_secs_f32(wait_time));
            } else {
                println!("Can't wait for negative time! Skipping");
            }

            // Motion detection
            let mut changed_pixels = 0;
            for y in 0 .. thumbnail.height() {
                for x in 0 .. thumbnail.width() {
                    let previous_pixel = prev_thumbnail.get_pixel(x, y);
                    let pixel = thumbnail.get_pixel(x, y);

                    let diff_r = (pixel[0] as i32 - previous_pixel[0] as i32).abs();
                    let diff_g = (pixel[1] as i32 - previous_pixel[1] as i32).abs();
                    let diff_b = (pixel[2] as i32 - previous_pixel[2] as i32).abs();

                    if diff_r >= pixel_threshold || diff_g >= pixel_threshold || diff_b >= pixel_threshold{
                        changed_pixels += 1;
                    }
                }   
            }

            let total_pixels = thumbnail.width() * thumbnail.height();
            let pixel_count_threshold = (total_pixels as f32 * image_threshold) as i32;

            // Outputs messages if motion events detected
            if changed_pixels > pixel_count_threshold {
                if latest_movement_time.is_none() {
                    println!("start");
                }
                *prev_thumbnail = thumbnail;
                latest_movement_time = Some(Instant::now());
            } else {
                // No movement in current frame, but there is an active movement started
                if let Some(time) = latest_movement_time {
                    if time.elapsed() > motion_tail_length {
                        println!("stop");
                        latest_movement_time = None;
                    }
                }
            }

        } else {

            // If not initialized, wait for camera warm up and initialize first thumbnail
            println!("'Warming up' camera...");
            std::thread::sleep(Duration::from_secs(1));
            if app_time.elapsed() > camera_warm_up {
                let thumbnail = capture_thumbnail();
                if thumbnail.save("test.png").is_err(){
                    println!("Error saving thumbnail image");
                } else {
                    println!("First thumbnail saved!");
                };
                prev_thumbnail = Some( thumbnail );
                println!("'Camera initialized. Waiting for motion...");
            }
        };

        last_frame_time = Instant::now();
    }
}


// println!(
//     "Captured! Total pixels:{}, changed pixels:{}, pixel threshold:{}, image threshold:{}",
//     total_pixels, changed_pixels, pixel_threshold, pixel_count_threshold
// );

// println!("Captured frame after {:.2} ms", last_frame_time.elapsed().as_secs_f32() * 1000.0);
