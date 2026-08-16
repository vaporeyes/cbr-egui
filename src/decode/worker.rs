use std::thread::{self, JoinHandle};

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded, unbounded};

use super::error::WorkerError;
use super::pipeline::{DecodeRequest, DecodeResult, decode_page};

pub struct WorkerPool {
    request_sender: Option<Sender<DecodeRequest>>,
    result_receiver: Receiver<DecodeResult>,
    handles: Vec<JoinHandle<()>>,
}

impl WorkerPool {
    pub fn start(worker_count: usize, queue_bound: usize) -> Result<Self, WorkerError> {
        if worker_count == 0 {
            return Err(WorkerError::InvalidWorkerCount);
        }
        if queue_bound == 0 {
            return Err(WorkerError::InvalidQueueBound);
        }

        let (request_sender, request_receiver) = bounded::<DecodeRequest>(queue_bound);
        // Results are unbounded so a worker can never block handing one back.
        // A bounded result channel deadlocks shutdown: nothing drains it while
        // the pool is being torn down, so a full channel would park a worker in
        // `send` forever. Depth stays bounded anyway, because a result only
        // exists for a request that got through the bounded request queue.
        let (result_sender, result_receiver) = unbounded::<DecodeResult>();
        let handles = (0..worker_count)
            .map(|_| spawn_worker(request_receiver.clone(), result_sender.clone()))
            .collect();
        drop(result_sender);

        Ok(Self {
            request_sender: Some(request_sender),
            result_receiver,
            handles,
        })
    }

    pub fn submit(&self, request: DecodeRequest) -> Result<(), WorkerError> {
        let Some(sender) = &self.request_sender else {
            return Err(WorkerError::ShutDown);
        };
        sender.try_send(request).map_err(|err| match err {
            TrySendError::Full(_) => WorkerError::QueueFull,
            TrySendError::Disconnected(_) => WorkerError::ShutDown,
        })
    }

    pub fn try_recv(&self) -> Option<DecodeResult> {
        self.result_receiver.try_recv().ok()
    }

    pub fn shutdown(mut self) -> Result<(), WorkerError> {
        self.request_sender.take();
        for handle in self.handles.drain(..) {
            handle.join().map_err(|_| WorkerError::WorkerPanicked)?;
        }
        Ok(())
    }
}

impl Drop for WorkerPool {
    /// Signals shutdown without waiting. Pools are dropped on the GUI thread
    /// whenever a comic is closed or rotated, and joining there stalls the
    /// frame for however long the in-progress decodes take. Closing the request
    /// channel is enough: each worker finishes its current page, sees the
    /// closed channel, and exits. Callers that need the threads gone
    /// deterministically use `shutdown`.
    fn drop(&mut self) {
        self.request_sender.take();
    }
}

fn spawn_worker(
    request_receiver: Receiver<DecodeRequest>,
    result_sender: Sender<DecodeResult>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while let Ok(request) = request_receiver.recv() {
            if result_sender.send(decode_page(request)).is_err() {
                break;
            }
        }
    })
}
