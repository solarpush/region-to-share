//! Performance optimization utilities, frame profiler, and lock-safe buffer pools.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Frame profiler for measuring capture and render performance.
pub struct FrameProfiler {
    frame_times: VecDeque<Duration>,
    capture_times: VecDeque<Duration>,
    max_samples: usize,
    last_frame_time: Option<Instant>,
}

impl FrameProfiler {
    pub fn new(max_samples: usize) -> Self {
        Self {
            frame_times: VecDeque::with_capacity(max_samples),
            capture_times: VecDeque::with_capacity(max_samples),
            max_samples: max_samples.max(1),
            last_frame_time: None,
        }
    }

    pub fn start_frame(&mut self) {
        let now = Instant::now();
        if let Some(last) = self.last_frame_time {
            let frame_time = now.duration_since(last);
            self.frame_times.push_back(frame_time);
            if self.frame_times.len() > self.max_samples {
                self.frame_times.pop_front();
            }
        }
        self.last_frame_time = Some(now);
    }

    pub fn record_capture(&mut self, duration: Duration) {
        self.capture_times.push_back(duration);
        if self.capture_times.len() > self.max_samples {
            self.capture_times.pop_front();
        }
    }

    pub fn avg_fps(&self) -> f64 {
        if self.frame_times.is_empty() {
            return 0.0;
        }
        let avg_frame_time: Duration =
            self.frame_times.iter().sum::<Duration>() / self.frame_times.len() as u32;
        if avg_frame_time.as_secs_f64() > 0.0 {
            1.0 / avg_frame_time.as_secs_f64()
        } else {
            0.0
        }
    }

    pub fn avg_capture_ms(&self) -> f64 {
        if self.capture_times.is_empty() {
            return 0.0;
        }
        let avg: Duration =
            self.capture_times.iter().sum::<Duration>() / self.capture_times.len() as u32;
        avg.as_secs_f64() * 1000.0
    }

    pub fn min_frame_ms(&self) -> f64 {
        self.frame_times
            .iter()
            .min()
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }

    pub fn max_frame_ms(&self) -> f64 {
        self.frame_times
            .iter()
            .max()
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }

    pub fn p99_frame_ms(&self) -> f64 {
        if self.frame_times.is_empty() {
            return 0.0;
        }
        let mut sorted: Vec<_> = self.frame_times.iter().collect();
        sorted.sort();
        let idx = ((sorted.len() as f64 * 0.99) as usize).min(sorted.len() - 1);
        sorted
            .get(idx)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }

    pub fn stats(&self) -> ProfilerStats {
        ProfilerStats {
            avg_fps: self.avg_fps(),
            avg_capture_ms: self.avg_capture_ms(),
            avg_frame_ms: self.frame_times.iter().sum::<Duration>().as_secs_f64() * 1000.0
                / self.frame_times.len().max(1) as f64,
            min_frame_ms: self.min_frame_ms(),
            max_frame_ms: self.max_frame_ms(),
            p99_frame_ms: self.p99_frame_ms(),
        }
    }

    pub fn summary(&self) -> String {
        let s = self.stats();
        format!(
            "FPS: {:.1} | Capture: {:.2}ms | Frame: {:.2}ms (min: {:.2}, max: {:.2}, p99: {:.2})",
            s.avg_fps, s.avg_capture_ms, s.avg_frame_ms, s.min_frame_ms, s.max_frame_ms, s.p99_frame_ms
        )
    }

    pub fn reset(&mut self) {
        self.frame_times.clear();
        self.capture_times.clear();
        self.last_frame_time = None;
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ProfilerStats {
    pub avg_fps: f64,
    pub avg_capture_ms: f64,
    pub avg_frame_ms: f64,
    pub min_frame_ms: f64,
    pub max_frame_ms: f64,
    pub p99_frame_ms: f64,
}

/// Single-threaded reusable byte-buffer pool.
pub struct BufferPool {
    buffers: Vec<Vec<u8>>,
    buffer_size: usize,
    max_cached: usize,
}

impl BufferPool {
    pub fn new(buffer_size: usize, initial_capacity: usize) -> Self {
        let mut buffers = Vec::with_capacity(initial_capacity);
        for _ in 0..initial_capacity {
            buffers.push(Vec::with_capacity(buffer_size));
        }
        Self {
            buffers,
            buffer_size,
            max_cached: initial_capacity.max(4),
        }
    }

    pub fn acquire(&mut self) -> Vec<u8> {
        let mut buf = self
            .buffers
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(self.buffer_size));
        buf.resize(self.buffer_size, 0);
        buf
    }

    pub fn release(&mut self, mut buffer: Vec<u8>) {
        if buffer.capacity() >= self.buffer_size && self.buffers.len() < self.max_cached {
            buffer.clear();
            self.buffers.push(buffer);
        }
    }

    pub fn available(&self) -> usize {
        self.buffers.len()
    }
}

/// Thread-safe reusable byte-buffer pool that adapts automatically to region resizes
/// without repeatedly allocating/freeing multi-megabyte frame buffers at 60 FPS.
#[derive(Clone)]
pub struct SharedBufferPool {
    inner: Arc<Mutex<SharedPoolInner>>,
}

struct SharedPoolInner {
    buffers: Vec<Vec<u8>>,
    max_cached: usize,
}

impl SharedBufferPool {
    pub fn new(max_cached: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SharedPoolInner {
                buffers: Vec::with_capacity(max_cached),
                max_cached: max_cached.max(2),
            })),
        }
    }

    /// Acquire a cleared buffer with at least `min_capacity` bytes allocated.
    pub fn acquire_with_capacity(&self, min_capacity: usize) -> Vec<u8> {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(mut buf) = guard.buffers.pop() {
                buf.clear();
                if buf.capacity() < min_capacity {
                    buf.reserve(min_capacity - buf.capacity());
                }
                return buf;
            }
        }
        Vec::with_capacity(min_capacity)
    }

    /// Return a buffer to the pool if capacity isn't absurdly oversized.
    pub fn release(&self, mut buf: Vec<u8>) {
        if let Ok(mut guard) = self.inner.lock() {
            if guard.buffers.len() < guard.max_cached {
                buf.clear();
                guard.buffers.push(buf);
            }
        }
    }

    pub fn available(&self) -> usize {
        self.inner.lock().map(|g| g.buffers.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_buffer_pool_reuse() {
        let pool = SharedBufferPool::new(4);
        let mut b1 = pool.acquire_with_capacity(4096);
        b1.extend_from_slice(&[42u8; 1024]);
        let ptr = b1.as_ptr();
        pool.release(b1);
        assert_eq!(pool.available(), 1);

        let b2 = pool.acquire_with_capacity(2048);
        assert_eq!(b2.len(), 0);
        assert!(b2.capacity() >= 4096);
        assert_eq!(b2.as_ptr(), ptr);
    }
}
