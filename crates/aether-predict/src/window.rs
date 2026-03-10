//! Sliding window buffer for time-series feature vectors.
//!
//! Maintains a fixed-capacity window of [`FeatureVector`] per process,
//! suitable for feeding into ONNX models that expect sequential input.

use std::collections::{HashMap, VecDeque};

use crate::features::FeatureVector;

const FEATURE_DIM: usize = 9;

/// Per-process sliding window of feature vectors.
///
/// Stores the most recent `capacity` samples per PID. When full,
/// pushing a new sample evicts the oldest one (ring buffer semantics).
#[derive(Debug)]
pub struct SlidingWindow {
    windows: HashMap<u32, VecDeque<FeatureVector>>,
    capacity: usize,
}

impl SlidingWindow {
    /// Creates a window with the given capacity per process.
    pub fn new(capacity: usize) -> Self {
        Self {
            windows: HashMap::new(),
            capacity,
        }
    }

    /// Pushes a feature vector for a process. Evicts the oldest if at capacity.
    pub fn push(&mut self, pid: u32, features: FeatureVector) {
        let deque = self
            .windows
            .entry(pid)
            .or_insert_with(|| VecDeque::with_capacity(self.capacity));

        if deque.len() == self.capacity {
            deque.pop_front();
        }
        deque.push_back(features);
    }

    /// Returns the feature window for a process, if any samples exist.
    pub fn get_window(&self, pid: u32) -> Option<&VecDeque<FeatureVector>> {
        self.windows.get(&pid)
    }

    /// Whether the window for `pid` has reached full capacity.
    pub fn is_full(&self, pid: u32) -> bool {
        self.windows
            .get(&pid)
            .is_some_and(|d| d.len() == self.capacity)
    }

    /// Flattens a full window into a contiguous tensor (Vec<f32>).
    ///
    /// Returns `None` if the window for `pid` doesn't exist or isn't full yet,
    /// since partial windows would produce incorrect-sized model input.
    pub fn to_tensor(&self, pid: u32) -> Option<Vec<f32>> {
        let deque = self.windows.get(&pid)?;
        if deque.len() != self.capacity {
            return None;
        }

        let mut tensor = Vec::with_capacity(self.capacity * FEATURE_DIM);
        for fv in deque {
            tensor.extend_from_slice(fv);
        }
        Some(tensor)
    }

    /// Removes windows for PIDs not present in `alive_pids`.
    pub fn cleanup(&mut self, alive_pids: &[u32]) {
        self.windows.retain(|pid, _| alive_pids.contains(pid));
    }
}

impl Default for SlidingWindow {
    fn default() -> Self {
        Self::new(60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_fv(seed: f32) -> FeatureVector {
        [seed; 9]
    }

    #[test]
    fn test_push_60_samples_window_full() {
        let mut win = SlidingWindow::default();
        for i in 0..60 {
            win.push(1, sample_fv(i as f32));
        }
        assert!(win.is_full(1), "window should be full after 60 pushes");
        assert_eq!(win.get_window(1).unwrap().len(), 60);
    }

    #[test]
    fn test_push_61_evicts_oldest() {
        let mut win = SlidingWindow::default();
        for i in 0..61 {
            win.push(1, sample_fv(i as f32));
        }
        assert!(win.is_full(1));
        let deque = win.get_window(1).unwrap();
        assert_eq!(deque.len(), 60);
        // The oldest (seed=0.0) should have been evicted; first element is seed=1.0.
        assert_eq!(deque[0][0], 1.0, "oldest sample should be evicted");
        assert_eq!(deque[59][0], 60.0, "newest sample should be last");
    }

    #[test]
    fn test_to_tensor_correct_size() {
        let mut win = SlidingWindow::default();
        for i in 0..60 {
            win.push(1, sample_fv(i as f32));
        }
        let tensor = win.to_tensor(1).expect("full window should produce tensor");
        assert_eq!(tensor.len(), 60 * 9, "tensor should be 60*9=540 floats");
    }

    #[test]
    fn test_cleanup_removes_dead_processes() {
        let mut win = SlidingWindow::default();
        win.push(1, sample_fv(1.0));
        win.push(2, sample_fv(2.0));
        win.push(3, sample_fv(3.0));

        win.cleanup(&[1, 3]);

        assert!(win.get_window(1).is_some(), "pid 1 should survive");
        assert!(win.get_window(2).is_none(), "pid 2 should be removed");
        assert!(win.get_window(3).is_some(), "pid 3 should survive");
    }

    #[test]
    fn test_partial_window_to_tensor_returns_none() {
        let mut win = SlidingWindow::default();
        for i in 0..30 {
            win.push(1, sample_fv(i as f32));
        }
        assert!(
            win.to_tensor(1).is_none(),
            "partial window should not produce tensor"
        );
    }
}
