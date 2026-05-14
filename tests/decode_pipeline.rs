use std::time::{Duration, Instant};

use cbr_egui::decode::{
    CancellationToken, DecodeError, DecodePurpose, DecodeRequest, DecodeRequestId, WorkerPool,
    decode_page,
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

fn request(
    request_id: u64,
    page_index: usize,
    bytes: Vec<u8>,
    target_size: Option<[u32; 2]>,
) -> DecodeRequest {
    DecodeRequest {
        request_id: DecodeRequestId(request_id),
        page_index,
        bytes,
        purpose: DecodePurpose::Direct,
        target_size,
        cancellation_token: None,
    }
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
