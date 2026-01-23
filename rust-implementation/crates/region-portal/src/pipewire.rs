//! PipeWire stream handling for screen capture.

use region_capture::{CaptureError, Frame, FrameData, Result};
use region_core::{Rectangle, PixelFormat};
use std::sync::Arc;
use std::time::Instant;

/// PipeWire stream state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    Disconnected,
    Connecting,
    Paused,
    Streaming,
    Error,
}

/// PipeWire stream for receiving video frames.
pub struct PipeWireStream {
    node_id: u32,
    state: StreamState,
    sequence: u64,
    format: PixelFormat,
}

impl PipeWireStream {
    /// Connect to a PipeWire node.
    pub async fn connect(node_id: u32) -> Result<Self> {
        // In a real implementation, this would:
        // 1. Initialize PipeWire context
        // 2. Create a stream
        // 3. Connect to the specified node
        // 4. Set up buffer callbacks
        
        Ok(Self {
            node_id,
            state: StreamState::Connecting,
            sequence: 0,
            format: PixelFormat::BGRA8888,
        })
    }

    /// Capture a frame from the stream.
    pub async fn capture_frame(&mut self, region: Rectangle) -> Result<Frame> {
        if self.state != StreamState::Streaming && self.state != StreamState::Connecting {
            return Err(CaptureError::CaptureFailed("Stream not active".to_string()));
        }

        // Simulate streaming state
        self.state = StreamState::Streaming;
        self.sequence += 1;

        // In a real implementation, this would:
        // 1. Wait for next buffer from PipeWire
        // 2. Import DMA-BUF if available (zero-copy)
        // 3. Or copy buffer data
        // 4. Extract region if needed
        
        // Mock frame data
        let bytes_per_pixel = self.format.bytes_per_pixel();
        let buffer_size = (region.width * region.height * bytes_per_pixel as u32) as usize;
        let buffer = Arc::new(vec![0u8; buffer_size]);

        Ok(Frame {
            width: region.width,
            height: region.height,
            format: self.format,
            data: FrameData::Buffer(buffer),
            timestamp: Instant::now(),
            sequence: self.sequence,
            region,
        })
    }

    /// Get the current stream state.
    pub fn state(&self) -> StreamState {
        self.state
    }

    /// Get the PipeWire node ID.
    pub fn node_id(&self) -> u32 {
        self.node_id
    }

    /// Disconnect from the stream.
    pub async fn disconnect(&mut self) -> Result<()> {
        self.state = StreamState::Disconnected;
        Ok(())
    }

    /// Check if DMA-BUF is supported.
    pub fn supports_dmabuf(&self) -> bool {
        // Check if the PipeWire stream negotiated DMA-BUF
        // This would be determined during connection
        true // Assume supported for now
    }
    
    /// Get the stream size (width, height).
    pub async fn get_stream_size(&self) -> Result<(u32, u32)> {
        // Dans une vraie implémentation, ceci retournerait la taille
        // négociée avec PipeWire. Pour l'instant, on retourne une valeur par défaut.
        Ok((1920, 1080))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stream_creation() {
        let stream = PipeWireStream::connect(42).await.unwrap();
        assert_eq!(stream.node_id(), 42);
        assert_eq!(stream.state(), StreamState::Connecting);
    }

    #[tokio::test]
    async fn test_frame_capture() {
        let mut stream = PipeWireStream::connect(42).await.unwrap();
        let region = Rectangle::new(0, 0, 1920, 1080);
        
        let frame = stream.capture_frame(region).await.unwrap();
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
        assert_eq!(stream.state(), StreamState::Streaming);
    }

    #[tokio::test]
    async fn test_disconnect() {
        let mut stream = PipeWireStream::connect(42).await.unwrap();
        stream.disconnect().await.unwrap();
        assert_eq!(stream.state(), StreamState::Disconnected);
    }

    #[test]
    fn test_stream_states() {
        assert_eq!(StreamState::Disconnected, StreamState::Disconnected);
        assert_ne!(StreamState::Connecting, StreamState::Streaming);
    }
}
