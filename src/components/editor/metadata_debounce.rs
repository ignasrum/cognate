use iced::futures::channel::mpsc;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub(super) struct MetadataDebounceScheduler {
    schedule_sender: std_mpsc::Sender<u64>,
}

impl MetadataDebounceScheduler {
    pub(super) fn new(window: Duration) -> (Self, mpsc::UnboundedReceiver<u64>) {
        let (schedule_sender, schedule_receiver) = std_mpsc::channel::<u64>();
        let (elapsed_sender, elapsed_receiver) = mpsc::unbounded::<u64>();

        if let Err(_error) = std::thread::Builder::new()
            .name("cognate-metadata-debounce".to_string())
            .spawn(move || Self::run_worker(window, schedule_receiver, elapsed_sender))
        {
            #[cfg(debug_assertions)]
            eprintln!("Failed to start metadata debounce worker: {}", _error);
        }

        (Self { schedule_sender }, elapsed_receiver)
    }

    pub(super) fn schedule(&self, generation: u64) {
        let _ = self.schedule_sender.send(generation);
    }

    fn run_worker(
        window: Duration,
        schedule_receiver: std_mpsc::Receiver<u64>,
        elapsed_sender: mpsc::UnboundedSender<u64>,
    ) {
        let mut pending_generation = match schedule_receiver.recv() {
            Ok(generation) => generation,
            Err(_) => return,
        };
        let mut deadline = std::time::Instant::now() + window;

        loop {
            let timeout = deadline.saturating_duration_since(std::time::Instant::now());

            match schedule_receiver.recv_timeout(timeout) {
                Ok(generation) => {
                    pending_generation = generation;
                    deadline = std::time::Instant::now() + window;
                }
                Err(std_mpsc::RecvTimeoutError::Timeout) => {
                    if elapsed_sender.unbounded_send(pending_generation).is_err() {
                        return;
                    }

                    pending_generation = match schedule_receiver.recv() {
                        Ok(generation) => generation,
                        Err(_) => return,
                    };
                    deadline = std::time::Instant::now() + window;
                }
                Err(std_mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}
