//! Single-slot delivery for replaceable, complete UI snapshots.
//! Commands, protocol deltas and approval replies must use ordered channels.
use std::sync::{
    Arc, Mutex,
    mpsc::{SendError, TryRecvError},
};

struct Slot<T> {
    value: Option<T>,
    sender_alive: bool,
    receiver_alive: bool,
}

pub struct Sender<T>(Arc<Mutex<Slot<T>>>);
pub struct Receiver<T>(Arc<Mutex<Slot<T>>>);

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let slot = Arc::new(Mutex::new(Slot {
        value: None,
        sender_alive: true,
        receiver_alive: true,
    }));
    (Sender(Arc::clone(&slot)), Receiver(slot))
}

impl<T> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        let mut slot = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if !slot.receiver_alive {
            return Err(SendError(value));
        }
        let previous = slot.value.replace(value);
        drop(slot);
        // Free an obsolete large snapshot without holding the UI's lock.
        drop(previous);
        Ok(())
    }
}

impl<T> Receiver<T> {
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let mut slot = self.0.lock().unwrap_or_else(|error| error.into_inner());
        slot.value.take().ok_or(if slot.sender_alive {
            TryRecvError::Empty
        } else {
            TryRecvError::Disconnected
        })
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .sender_alive = false;
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut slot = self.0.lock().unwrap_or_else(|error| error.into_inner());
        slot.receiver_alive = false;
        let previous = slot.value.take();
        drop(slot);
        drop(previous);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_consumer_retains_only_latest_complete_snapshot() {
        let (tx, rx) = channel();
        let first = Arc::new(vec![0; 1024]);
        tx.send(Arc::clone(&first)).unwrap();
        for value in 1..10_000 {
            tx.send(Arc::new(vec![value])).unwrap();
        }
        assert_eq!(Arc::strong_count(&first), 1);
        assert_eq!(*rx.try_recv().unwrap(), vec![9999]);
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn final_value_survives_sender_and_receiver_drop_releases_pending() {
        let (tx, rx) = channel();
        tx.send(42).unwrap();
        drop(tx);
        assert_eq!(rx.try_recv(), Ok(42));
        assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
        let (tx, rx) = channel();
        let value = Arc::new(42);
        tx.send(Arc::clone(&value)).unwrap();
        drop(rx);
        assert_eq!(Arc::strong_count(&value), 1);
        assert!(tx.send(value).is_err());
    }
}
