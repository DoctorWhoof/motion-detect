use eye::hal::{
    format::PixelFormat,
    stream::Descriptor,
    traits::{Context, Device, Stream},
    PlatformContext,
};
use std::{
    error::Error,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn Error>> {
    // Settings
    let camera_warm_up = Duration::from_secs(2);
    let motion_tail_length = Duration::from_secs(1);
    let frame_capture_interval = Duration::from_secs_f32(0.2);

    let capture_width = 640;
    let capture_height = 480;
    let downsample: usize = 8;

    let pixel_threshold = 10.0; // The percentage a pixel must change for it to count as an actual change.
    let image_threshold = 20.0; // The percentage of pixels in an image needed to change to to trigger movement detection.

    // Create a context
    let ctx = PlatformContext::default();

    // Query for available devices.
    let devices = ctx.devices()?;
    if devices.is_empty() {
        println!("\nError, no device detected.");
        std::process::exit(19); // No such device
    }
    println!("Available devices:");
    for dev in &devices {
        println!("    {:?}", dev);
    }

    // Query for available streams and choose the first index with available streams.
    let mut device_index = 0;
    for (n, device) in devices.iter().enumerate() {
        device_index = n;
        let candidate = ctx.open_device(&device.uri)?;
        if !candidate.streams()?.is_empty() {
            println!("Detected video stream on device {n}:");
            break;
        } else {
            println!("Device {n} has no video streams. Checking next one...");
        }
    }
    let device = ctx.open_device(&devices[device_index].uri)?;

    // // TODO: Only pick a stream if it satisfies the required video specs (resolution, frame rate)
    let streams = device.streams()?;
    let pixfmt = if streams.is_empty() {
        println!("    Warning, no video streams detected. Attempting default pixel format.");
        PixelFormat::Rgb(24)
    } else {
        streams[0].pixfmt.clone()
    };

    // Since we want to capture images, we need to access the native image stream of the device.
    // The backend will internally select a suitable implementation for the platform stream. On
    // Linux for example, most devices support memory-mapped buffers.
    let stream_desc = Descriptor {
        width: capture_width,
        height: capture_height,
        interval: frame_capture_interval,
        pixfmt,
    };
    let mut stream = device.start_stream(&stream_desc)?;

    // Convert pixel_threshold from a percentage to an integer amount with a max value of 255
    let pixel_threshold = ((pixel_threshold * (255.0 / 100.0)) as i32).clamp(0, 255);

    // Normalize and clamp image threshold from its original percentage value
    let image_threshold = (image_threshold / 100.0f32).clamp(0.0, 1.0);

    // Thumbnail management.
    let thumb_width = stream_desc.width as usize / downsample;
    let thumb_height = stream_desc.height as usize / downsample;
    let thumb_len = thumb_width * thumb_height;
    let sample_count = downsample * downsample;
    let pixel_count_threshold = (thumb_len as f32 * image_threshold) as i32;
    let mut previous_thumb = 0;
    let mut current_thumb = 1;
    let mut thumbs = [
        // Two thumbnail buffers, previous and current.
        vec![0; thumb_width * thumb_height * 3], // 3 bytes per pixel (RGB).
        vec![0; thumb_width * thumb_height * 3],
    ];

    // Function (OK, closure) to capture single frame and resize it to a thumbnail size,
    // stored in the "thumb" byte buffer passed as an argument.
    let mut update_thumbnail = |thumb: &mut Vec<u8>| {
        let frame = stream
            .next()
            .expect("Stream is dead") // Unwraps result.
            .expect("Failed to capture frame"); // Unwraps option.

        let mut source_x = 0;
        let mut source_y = 0;
        while source_y < stream_desc.height as usize {
            while source_x < stream_desc.width as usize {
                let mut resized_pixel: [u32; 3] = [0, 0, 0];
                for y in 0..downsample {
                    for x in 0..downsample {
                        let sub_pixel_index =
                            (((source_y + y) * stream_desc.width as usize) + (source_x + x)) * 3;
                        // Accumulate RGB values
                        resized_pixel[0] += frame[sub_pixel_index] as u32;
                        resized_pixel[1] += frame[sub_pixel_index + 1] as u32;
                        resized_pixel[2] += frame[sub_pixel_index + 2] as u32;
                    }
                }

                let dest_index =
                    (((source_y / downsample) * thumb_width) + (source_x / downsample)) * 3;

                // Averages RGB value and assigns it to thumbnail
                thumb[dest_index] = (resized_pixel[0] as usize / sample_count).clamp(0, 255) as u8;
                thumb[dest_index + 1] =
                    (resized_pixel[1] as usize / sample_count).clamp(0, 255) as u8;
                thumb[dest_index + 2] =
                    (resized_pixel[2] as usize / sample_count).clamp(0, 255) as u8;

                source_x += downsample; // We increment by as many pixels as "downsample" instead of just 1.
            }
            source_x = 0;
            source_y += downsample;
        }
    };

    // Time keeping
    let app_time = std::time::Instant::now();
    let mut last_frame_time = app_time;
    let mut latest_movement_time: Option<Instant> = None;

    // Wait for camera warm up (avoids black frames and false motion positives)
    println!("warming up");
    std::thread::sleep(camera_warm_up);

    // Init thumbnails
    update_thumbnail(&mut thumbs[previous_thumb]);
    update_thumbnail(&mut thumbs[current_thumb]);

    // // Debug save image. Optional! Comment out if image crate is not available.
    // let img = image::RgbImage::from_raw(thumb_width as u32, thumb_height as u32, thumbs[0].clone()).unwrap();
    // if img.save("test.png").is_err(){
    //     println!("Error saving thumbnail image");
    // } else {
    //     println!("First thumbnail saved!");
    // };

    // Loop forever until interrupted.
    println!("ready");
    loop {
        // Capture new thumbnail for current frame
        update_thumbnail(&mut thumbs[current_thumb]);

        // Ensures processing will actually wait for the desired capture interval,
        // since a camera may refuse to record at very low frame rates
        let processing_time = last_frame_time.elapsed();
        if processing_time < frame_capture_interval {
            let wait_time = frame_capture_interval - processing_time;
            if wait_time.as_millis() > 1 {
                std::thread::sleep(wait_time);
            }
        }

        // Pixel change detection
        let mut changed_pixels = 0;
        for y in 0..thumb_height {
            for x in 0..thumb_width {
                let index = ((y * thumb_width) + x) * 3;

                let previous_pixel = &thumbs[previous_thumb][index..=index + 2];
                let pixel = &&thumbs[current_thumb][index..=index + 2];

                let diff_r = (pixel[0] as i32 - previous_pixel[0] as i32).abs();
                let diff_g = (pixel[1] as i32 - previous_pixel[1] as i32).abs();
                let diff_b = (pixel[2] as i32 - previous_pixel[2] as i32).abs();

                if diff_r >= pixel_threshold
                    || diff_g >= pixel_threshold
                    || diff_b >= pixel_threshold
                {
                    changed_pixels += 1;
                }
            }
        }

        // Outputs messages if sufficient pixels have changed or stopped changing.
        if changed_pixels > pixel_count_threshold {
            if latest_movement_time.is_none() {
                println!("start");
            }
            latest_movement_time = Some(Instant::now());
            // "flip" buffers!
            previous_thumb = 1 - previous_thumb;
            current_thumb = 1 - current_thumb;
        } else {
            // No movement in current frame, but there is an active movement started.
            if let Some(time) = latest_movement_time {
                if time.elapsed() > motion_tail_length {
                    println!("stop");
                    latest_movement_time = None;
                }
            }
        }

        last_frame_time = Instant::now();
    }
}
