//! 环形缓冲区
//!
//! 固定大小的循环缓冲区，当缓冲区满时自动驱逐最旧的元素。
//! 适用于维护滚动数据窗口。

/// 固定大小的循环缓冲区，当缓冲区满时自动驱逐最旧的元素。
pub struct CircularBuffer<T> {
    buffer: Vec<T>,
    head: usize,
    size: usize,
    capacity: usize,
}

impl<T> CircularBuffer<T> {
    /// 创建指定容量的新缓冲区。
    pub fn new(capacity: usize) -> Self {
        CircularBuffer {
            buffer: Vec::with_capacity(capacity),
            head: 0,
            size: 0,
            capacity,
        }
    }

    /// 向缓冲区添加元素。如果缓冲区已满，最旧的元素将被驱逐。
    pub fn add(&mut self, item: T) {
        if self.size < self.capacity {
            self.buffer.push(item);
        } else {
            self.buffer[self.head] = item;
        }
        self.head = (self.head + 1) % self.capacity;
        if self.size < self.capacity {
            self.size += 1;
        }
    }

    /// 批量添加元素到缓冲区。
    pub fn add_all(&mut self, items: impl IntoIterator<Item = T>) {
        for item in items {
            self.add(item);
        }
    }

    /// 获取最近 N 个元素。如果缓冲区包含少于 N 个元素，返回较少元素。
    pub fn get_recent(&self, count: usize) -> Vec<&T> {
        if self.size == 0 {
            return vec![];
        }

        let start = if self.size < self.capacity { 0 } else { self.head };
        let available = count.min(self.size);
        let mut result = Vec::with_capacity(available);

        for i in 0..available {
            let index = (start + self.size - available + i) % self.capacity;
            result.push(&self.buffer[index]);
        }

        result
    }

    /// 获取缓冲区中的所有元素，按从旧到新的顺序。
    pub fn to_array(&self) -> Vec<&T> {
        if self.size == 0 {
            return vec![];
        }

        let start = if self.size < self.capacity { 0 } else { self.head };
        let mut result = Vec::with_capacity(self.size);

        for i in 0..self.size {
            let index = (start + i) % self.capacity;
            result.push(&self.buffer[index]);
        }

        result
    }

    /// 清空缓冲区中的所有元素。
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.head = 0;
        self.size = 0;
    }

    /// 获取缓冲区中当前元素的数量。
    pub fn len(&self) -> usize {
        self.size
    }

    /// 检查缓冲区是否为空。
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
}

impl<T: Clone> CircularBuffer<T> {
    /// 获取最近 N 个元素的克隆副本。
    pub fn get_recent_cloned(&self, count: usize) -> Vec<T> {
        self.get_recent(count).iter().cloned().collect()
    }

    /// 获取所有元素的克隆副本。
    pub fn to_array_cloned(&self) -> Vec<T> {
        self.to_array().iter().cloned().collect()
    }
}
