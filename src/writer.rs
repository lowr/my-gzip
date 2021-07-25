use crate::ring_buffer::RingBuffer;
use anyhow::Result;
use std::io::{Read, Write};

pub struct Writer<W> {
    writer: W,
    ringbuf: RingBuffer<u8>,
}

impl<W> Writer<W> {
    pub fn new(writer: W, buf_size: usize) -> Self {
        Self {
            writer,
            ringbuf: RingBuffer::new(buf_size),
        }
    }
}

impl<W> Writer<W>
where
    W: Write,
{
    pub fn copy_from<R>(&mut self, reader: &mut R, length: usize) -> Result<()>
    where
        R: Read,
    {
        let (first, second) = self.ringbuf.copy_from(reader, length)?;
        self.writer.write_all(first)?;
        self.writer.write_all(second)?;
        Ok(())
    }

    pub fn copy_within(&mut self, distance: usize, length: usize) -> Result<usize> {
        let (first, second) = self.ringbuf.copy_within(distance, length);
        self.writer.write_all(first)?;
        self.writer.write_all(second)?;
        Ok(first.len() + second.len())
    }

    pub fn push(&mut self, value: u8) -> Result<()> {
        self.ringbuf.push(value);
        self.writer.write_all(&[value])?;
        Ok(())
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}
