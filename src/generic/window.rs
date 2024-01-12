use bytes::Bytes;

use crate::protocol::WINDOW_SIZE;

pub struct SequenceWindow {
    pub start: u32,
    pub end: u32,
    pub highest: u32,
    pub acks: Vec<u32>,
    pub nacks: Vec<u32>,
}

impl SequenceWindow {
    pub fn new() -> Self {
        Self {
            start: 0,
            end: WINDOW_SIZE,
            highest: 0,
            acks: Vec::new(),
            nacks: Vec::new(),
        }
    }

    pub fn receive(&mut self, seq: u32) -> bool {
        if seq < self.start || seq > self.end || self.acks.contains(&seq) {
            return false;
        }

        self.nacks.retain(|&x| x != seq);
        self.acks.push(seq);

        if seq > self.highest {
            self.highest = seq;
        }

        if seq == self.start {
            for i in self.start..self.end {
                if !self.acks.contains(&i) {
                    break;
                }

                self.start += 1;
                self.end += 1;
            }
        } else {
            for i in self.start..seq {
                if !self.acks.contains(&i) {
                    self.nacks.push(i);
                }
            }
        }

        true
    }

    pub fn shift(&mut self) {
        self.start += self.highest + 1;
        self.end += self.highest + 1;
    }
}

pub struct MessageWindow {
    pub start: u32,
    pub end: u32,
    pub indexes: Vec<u32>,
}

impl MessageWindow {
    pub fn new() -> Self {
        Self {
            start: 0,
            end: WINDOW_SIZE,
            indexes: Vec::new(),
        }
    }

    pub fn receive(&mut self, index: u32) -> bool {
        if index < self.start || index > self.end || self.indexes.contains(&index) {
            return false;
        }

        self.indexes.push(index);

        if index == self.start {
            for i in self.start..self.end {
                if !self.indexes.contains(&i) {
                    break;
                }

                self.indexes.retain(|&x| x != i);
                self.start += 1;
                self.end += 1;
            }
        }

        true
    }
}

pub struct SplitWindow {
    pub count: u32,
    pub fragments: Vec<Vec<u8>>,
}

impl SplitWindow {
    pub fn new(count: u32) -> Self {
        Self {
            count,
            fragments: Vec::with_capacity(count as usize),
        }
    }

    pub fn receive(&mut self, index: u32, fragment: Vec<u8>) -> Option<Bytes> {
        self.fragments.insert(index as usize, fragment);

        if self.fragments.capacity() != self.fragments.len() {
            return None;
        }

        let mut buffer = self.fragments.remove(0);

        for i in 1..self.fragments.len() {
            let fragment = self.fragments.remove(i);
            buffer.extend_from_slice(&fragment);
        }

        Some(Bytes::from(buffer))
    }
}
