use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc::{self, Receiver, Sender};

type ChannelId = u64;

/// Root controller of streaming channels
pub struct Controller<M>(Arc<Mutex<ChannelsInner<M>>>);

impl<M> Clone for Controller<M> {
    fn clone(&self) -> Self { Self(Arc::clone(&self.0)) }
}

/// Inner part of the controller
pub struct ChannelsInner<M> {
    last_id: u64,
    channels: HashMap<ChannelId, Channel<M>>,
}

struct Channel<M> {
    tx: Sender<Arc<M>>,
}

/// guard to trace channels disconnection
pub struct ChannelGuard<M> {
    channel_id: ChannelId,
    controller: Controller<M>,
}

/// Receiver to cleanup resources on `Drop`
pub struct GuardedReceiver<M> {
    rx: Receiver<Arc<M>>,
    #[allow(dead_code)]
    guard: ChannelGuard<M>,
}

impl<M> Controller<M> {
    /// Creates a new channels controller
    pub fn new() -> Self { Default::default() }

    /// Creates a new channel and returns it's events receiver
    pub fn create_channel(&mut self, concurrency: usize) -> GuardedReceiver<M> {
        let (tx, rx) = mpsc::channel::<Arc<M>>(concurrency);
        let channel = Channel { tx };

        let mut inner = self.0.lock();
        let channel_id = inner.last_id.overflowing_add(1).0;
        inner.channels.insert(channel_id, channel);
        inner.last_id = channel_id;

        let guard = ChannelGuard::new(channel_id, self.clone());
        GuardedReceiver { rx, guard }
    }

    /// Returns number of active channels
    pub fn num_connections(&self) -> usize { self.0.lock().channels.len() }

    /// Broadcast message to all channels
    pub async fn broadcast(&self, message: M) {
        let msg = Arc::new(message);
        for rx in self.all_senders() {
            rx.send(Arc::clone(&msg)).await.ok();
        }
    }

    /// Removes the channel from the controller
    fn remove_channel(&mut self, channel_id: &ChannelId) {
        let mut inner = self.0.lock();
        inner.channels.remove(channel_id);
    }

    /// Returns all the active channels
    fn all_senders(&self) -> Vec<Sender<Arc<M>>> { self.0.lock().channels.values().map(|c| c.tx.clone()).collect() }
}

impl<M> Default for Controller<M> {
    fn default() -> Self {
        let inner = ChannelsInner {
            last_id: 0,
            channels: HashMap::new(),
        };
        Self(Arc::new(Mutex::new(inner)))
    }
}

impl<M> ChannelGuard<M> {
    fn new(channel_id: ChannelId, controller: Controller<M>) -> Self { Self { channel_id, controller } }
}

impl<M> Drop for ChannelGuard<M> {
    fn drop(&mut self) { self.controller.remove_channel(&self.channel_id); }
}

impl<M> GuardedReceiver<M> {
    /// Receives the next event from the channel
    pub async fn recv(&mut self) -> Option<Arc<M>> { self.rx.recv().await }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_create_channel_and_broadcast() {
        let mut controller = Controller::new();
        let mut guard_receiver = controller.create_channel(1);

        controller.broadcast("Message".to_string()).await;

        let received_msg = guard_receiver.recv().await.unwrap();
        assert_eq!(*received_msg, "Message".to_string());
    }

    #[tokio::test]
    async fn test_multiple_channels_and_broadcast() {
        let mut controller = Controller::new();

        let mut receivers = Vec::new();
        for _ in 0..3 {
            receivers.push(controller.create_channel(1));
        }

        controller.broadcast("Message".to_string()).await;

        for receiver in &mut receivers {
            let received_msg = receiver.recv().await.unwrap();
            assert_eq!(*received_msg, "Message".to_string());
        }
    }

    #[tokio::test]
    async fn test_channel_cleanup_on_drop() {
        let mut controller: Controller<()> = Controller::new();
        let guard_receiver = controller.create_channel(1);

        assert_eq!(controller.num_connections(), 1);

        drop(guard_receiver);

        sleep(Duration::from_millis(10)).await; // Give time for the drop to execute

        assert_eq!(controller.num_connections(), 0);
    }

    #[tokio::test]
    async fn test_broadcast_across_channels() {
        let mut controller = Controller::new();

        let mut receivers = Vec::new();
        for _ in 0..3 {
            receivers.push(controller.create_channel(1));
        }

        controller.broadcast("Message".to_string()).await;

        for receiver in &mut receivers {
            let received_msg = receiver.recv().await.unwrap();
            assert_eq!(*received_msg, "Message".to_string());
        }
    }

    #[tokio::test]
    async fn test_multiple_messages_and_drop() {
        let mut controller = Controller::new();
        let mut guard_receiver = controller.create_channel(6);

        controller.broadcast("Message 1".to_string()).await;
        controller.broadcast("Message 2".to_string()).await;
        controller.broadcast("Message 3".to_string()).await;
        controller.broadcast("Message 4".to_string()).await;
        controller.broadcast("Message 5".to_string()).await;
        controller.broadcast("Message 6".to_string()).await;

        let mut received_msgs = Vec::new();
        for _ in 0..6 {
            let received_msg = guard_receiver.recv().await.unwrap();
            received_msgs.push(received_msg);
        }

        assert_eq!(*received_msgs[0], "Message 1".to_string());
        assert_eq!(*received_msgs[1], "Message 2".to_string());
        assert_eq!(*received_msgs[2], "Message 3".to_string());
        assert_eq!(*received_msgs[3], "Message 4".to_string());
        assert_eq!(*received_msgs[4], "Message 5".to_string());
        assert_eq!(*received_msgs[5], "Message 6".to_string());

        // Consume the GuardedReceiver to trigger drop and channel cleanup
        drop(guard_receiver);

        // Sleep for a short time to allow cleanup to complete
        sleep(Duration::from_millis(10)).await;

        assert_eq!(controller.num_connections(), 0);
    }
}
