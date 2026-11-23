//! Zero-copy `FrameHub` fan-out broadcaster.
//!
//! Feeds a single captured `Arc<Frame>` concurrently to multiple independent
//! consumers (mirror preview window, live video recorder, PipeWire virtual camera,
//! snapshot grabber) with per-consumer drop-oldest backpressure so a slow or
//! minimized preview window never stalls disk recording.

use crate::Frame;
use crossbeam_channel::{self as channel, Receiver, Sender, TrySendError};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

struct SubscriberSlot {
    id: u64,
    name: String,
    tx: Sender<Arc<Frame>>,
    dropped: Arc<AtomicU64>,
}

/// Statistics for an individual hub subscriber or the hub itself.
#[derive(Debug, Clone)]
pub struct HubStats {
    pub frames_published: u64,
    pub subscriber_count: usize,
    pub subscriber_drops: Vec<(String, u64)>,
}

/// Fan-out hub broadcasting captured `Arc<Frame>` to any number of subscribers.
#[derive(Clone)]
pub struct FrameHub {
    subscribers: Arc<RwLock<Vec<SubscriberSlot>>>,
    next_id: Arc<AtomicU64>,
    published: Arc<AtomicU64>,
}

/// A live consumer subscription to a [`FrameHub`].
/// Automatically unsubscribes on drop.
pub struct FrameSubscriber {
    id: u64,
    rx: Receiver<Arc<Frame>>,
    dropped: Arc<AtomicU64>,
    subscribers: Arc<RwLock<Vec<SubscriberSlot>>>,
}

impl FrameHub {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(Vec::new())),
            next_id: Arc::new(AtomicU64::new(1)),
            published: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Subscribe with a bounded queue of `capacity` frames.
    ///
    /// Recommended capacities:
    /// - Mirror preview UI: `2` (always show latest frame, drop older)
    /// - Live video recorder: `16..32` (absorb brief encoder spikes without drops)
    /// - Virtual camera: `2` (low latency)
    pub fn subscribe(&self, name: impl Into<String>, capacity: usize) -> FrameSubscriber {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel::bounded(capacity.max(1));
        let dropped = Arc::new(AtomicU64::new(0));
        self.subscribers.write().push(SubscriberSlot {
            id,
            name: name.into(),
            tx,
            dropped: Arc::clone(&dropped),
        });
        FrameSubscriber {
            id,
            rx,
            dropped,
            subscribers: Arc::clone(&self.subscribers),
        }
    }

    /// Publish one frame to all active subscribers (zero-copy `Arc::clone`).
    /// Disconnected subscribers are pruned automatically.
    pub fn publish(&self, frame: Arc<Frame>) {
        self.published.fetch_add(1, Ordering::Relaxed);
        let mut stale_ids: Vec<u64> = Vec::new();
        {
            let subs = self.subscribers.read();
            for slot in subs.iter() {
                match slot.tx.try_send(Arc::clone(&frame)) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) => {
                        slot.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(TrySendError::Disconnected(_)) => {
                        stale_ids.push(slot.id);
                    }
                }
            }
        }
        if !stale_ids.is_empty() {
            let mut subs = self.subscribers.write();
            subs.retain(|s| !stale_ids.contains(&s.id));
        }
    }

    pub fn stats(&self) -> HubStats {
        let subs = self.subscribers.read();
        HubStats {
            frames_published: self.published.load(Ordering::Relaxed),
            subscriber_count: subs.len(),
            subscriber_drops: subs
                .iter()
                .map(|s| (s.name.clone(), s.dropped.load(Ordering::Relaxed)))
                .collect(),
        }
    }
}

impl Default for FrameHub {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameSubscriber {
    pub fn receiver(&self) -> &Receiver<Arc<Frame>> {
        &self.rx
    }

    /// Non-blocking drain returning the most recent queued frame (ideal for UI preview).
    pub fn try_recv_latest(&self) -> Option<Arc<Frame>> {
        let mut latest = None;
        while let Ok(frame) = self.rx.try_recv() {
            latest = Some(frame);
        }
        latest
    }

    /// Blocking receive for the next frame in order (ideal for live video recording).
    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Option<Arc<Frame>> {
        self.rx.recv_timeout(timeout).ok()
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl Drop for FrameSubscriber {
    fn drop(&mut self) {
        let mut subs = self.subscribers.write();
        subs.retain(|s| s.id != self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FrameData;
    use region_core::{PixelFormat, Rectangle};

    fn dummy_frame(seq: u64) -> Arc<Frame> {
        Arc::new(Frame::new(
            16,
            16,
            PixelFormat::BGRA8888,
            FrameData::from_buffer(vec![seq as u8; 16 * 16 * 4]),
            Rectangle::new(0, 0, 16, 16),
            seq,
        ))
    }

    #[test]
    fn test_fanout_independent_backpressure() {
        let hub = FrameHub::new();
        let slow_ui = hub.subscribe("preview", 2);
        let fast_recorder = hub.subscribe("recorder", 10);

        for seq in 1..=6 {
            hub.publish(dummy_frame(seq));
        }

        // Recorder received all 6 frames intact
        let mut rec_seqs = Vec::new();
        while let Ok(f) = fast_recorder.receiver().try_recv() {
            rec_seqs.push(f.sequence);
        }
        assert_eq!(rec_seqs, vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(fast_recorder.dropped_frames(), 0);

        // Slow UI dropped 4 frames without blocking the recorder
        assert_eq!(slow_ui.dropped_frames(), 4);
        assert!(slow_ui.try_recv_latest().is_some());

        drop(slow_ui);
        assert_eq!(hub.stats().subscriber_count, 1);
    }
}
