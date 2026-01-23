//! Stream state management.

pub use crate::pipewire::StreamState;

/// Stream statistics for monitoring performance.
#[derive(Debug, Clone, Default)]
pub struct StreamStats {
    /// Total frames received
    pub frames_received: u64,
    
    /// Frames dropped due to lag
    pub frames_dropped: u64,
    
    /// Average frame rate
    pub avg_fps: f32,
    
    /// Current bandwidth usage (bytes/sec)
    pub bandwidth: u64,
}

impl StreamStats {
    /// Create new empty statistics.
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculate frame drop rate as percentage.
    pub fn drop_rate(&self) -> f32 {
        if self.frames_received == 0 {
            0.0
        } else {
            (self.frames_dropped as f32 / self.frames_received as f32) * 100.0
        }
    }

    /// Reset all statistics.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_creation() {
        let stats = StreamStats::new();
        assert_eq!(stats.frames_received, 0);
        assert_eq!(stats.frames_dropped, 0);
    }

    #[test]
    fn test_drop_rate() {
        let mut stats = StreamStats::new();
        stats.frames_received = 100;
        stats.frames_dropped = 10;
        
        assert_eq!(stats.drop_rate(), 10.0);
    }

    #[test]
    fn test_reset() {
        let mut stats = StreamStats {
            frames_received: 100,
            frames_dropped: 10,
            avg_fps: 60.0,
            bandwidth: 1000000,
        };
        
        stats.reset();
        assert_eq!(stats.frames_received, 0);
        assert_eq!(stats.frames_dropped, 0);
    }
}
