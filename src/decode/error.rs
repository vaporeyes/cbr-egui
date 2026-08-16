use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DecodeError {
    #[error("decode request contained no image bytes")]
    EmptyBytes,
    #[error("image decode failed: {0}")]
    Image(String),
    #[error("decode request cancelled")]
    Cancelled,
    #[error("decoded image dimensions are too large: {width}x{height}")]
    ImageTooLarge { width: u32, height: u32 },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WorkerError {
    #[error("worker count must be at least one")]
    InvalidWorkerCount,
    #[error("queue bound must be at least one")]
    InvalidQueueBound,
    #[error("decode worker pool is shut down")]
    ShutDown,
    #[error("decode request queue is full")]
    QueueFull,
    #[error("worker thread panicked during shutdown")]
    WorkerPanicked,
}
