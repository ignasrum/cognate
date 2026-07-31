use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Serializes and coalesces API content writes independently for each note.
#[derive(Clone, Default)]
pub(crate) struct WriteCoordinator {
    states: Arc<Mutex<HashMap<String, Arc<NoteWriteState>>>>,
}

struct NoteWriteState {
    generation: Mutex<u64>,
    lock: Arc<tokio::sync::Mutex<()>>,
}

pub(crate) struct WriteTicket {
    state: Arc<NoteWriteState>,
    generation: u64,
}

impl WriteCoordinator {
    pub(crate) fn begin(&self, rel_path: &str) -> WriteTicket {
        let state = {
            let mut states = self.states.lock().unwrap();
            states
                .entry(rel_path.to_string())
                .or_insert_with(|| {
                    Arc::new(NoteWriteState {
                        generation: Mutex::new(0),
                        lock: Arc::new(tokio::sync::Mutex::new(())),
                    })
                })
                .clone()
        };
        let generation = {
            let mut current = state.generation.lock().unwrap();
            *current = current.wrapping_add(1);
            *current
        };
        WriteTicket { state, generation }
    }
}

impl WriteTicket {
    pub(crate) async fn acquire(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.state.lock.clone().lock_owned().await
    }

    pub(crate) fn was_superseded(&self) -> bool {
        *self.state.generation.lock().unwrap() != self.generation
    }
}

#[cfg(test)]
mod tests {
    use super::WriteCoordinator;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn newer_generation_supersedes_older_write() {
        let coordinator = WriteCoordinator::default();
        let first = coordinator.begin("note");
        let second = coordinator.begin("note");
        assert!(first.was_superseded());
        assert!(!second.was_superseded());
    }

    #[tokio::test]
    async fn different_notes_have_independent_locks() {
        let coordinator = WriteCoordinator::default();
        let note = coordinator.begin("note");
        let other = coordinator.begin("other");
        let _note_guard = note.acquire().await;
        let (sender, receiver) = oneshot::channel();
        tokio::spawn(async move {
            let _guard = other.acquire().await;
            let _ = sender.send(());
        });
        tokio::time::timeout(std::time::Duration::from_millis(100), receiver)
            .await
            .expect("different note should not be blocked")
            .unwrap();
    }
}
