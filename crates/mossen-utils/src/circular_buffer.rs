//! A fixed-size circular buffer that automatically evicts the oldest items
//! when the buffer is full. Useful for maintaining a rolling window of data.

use std::collections::VecDeque;

/// A fixed-size circular buffer that automatically evicts the oldest items
/// when the buffer is full.
pub struct CircularBuffer<T> {
    buffer: VecDeque<T>,
    capacity: usize,
}

impl<T> CircularBuffer<T> {
    /// Creates a new CircularBuffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Add an item to the buffer. If the buffer is full,
    /// the oldest item will be evicted.
    pub fn add(&mut self, item: T) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(item);
    }

    /// Add multiple items to the buffer at once.
    pub fn add_all(&mut self, items: impl IntoIterator<Item = T>) {
        for item in items {
            self.add(item);
        }
    }

    /// Get the most recent N items from the buffer.
    /// Returns fewer items if the buffer contains less than N items.
    pub fn get_recent(&self, count: usize) -> Vec<&T> {
        let available = count.min(self.buffer.len());
        self.buffer
            .iter()
            .rev()
            .take(available)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// Get all items currently in the buffer, in order from oldest to newest.
    pub fn to_array(&self) -> Vec<&T> {
        self.buffer.iter().collect()
    }

    /// Get all items as owned values
    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.buffer.iter().cloned().collect()
    }

    /// Clear all items from the buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Get the current number of items in the buffer.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get the capacity of the buffer.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Check if the buffer is full.
    pub fn is_full(&self) -> bool {
        self.buffer.len() >= self.capacity
    }

    /// Get an iterator over the items in the buffer.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.buffer.iter()
    }
}

impl<T> Default for CircularBuffer<T> {
    fn default() -> Self {
        Self::new(16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_buffer() {
        let buffer: CircularBuffer<i32> = CircularBuffer::new(3);
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn test_add_single_item() {
        let mut buffer = CircularBuffer::new(3);
        buffer.add(1);
        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.to_vec(), vec![1]);
    }

    #[test]
    fn test_eviction() {
        let mut buffer = CircularBuffer::new(3);
        buffer.add(1);
        buffer.add(2);
        buffer.add(3);
        buffer.add(4); // Should evict 1

        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.to_vec(), vec![2, 3, 4]);
    }

    #[test]
    fn test_get_recent() {
        let mut buffer = CircularBuffer::new(5);
        buffer.add_all(vec![1, 2, 3, 4, 5]);
        let recent = buffer.get_recent(3);
        assert_eq!(recent, vec![&3, &4, &5]);
    }

    #[test]
    fn test_clear() {
        let mut buffer = CircularBuffer::new(3);
        buffer.add(1);
        buffer.add(2);
        buffer.clear();
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_is_full() {
        let mut buffer = CircularBuffer::new(2);
        assert!(!buffer.is_full());
        buffer.add(1);
        assert!(!buffer.is_full());
        buffer.add(2);
        assert!(buffer.is_full());
        buffer.add(3);
        // Still full, oldest evicted
        assert!(buffer.is_full());
    }
}
