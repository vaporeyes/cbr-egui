use std::time::{Duration, Instant};

use cbr_egui::decode::{
    CancellationToken, DecodeError, DecodePurpose, DecodeRequest, DecodeRequestId, DecodeSource,
    ImageAdjustments, Rotation, WorkerPool, decode_page,
};
use image::{ImageBuffer, ImageFormat, Rgba};

#[test]
fn decodes_valid_image_bytes_to_color_image() {
    let result = decode_page(request(1, 7, png_bytes(2, 3), None));
    let image = result.outcome.expect("decode succeeds");

    assert_eq!(result.request_id, DecodeRequestId(1));
    assert_eq!(result.page_index, 7);
    assert_eq!(image.size, [2, 3]);
}

#[test]
fn corrupt_image_bytes_return_recoverable_error() {
    let result = decode_page(request(2, 0, b"not an image".to_vec(), None));

    assert!(matches!(result.outcome, Err(DecodeError::Image(_))));
}

#[test]
fn target_size_downsamples_before_color_image_conversion() {
    let result = decode_page(request(3, 0, png_bytes(40, 20), Some([10, 10])));
    let image = result.outcome.expect("decode succeeds");

    assert_eq!(image.size, [10, 5]);
}

#[test]
fn worker_pool_processes_many_requests_without_blocking_submission() {
    let pool = WorkerPool::start(4, 64).expect("worker pool");
    let start = Instant::now();

    for id in 0..25 {
        pool.submit(request(id, id as usize, png_bytes(1, 1), None))
            .expect("submit request");
    }

    assert!(start.elapsed() < Duration::from_millis(100));

    let mut received = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while received.len() < 25 && Instant::now() < deadline {
        if let Some(result) = pool.try_recv() {
            received.push(result);
        }
    }

    assert_eq!(received.len(), 25);
    assert!(received.into_iter().all(|result| result.outcome.is_ok()));
    pool.shutdown().expect("shutdown");
}

#[test]
fn worker_result_preserves_request_identity() {
    let pool = WorkerPool::start(1, 4).expect("worker pool");
    pool.submit(request(42, 9, png_bytes(1, 1), None))
        .expect("submit request");

    let result = wait_for_result(&pool);

    assert_eq!(result.request_id, DecodeRequestId(42));
    assert_eq!(result.page_index, 9);
    pool.shutdown().expect("shutdown");
}

#[test]
fn cancelled_decode_request_returns_recoverable_error() {
    let token = CancellationToken::new();
    token.cancel();
    let mut request = request(77, 4, png_bytes(1, 1), None);
    request.cancellation_token = Some(token);

    let result = decode_page(request);

    assert_eq!(result.request_id, DecodeRequestId(77));
    assert!(
        matches!(result.outcome, Err(DecodeError::Image(message)) if message.contains("cancelled"))
    );
}

#[test]
fn quarter_turn_rotation_swaps_decoded_dimensions() {
    let mut request = request(10, 0, png_bytes(40, 20), None);
    request.rotation = Rotation::Cw90;

    let image = decode_page(request).outcome.expect("decode succeeds");

    assert_eq!(image.size, [20, 40]);
}

#[test]
fn half_turn_rotation_preserves_decoded_dimensions() {
    let mut request = request(11, 0, png_bytes(40, 20), None);
    request.rotation = Rotation::Cw180;

    let image = decode_page(request).outcome.expect("decode succeeds");

    assert_eq!(image.size, [40, 20]);
}

#[test]
fn rotation_cycles_right_and_left_through_quarter_turns() {
    assert_eq!(Rotation::None.rotate_right(), Rotation::Cw90);
    assert_eq!(Rotation::Cw270.rotate_right(), Rotation::None);
    assert_eq!(Rotation::None.rotate_left(), Rotation::Cw270);
    assert_eq!(Rotation::Cw90.rotate_left(), Rotation::None);
    assert!(Rotation::Cw90.swaps_dimensions());
    assert!(Rotation::Cw270.swaps_dimensions());
    assert!(!Rotation::None.swaps_dimensions());
    assert!(!Rotation::Cw180.swaps_dimensions());
}

#[test]
fn image_adjustments_default_is_identity() {
    let adjustments = ImageAdjustments::default();

    assert!(adjustments.is_identity());
    for value in [0u8, 64, 128, 200, 255] {
        assert_eq!(adjustments.map_channel(value), value);
    }
}

#[test]
fn brightness_and_gamma_change_midtone_channel() {
    let brighter = ImageAdjustments {
        brightness: 0.5,
        ..Default::default()
    };
    assert!(!brighter.is_identity());
    assert!(brighter.map_channel(128) > 128);

    let gamma_up = ImageAdjustments {
        gamma: 2.0,
        ..Default::default()
    };
    assert!(gamma_up.map_channel(128) > 128);

    let gamma_down = ImageAdjustments {
        gamma: 0.5,
        ..Default::default()
    };
    assert!(gamma_down.map_channel(128) < 128);
}

#[test]
fn adjustments_apply_to_decoded_pixels() {
    let mut request = request(20, 0, gray_png_bytes(128), None);
    request.adjustments = ImageAdjustments {
        brightness: 0.4,
        ..Default::default()
    };

    let image = decode_page(request).outcome.expect("decode succeeds");

    let pixel = image.pixels[0];
    assert!(pixel.r() > 128);
    assert!(pixel.g() > 128);
    assert!(pixel.b() > 128);
}

fn request(
    request_id: u64,
    page_index: usize,
    bytes: Vec<u8>,
    target_size: Option<[u32; 2]>,
) -> DecodeRequest {
    DecodeRequest {
        request_id: DecodeRequestId(request_id),
        page_index,
        source: DecodeSource::Bytes(bytes),
        purpose: DecodePurpose::Direct,
        target_size,
        rotation: Rotation::None,
        adjustments: ImageAdjustments::default(),
        cancellation_token: None,
    }
}

fn gray_png_bytes(value: u8) -> Vec<u8> {
    let image = ImageBuffer::from_pixel(4, 4, Rgba([value, value, value, 255]));
    let mut cursor = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, ImageFormat::Png)
        .expect("encode png");
    cursor.into_inner()
}

fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let image = ImageBuffer::from_pixel(width, height, Rgba([255_u8, 0, 0, 255]));
    let mut cursor = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, ImageFormat::Png)
        .expect("encode png");
    cursor.into_inner()
}

fn wait_for_result(pool: &WorkerPool) -> cbr_egui::decode::DecodeResult {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(result) = pool.try_recv() {
            return result;
        }
    }
    panic!("timed out waiting for decode result");
}
