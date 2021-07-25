use anyhow::{anyhow, Result};
use std::io::{Read, Write};

pub struct Reader<R> {
    reader: R,
    current: u8,
    pos: u8,
}

impl<R> Reader<R>
where
    R: Read,
{
    pub fn new(r: R) -> Self {
        Self {
            reader: r,
            current: 0,
            pos: 0,
        }
    }

    pub fn next_bit(&mut self) -> Result<bool> {
        if self.pos >= 8 {
            return Err(anyhow!("finished"));
        }

        let masked = self.current & (1 << self.pos);
        let bit = masked > 0;

        self.pos += 1;
        if self.pos >= 8 {
            self.read_next_byte()?;
        }

        Ok(bit)
    }

    // returns the next byte.
    // Note that this function disregards any remaining bits in the current byte
    // when current position isn't on the byte boundary.
    pub fn next_byte(&mut self) -> Result<u8> {
        self.ensure_byte_boundary()?;

        let byte = self.current;

        self.read_next_byte()?;

        Ok(byte)
    }

    pub fn ensure_byte_boundary(&mut self) -> Result<()> {
        if self.pos == 0 {
            return Ok(());
        }

        match self.read_next_byte()? {
            Some(_) => Ok(()),
            None => Err(anyhow!("no more bytes")),
        }
    }

    // reads from underlying reader to the given buffer; returns the bytes read.
    // Note that this function disregards any remaining bits in the current byte
    // when current position isn't on the byte boundary.
    pub fn copy_to<W>(&mut self, writer: &mut W, length: usize) -> Result<usize>
    where
        W: Write,
    {
        use std::io::ErrorKind;

        self.ensure_byte_boundary()?;

        let mut remain = length;
        let mut buf = [0; 8192];

        'outer: while remain > 0 {
            let buf = if remain < 8192 {
                &mut buf[..remain]
            } else {
                &mut buf
            };
            loop {
                match self.reader.read(buf) {
                    Ok(0) => break 'outer,
                    Ok(bytes) => {
                        writer.write_all(buf)?;
                        remain -= bytes;
                        break;
                    }
                    Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e.into()),
                }
            }
        }

        self.read_next_byte()?;

        Ok(length - remain)
    }

    pub fn skip(&mut self, length: usize) -> Result<usize> {
        self.ensure_byte_boundary()?;

        self.copy_to(&mut std::io::sink(), length)
    }

    fn read_next_byte(&mut self) -> std::io::Result<Option<()>> {
        use std::io::ErrorKind;

        // taken from `Iterator` impl for `std::io::Bytes`
        loop {
            return match self.reader.read(std::slice::from_mut(&mut self.current)) {
                Ok(0) => {
                    self.pos = 8;
                    Ok(None)
                }
                Ok(..) => {
                    self.pos = 0;
                    Ok(Some(()))
                }
                Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => Err(e),
            };
        }
    }
}

impl<R> Read for Reader<R>
where
    R: Read,
{
    // discards partially read `self.current` (i.e. when `self.pos > 0`)
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let amount = if self.pos == 0 {
            buf[0] = self.current;
            self.reader.read(&mut buf[1..])? + 1
        } else {
            self.reader.read(buf)?
        };

        self.read_next_byte()?;

        Ok(amount)
    }
}
