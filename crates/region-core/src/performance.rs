//! Performance optimization utilities and profiling.

use std::time::{Duration, Instant};
use std::collections::VecDeque;

/// Frame profiler for measuring capture performance.
pub struct FrameProfiler {
    frame_times: VecDeque<Duration>,
    capture_times: VecDeque<Duration>,
    max_samples: usize,
    last_frame_time: Option<Instant>,
}

impl FrameProfiler {
    /// Create a new frame profiler.
    pub fn new(max_samples: usize) -> Self {
        Self {
            frame_times: VecDeque::with_capacity(max_samples),
            capture_times: VecDeque::with_capacity(max_samples),
            max_samples,
            last_frame_time: None,
        }
    }

    /// Mark the start of a new frame.
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

    /// Record a capture operation duration.
    pub fn record_capture(&mut self, duration: Duration) {
        self.capture_times.push_back(duration);
        
        if self.capture_times.len() > self.max_samples {
            self.capture_times.pop_front();
        }
    }

    /// Get average FPS.
    pub fn avg_fps(&self) -> f64 {
        if self.frame_times.is_empty() {
            return 0.0;
        }

        let avg_frame_time: Duration = self.frame_times.iter()
            .sum::<Duration>() / self.frame_times.len() as u32;

        if avg_frame_time.as_secs_f64() > 0.0 {
            1.0 / avg_frame_time.as_secs_f64()
        } else {
            0.0
        }
    }

    /// Get average capture time in milliseconds.
    pub fn avg_capture_ms(&self) -> f64 {
        if self.capture_times.is_empty() {
            return 0.0;
        }

        let avg: Duration = self.capture_times.iter()
            .sum::<Duration>() / self.capture_times.len() as u32;

        avg.as_secs_f64() * 1000.0
    }

    /// Get minimum frame time in milliseconds.
    pub fn min_frame_ms(&self) -> f64 {
        self.frame_times.iter()
            .min()
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }

    /// Get maximum frame time in milliseconds.
    pub fn max_frame_ms(&self) -> f64 {
        self.frame_times.iter()
            .max()
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }

    /// Get the 99th percentile frame time.
    pub fn p99_frame_ms(&self) -> f64 {
        if self.frame_times.is_empty() {
            return 0.0;
        }

        let mut sorted: Vec<_> = self.frame_times.iter().collect();
        sorted.sort();

        let idx = (sorted.len() as f64 * 0.99) as usize;
        sorted.get(idx)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }

    /// Get statistics summary.
    pub fn summary(&self) -> String {
        format!(
            "FPS: {:.1} | Capture: {:.2}ms | Frame: {:.2}ms (min: {:.2}, max: {:.2}, p99: {:.2})",
            self.avg_fps(),
            self.avg_capture_ms(),
            self.frame_times.iter().sum::<Duration>().as_secs_f64() * 1000.0 / self.frame_times.len().max(1) as f64,
            self.min_frame_ms(),
            self.max_frame_ms(),
            self.p99_frame_ms()
        )
    }

    /// Get structured statistics.
    pub fn stats(&self) -> ProfilerStats {
        ProfilerStats {
            avg_fps: self.avg_fps(),
            avg_capture_ms: self.avg_capture_ms(),
            avg_frame_ms: self.frame_times.iter().sum::<Duration>().as_secs_f64() * 1000.0 / self.frame_times.len().max(1) as f64,
            min_frame_ms: self.min_frame_ms(),
            max_frame_ms: self.max_frame_ms(),
            p99_frame_ms: self.p99_frame_ms(),
        }
    }

    /// Reset all statistics.
    pub fn reset(&mut self) {
        self.frame_times.clear();
        self.capture_times.clear();
        self.last_frame_time = None;
    }
}

/// Structured profiler statistics.
#[derive(Debug, Clone, Copy)]
pub struct ProfilerStats {
    pub avg_fps: f64,
    pub avg_capture_ms: f64,
    pub avg_frame_ms: f64,
    pub min_frame_ms: f64,
    pub max_frame_ms: f64,
    pub p99_frame_ms: f64,
}

/// Memory pool for reusing frame buffers.
pub struct BufferPool {
    buffers: Vec<Vec<u8>>,
    buffer_size: usize,
}

impl BufferPool {
    /// Create a new buffer pool.
    pub fn new(buffer_size: usize, initial_capacity: usize) -> Self {
        let mut buffers = Vec::with_capacity(initial_capacity);
        for _ in 0..initial_capacity {
            buffers.push(vec![0u8; buffer_size]);
        }

        Self {
            buffers,
            buffer_size,
        }
    }

    /// Acquire a buffer from the pool.
    pub fn acquire(&mut self) -> Vec<u8> {
        self.buffers.pop().unwrap_or_else(|| vec![0u8; self.buffer_size])
    }

    /// Return a buffer to the pool.
    pub fn release(&mut self, mut buffer: Vec<u8>) {
        if buffer.len() == self.buffer_size {
            buffer.clear();
            buffer.resize(self.buffer_size, 0);
            self.buffers.push(buffer);
        }
    }

    /// Get pool statistics.
    pub fn available(&self) -> usize {
        self.buffers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_profiler_basic() {
        let mut profiler = FrameProfiler::new(100);
        
        profiler.start_frame();
        thread::sleep(Duration::from_millis(16)); // ~60 FPS
        profiler.start_frame();
        thread::sleep(Duration::from_millis(16));
        profiler.start_frame();

        let fps = profiler.avg_fps();
        assert!(fps > 50.0 && fps < 70.0, "FPS should be around 60");
    }

    #[test]
    fn test_profiler_capture() {
        let mut profiler = FrameProfiler::new(100);
        
        profiler.record_capture(Duration::from_millis(5));
        profiler.record_capture(Duration::from_millis(10));
        profiler.record_capture(Duration::from_millis(7));

        let avg = profiler.avg_capture_ms();
        assert!((avg - 7.333).abs() < 0.1);
    }

    #[test]
    fn test_profiler_percentiles() {
        let mut profiler = FrameProfiler::new(100);
        
        for i in 1..=100 {
            profiler.start_frame();
            thread::sleep(Duration::from_micros(i * 100));
        }

        let p99 = profiler.p99_frame_ms();
        assert!(p99 > 0.0);
    }

    #[test]
    fn test_buffer_pool() {
        let mut pool = BufferPool::new(1024, 5);
        
        assert_eq!(pool.available(), 5);
        
        let buf1 = pool.acquire();
        assert_eq!(buf1.len(), 1024);
        assert_eq!(pool.available(), 4);
        
        pool.release(buf1);
        assert_eq!(pool.available(), 5);
    }

    #[test]
    fn test_buffer_pool_overflow() {
        let mut pool = BufferPool::new(512, 2);
        
        let _buf1 = pool.acquire();
        let _buf2 = pool.acquire();
        let buf3 = pool.acquire(); // Should allocate new
        
        assert_eq!(buf3.len(), 512);
        assert_eq!(pool.available(), 0);
    }

    #[test]
    fn test_profiler_reset() {
        let mut profiler = FrameProfiler::new(100);
        
        profiler.start_frame();
        profiler.record_capture(Duration::from_millis(5));
        
        profiler.reset();
        
        assert_eq!(profiler.avg_fps(), 0.0);
        assert_eq!(profiler.avg_capture_ms(), 0.0);
    }
}
