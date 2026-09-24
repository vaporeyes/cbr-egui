// ABOUTME: Runs bounded image decode work away from the GUI thread.
// ABOUTME: Disconnects result delivery before joining workers at shutdown.
use std::thread::{self, JoinHandle};

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};

use super::error::WorkerError;
use super::pipeline::{DecodeRequest, DecodeResult, decode_page};

pub struct WorkerPool {
    request_sender: Option<Sender<DecodeRequest>>,
    result_receiver: Option<Receiver<DecodeResult>>,
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
        // Bound completed pixel buffers as well as requests. Shutdown drops
        // the receiver first, waking any worker blocked on a full result queue.
        let (result_sender, result_receiver) = bounded::<DecodeResult>(worker_count);
        let handles = (0..worker_count)
            .map(|_| spawn_worker(request_receiver.clone(), result_sender.clone()))
            .collect();
        drop(result_sender);

        Ok(Self {
            request_sender: Some(request_sender),
            result_receiver: Some(result_receiver),
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
        self.result_receiver.as_ref()?.try_recv().ok()
    }

    pub fn shutdown(mut self) -> Result<(), WorkerError> {
        self.request_sender.take();
        self.result_receiver.take();
        let mut panicked = false;
        for handle in self.handles.drain(..) {
            panicked |= handle.join().is_err();
        }
        if panicked {
            Err(WorkerError::WorkerPanicked)
        } else {
            Ok(())
        }
    }
}

impl Drop for WorkerPool {
    /// Signals shutdown without waiting. Pools are dropped on the GUI thread
    /// whenever a comic is closed or rotated, and joining there stalls the
    /// frame for however long the in-progress decodes take. Closing the request
    /// and result channels lets each worker finish its current page and exit.
    /// Callers that need the threads gone
    /// deterministically use `shutdown`.
    fn drop(&mut self) {
        self.request_sender.take();
        self.result_receiver.take();
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
