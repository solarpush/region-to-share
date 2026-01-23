//! Frame data structures for captured screen regions.

use region_core::{PixelFormat, Rectangle};
use std::sync::Arc;
use std::time::Instant;

/// A captured frame from the screen.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Width of the frame in pixels.
    pub width: u32,
    
    /// Height of the frame in pixels.
    pub height: u32,
    
    /// Pixel format of the frame data.
    pub format: PixelFormat,
    
    /// Frame data storage.
    pub data: FrameData,
    
    /// Timestamp when the frame was captured.
    pub timestamp: Instant,
    
    /// Sequence number (incremental frame counter).
    pub sequence: u64,
    
    /// The region that was captured.
    pub region: Rectangle,
}

impl Frame {
    /// Create a new frame.
    pub fn new(
        width: u32,
        height: u32,
        format: PixelFormat,
        data: FrameData,
        region: Rectangle,
        sequence: u64,
    ) -> Self {
        Self {
            width,
            height,
            format,
            data,
            timestamp: Instant::now(),
            sequence,
            region,
        }
    }

    /// Get the size of the frame data in bytes.
    pub fn data_size(&self) -> usize {
        (self.width * self.height) as usize * self.format.bytes_per_pixel()
    }

    /// Get age of the frame since capture.
    pub fn age(&self) -> std::time::Duration {
        self.timestamp.elapsed()
    }
}

/// Storage for frame pixel data.
#[derive(Debug, Clone)]
pub enum FrameData {
    /// CPU buffer (owned Vec).
    Buffer(Arc<Vec<u8>>),
    
    /// Shared memory buffer (file descriptor + offset).
    #[cfg(unix)]
    SharedMemory {
        /// File descriptor for shared memory.
        fd: std::os::unix::io::RawFd,
        /// Offset into the shared memory region.
        offset: usize,
        /// Size of the data.
        size: usize,
    },
    
    /// DMA-BUF (GPU memory, zero-copy).
    #[cfg(unix)]
    DmaBuf {
        /// DMA-BUF file descriptor.
        fd: std::os::unix::io::RawFd,
        /// DRM fourcc format code.
        fourcc: u32,
        /// Format modifier.
        modifier: u64,
        /// Number of planes.
        num_planes: u32,
        /// Stride for each plane.
        strides: [u32; 4],
        /// Offset for each plane.
        offsets: [u32; 4],
    },
}

impl FrameData {
    /// Create frame data from an owned buffer.
    pub fn from_buffer(buffer: Vec<u8>) -> Self {
        Self::Buffer(Arc::new(buffer))
    }

    /// Create frame data from shared buffer (Arc).
    pub fn from_shared_buffer(buffer: Arc<Vec<u8>>) -> Self {
        Self::Buffer(buffer)
    }

    /// Get a reference to the buffer data if available.
    pub fn as_buffer(&self) -> Option<&[u8]> {
        match self {
            Self::Buffer(buf) => Some(buf.as_slice()),
            _ => None,
        }
    }

    /// Check if this is a zero-copy frame (DMA-BUF or shared memory).
    pub fn is_zero_copy(&self) -> bool {
        #[cfg(unix)]
        {
            matches!(self, Self::DmaBuf { .. } | Self::SharedMemory { .. })
        }
        #[cfg(not(unix))]
        {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_creation() {
        let data = vec![0u8; 800 * 600 * 4];
        let frame_data = FrameData::from_buffer(data);
        let region = Rectangle::new(0, 0, 800, 600);
        
        let frame = Frame::new(
            800,
            600,
            PixelFormat::RGBA8888,
            frame_data,
            region,
            1,
        );
        
        assert_eq!(frame.width, 800);
        assert_eq!(frame.height, 600);
        assert_eq!(frame.format, PixelFormat::RGBA8888);
        assert_eq!(frame.sequence, 1);
        assert_eq!(frame.data_size(), 800 * 600 * 4);
    }

    #[test]
    fn test_frame_data_buffer() {
        let data = vec![1, 2, 3, 4];
        let frame_data = FrameData::from_buffer(data.clone());
        
        assert_eq!(frame_data.as_buffer(), Some(data.as_slice()));
        assert!(!frame_data.is_zero_copy());
    }

    #[test]
    fn test_frame_age() {
        let data = FrameData::from_buffer(vec![0; 100]);
        let region = Rectangle::new(0, 0, 10, 10);
        let frame = Frame::new(10, 10, PixelFormat::RGBA8888, data, region, 0);
        
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(frame.age().as_millis() >= 10);
    }
}
