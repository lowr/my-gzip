use std::io::{Read, Result};

pub struct RingBuffer<T> {
    buf: Vec<T>,
    next: usize,
}

impl<T> RingBuffer<T> {
    pub fn new(size: usize) -> Self {
        assert!(size > 0);

        Self {
            buf: Vec::with_capacity(size),
            next: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.buf.capacity()
    }

    pub fn len(&self) -> usize {
        if self.is_wrapped() {
            self.capacity()
        } else {
            self.next
        }
    }

    pub fn push(&mut self, value: T) {
        if self.is_wrapped() {
            self.buf[self.next] = value;
        } else {
            self.buf.push(value);
        }
        self.next += 1;
        if self.next == self.capacity() {
            self.next = 0;
        }
    }

    #[allow(unused)]
    pub fn as_slices(&self) -> (&[T], &[T]) {
        if self.is_wrapped() {
            (&self.buf[self.next..], &self.buf[..self.next])
        } else {
            (&self.buf[..self.next], &[])
        }
    }

    #[allow(unused)]
    pub fn as_mut_slices(&mut self) -> (&mut [T], &mut [T]) {
        if self.is_wrapped() {
            let (first, second) = self.buf.split_at_mut(self.next);
            (second, first)
        } else {
            (&mut self.buf[..self.next], &mut [])
        }
    }

    pub fn is_wrapped(&self) -> bool {
        self.buf.len() == self.buf.capacity()
    }
}

// We don't aim for general purpose container, so we won't provide impl<T> where
// T: Clone.
impl<T> RingBuffer<T>
where
    T: Copy,
{
    // TODO: current implementation is simple but apparently not performant. Can we
    //       improve it using `slice::copy_within()` and such?
    pub fn copy_within(&mut self, distance: usize, length: usize) -> (&[T], &[T]) {
        assert!(distance > 0, "distance must not be 0");
        assert!(
            self.is_wrapped() || distance <= self.next,
            "distance too long for current buffer; current buffered length = {}, given distance = {}",
            self.next,
            distance,
        );
        assert!(
            length <= self.capacity(),
            "specified length is longer than ringbuffer's capacity; capacity = {}, given length = {}",
            self.capacity(),
            length,
        );

        let cap = self.capacity();
        let start = self.next + cap - distance;

        // TODO: reconsider when `self.next == start`

        let elements_to_be_pushed = std::cmp::min(length, cap - self.len());

        for i in 0..elements_to_be_pushed {
            // when `!self.is_wrapped()`, `distance <= self.next()` holds and thus
            // `start > cap`
            self.buf.push(self.buf[start - cap + i]);
        }

        for i in elements_to_be_pushed..length {
            self.buf[(self.next + i) % cap] = self.buf[(start + i) % cap];
        }

        let old_next = self.next;
        self.next = (self.next + length) % cap;

        if self.next <= old_next {
            // wrapped; returning 2 slices
            (&self.buf[old_next..], &self.buf[..self.next])
        } else {
            // contiguous; returning the slice and an empty one
            (&self.buf[old_next..self.next], &[])
        }
    }
}

impl RingBuffer<u8> {
    pub fn copy_from<R>(&mut self, reader: &mut R, mut length: usize) -> Result<(&[u8], &[u8])>
    where
        R: Read,
    {
        assert!(
            length <= self.capacity(),
            "specified length is longer than ringbuffer's capacity; capacity = {}, given length = {}",
            self.capacity(),
            length,
        );

        if length == 0 {
            return Ok((&[][..], &[][..]));
        }

        let cap = self.capacity();
        let old_next = self.next;
        debug_assert!(length <= cap);

        if !self.is_wrapped() {
            let elements_to_be_pushed = std::cmp::min(length, cap - self.next);
            let mut buf = vec![0; elements_to_be_pushed];
            reader.read_exact(&mut buf)?;
            self.buf.extend_from_slice(&buf);
            self.next = (self.next + elements_to_be_pushed) % cap;
            length -= elements_to_be_pushed;
        }

        if length > 0 {
            debug_assert!(self.is_wrapped());
            let (second, first) = self.buf.split_at_mut(self.next);

            if length <= first.len() {
                reader.read_exact(&mut first[..length])?;
            } else {
                let remainder = length - first.len();
                reader.read_exact(first)?;
                reader.read_exact(&mut second[..remainder])?;
            }

            self.next = (self.next + length) % cap;
        }

        if self.next <= old_next {
            // wrapped; returning 2 slices
            Ok((&self.buf[old_next..], &self.buf[..self.next]))
        } else {
            // contiguous; returning the slice and an empty one
            Ok((&self.buf[old_next..self.next], &[]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_capacity() {
        let cap = 10;
        let rb = RingBuffer::<u8>::new(cap);
        assert_eq!(rb.capacity(), cap);
    }

    #[test]
    fn push_does_not_expand_capacity() {
        let cap = 10;
        let mut rb = RingBuffer::<u8>::new(cap);

        for i in 0..20 {
            rb.push(i);
            assert_eq!(rb.capacity(), cap);
        }
    }

    #[test]
    fn push_overwrites_when_wrapped() {
        let mut rb = RingBuffer::<u8>::new(10);

        for i in 0..10 {
            rb.push(i);
        }

        // current state of buffer
        // [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
        //  ^
        //  next
        assert_eq!(
            rb.as_slices(),
            (&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9][..], &[][..]),
        );

        for i in 10..13 {
            rb.push(i);
        }

        // current state of buffer
        // [10, 11, 12, 3, 4, 5, 6, 7, 8, 9]
        //              ^
        //              next
        assert_eq!(
            rb.as_slices(),
            (&[3, 4, 5, 6, 7, 8, 9][..], &[10, 11, 12][..]),
        );
    }

    // tests for `copy_within()` are written as thoroughly as possible to
    // facilitate refactoring or even reimplementation.
    #[test]
    fn copy_within_works_when_buffer_is_not_fully_filled() {
        fn setup() -> RingBuffer<u8> {
            let mut rb = RingBuffer::new(10);
            for i in 0..5 {
                rb.push(i);
            }
            rb
        }

        // `u` represents uninitialized region of buffer

        // original state of buffer
        // [0, 1, 2, 3, 4, u, u, u, u, u]
        //                 ^
        //                 next

        let mut rb = setup();

        let copied = rb.copy_within(4, 2);
        // [0, 1, 2, 3, 4, u, u, u, u, u]
        //     ~~~~
        //                 ^^^^
        //      copy from here
        //                 to here
        assert_eq!(copied, (&[1, 2][..], &[][..]));

        // current state of buffer
        // [0, 1, 2, 3, 4, 1, 2, u, u, u]
        //                       ^
        //                       next
        assert_eq!(rb.as_slices(), (&[0, 1, 2, 3, 4, 1, 2][..], &[][..]));

        let mut rb = setup();

        let copied = rb.copy_within(3, 4);
        // [0, 1, 2, 3, 4, u, u, u, u, u]
        //        ~~~~~~~~~~
        //                 ^^^^^^^^^^
        //        copy from here
        //                 to here
        assert_eq!(copied, (&[2, 3, 4, 2][..], &[][..]));

        // current state of buffer
        // [0, 1, 2, 3, 4, 2, 3, 4, 2, u]
        //                             ^
        //                             next
        assert_eq!(rb.as_slices(), (&[0, 1, 2, 3, 4, 2, 3, 4, 2][..], &[][..]));

        let mut rb = setup();

        let copied = rb.copy_within(3, 7);
        // [0, 1, 2, 3, 4, u, u, u, u, u]
        //        ~~~~~~~~~~~~~~~~~~~
        //  ^^^^           ^^^^^^^^^^^^^
        //        copy from here
        //                 to here (wraps)
        assert_eq!(copied, (&[2, 3, 4, 2, 3][..], &[4, 2][..]));

        // current state of buffer
        // [4, 2, 2, 3, 4, 2, 3, 4, 2, 3]
        //        ^
        //        next
        assert_eq!(rb.as_slices(), (&[2, 3, 4, 2, 3, 4, 2, 3][..], &[4, 2][..]));

        let mut rb = setup();

        let copied = rb.copy_within(2, 8);
        // [0, 1, 2, 3, 4, u, u, u, u, u]
        //  ~        ~~~~~~~~~~~~~~~~~~~
        //  ^^^^^^^        ^^^^^^^^^^^^^
        //           copy from here (wraps)
        //                 to here (wraps)
        assert_eq!(copied, (&[3, 4, 3, 4, 3][..], &[4, 3, 4][..]));

        // current state of buffer
        // [4, 3, 4, 3, 4, 3, 4, 3, 4, 3]
        //           ^
        //           next
        assert_eq!(rb.as_slices(), (&[3, 4, 3, 4, 3, 4, 3][..], &[4, 3, 4][..]));
    }

    #[test]
    fn copy_within_works_when_src_and_dest_do_not_overlap() {
        let mut rb = RingBuffer::<u8>::new(10);

        for i in 0..15 {
            rb.push(i);
        }

        /* 1. when neither `src` nor `dest` wraps */

        // current state of buffer
        // [10, 11, 12, 13, 14, 5, 6, 7, 8, 9]
        //                      ^
        //                      next

        let copied = rb.copy_within(4, 3);
        // [10, 11, 12, 13, 14, 5, 6, 7, 8, 9]
        //      ~~~~~~~~~~      ^^^^^^^
        //      copy from here
        //                      to here
        assert_eq!(copied, (&[11, 12, 13][..], &[][..]));

        // current state of buffer
        // [10, 11, 12, 13, 14, 11, 12, 13, 8, 9]
        //                                  ^
        //                                  next
        assert_eq!(
            rb.as_slices(),
            (&[8, 9][..], &[10, 11, 12, 13, 14, 11, 12, 13][..]),
        );

        /* when 2. `dest` wraps */

        let copied = rb.copy_within(4, 3);
        // [10, 11, 12, 13, 14, 11, 12, 13, 8, 9]
        //  ^^              ~~~~~~~~~~      ^^^^
        //                  copy from here
        //                                  to here (wraps)
        assert_eq!(copied, (&[14, 11][..], &[12][..]));

        // current state of buffer
        // [12, 11, 12, 13, 14, 11, 12, 13, 14, 11]
        //      ^^
        //      next
        assert_eq!(
            rb.as_slices(),
            (&[11, 12, 13, 14, 11, 12, 13, 14, 11][..], &[12][..]),
        );

        /* when 3. `src` wraps */

        let copied = rb.copy_within(3, 3);
        // [10, 11, 12, 13, 14, 11, 12, 13, 8, 9]
        //  ~~                              ~~~~
        //      ^^^^^^^^^^
        //                                  copy from here (wraps)
        //      to here
        assert_eq!(copied, (&[14, 11, 12][..], &[][..]));

        // current state of buffer
        // [12, 14, 11, 12, 14, 11, 12, 13, 14, 11]
        //                  ^^
        //                  next
        assert_eq!(
            rb.as_slices(),
            (&[14, 11, 12, 13, 14, 11][..], &[12, 14, 11, 12][..]),
        );
    }

    #[test]
    fn copy_within_works_when_src_and_dest_overlap() {
        fn setup() -> RingBuffer<u8> {
            let mut rb = RingBuffer::new(10);
            for i in 0..15 {
                rb.push(i);
            }
            rb
        }

        // original state of buffer
        // [10, 11, 12, 13, 14, 5, 6, 7, 8, 9]
        //                      ^
        //                      next

        let mut rb = setup();

        let copied = rb.copy_within(2, 4);
        // [10, 11, 12, 13, 14, 5, 6, 7, 8, 9]
        //              ~~~~~~~~~~~~
        //                      ^^^^^^^^^^
        //              copy from here
        //                      to here
        assert_eq!(copied, (&[13, 14, 13, 14][..], &[][..]));

        // current state of buffer
        // [10, 11, 12, 13, 14, 13, 14, 13, 14, 9]
        //                                      ^
        //                                      next
        assert_eq!(
            rb.as_slices(),
            (&[9][..], &[10, 11, 12, 13, 14, 13, 14, 13, 14][..]),
        );

        let mut rb = setup();

        let copied = rb.copy_within(8, 3);
        // [10, 11, 12, 13, 14, 5, 6, 7, 8, 9]
        //                            ~~~~~~~
        //                      ^^^^^^^
        //                            copy from here
        //                      to here
        assert_eq!(copied, (&[7, 8, 9][..], &[][..]));

        // current state of buffer
        // [10, 11, 12, 13, 14, 7, 8, 9, 8, 9]
        //                               ^
        //                               next
        assert_eq!(
            rb.as_slices(),
            (&[8, 9][..], &[10, 11, 12, 13, 14, 7, 8, 9][..]),
        );

        let mut rb = setup();

        let copied = rb.copy_within(7, 4);
        // [10, 11, 12, 13, 14, 5, 6, 7, 8, 9]
        //  ~~~~~~                       ~~~~
        //                      ^^^^^^^^^^
        //                               copy from here (wraps)
        //                      to here
        assert_eq!(copied, (&[8, 9, 10, 11][..], &[][..]));

        // current state of buffer
        // [10, 11, 12, 13, 14, 8, 9, 10, 11, 9]
        //                                    ^
        //                                    next
        assert_eq!(
            rb.as_slices(),
            (&[9][..], &[10, 11, 12, 13, 14, 8, 9, 10, 11][..]),
        );

        let mut rb = setup();
        rb.next = 2;

        let copied = rb.copy_within(3, 4);
        // [10, 11, 12, 13, 14, 5, 6, 7, 8, 9]
        //  ~~~~~~~~~~                      ~
        //          ^^^^^^^^^^^^^
        //                                  copy from here (wraps)
        //          to here
        assert_eq!(copied, (&[9, 10, 11, 9][..], &[][..]));

        // current state of buffer
        // [10, 11, 9, 10, 11, 9, 6, 7, 8, 9]
        //                        ^
        //                        next
        assert_eq!(
            rb.as_slices(),
            (&[6, 7, 8, 9][..], &[10, 11, 9, 10, 11, 9][..]),
        );

        let mut rb = setup();
        rb.next = 8;

        let copied = rb.copy_within(7, 4);
        // [10, 11, 12, 13, 14, 5, 6, 7, 8, 9]
        //      ~~~~~~~~~~~~~~
        //  ^^^^^^                       ^^^^
        //      copy from here
        //                               to here (wraps)
        assert_eq!(copied, (&[11, 12][..], &[13, 14][..]));

        // current state of buffer
        // [13, 14, 12, 13, 14, 5, 6, 7, 11, 12]
        //          ^^
        //          next
        assert_eq!(
            rb.as_slices(),
            (&[12, 13, 14, 5, 6, 7, 11, 12][..], &[13, 14][..]),
        );

        let mut rb = setup();
        rb.next = 8;

        let copied = rb.copy_within(2, 4);
        // [10, 11, 12, 13, 14, 5, 6, 7, 8, 9]
        //                         ~~~~~~~~~~
        //  ^^^^^^                       ^^^^
        //                         copy from here
        //                               to here (wraps)
        assert_eq!(copied, (&[6, 7][..], &[6, 7][..]));

        // current state of buffer
        // [6, 7, 12, 13, 14, 5, 6, 7, 6, 7]
        //        ^^
        //        next
        assert_eq!(
            rb.as_slices(),
            (&[12, 13, 14, 5, 6, 7, 6, 7][..], &[6, 7][..]),
        );

        let mut rb = setup();
        rb.next = 7;

        let copied = rb.copy_within(9, 4);
        // [10, 11, 12, 13, 14, 5, 6, 7, 8, 9]
        //  ~~~~~~                       ~~~~
        //  ^^                        ^^^^^^^
        //                               copy from here (wraps)
        //                            to here (wraps)
        assert_eq!(copied, (&[8, 9, 10][..], &[11][..]));

        // current state of buffer
        // [11, 11, 12, 13, 14, 5, 6, 8, 9, 10]
        //      ^^
        //      next
        assert_eq!(
            rb.as_slices(),
            (&[11, 12, 13, 14, 5, 6, 8, 9, 10][..], &[11][..]),
        );

        let mut rb = setup();
        rb.next = 9;

        let copied = rb.copy_within(1, 4);
        // [10, 11, 12, 13, 14, 5, 6, 7, 8, 9]
        //  ~~~~~~                       ~~~~
        //  ^^^^^^^^^^                      ^
        //                               copy from here (wraps)
        //                                  to here (wraps)
        assert_eq!(copied, (&[8][..], &[8, 8, 8][..]));

        // current state of buffer
        // [8, 8, 8, 13, 14, 5, 6, 7, 8, 8]
        //           ^^
        //           next
        assert_eq!(
            rb.as_slices(),
            (&[13, 14, 5, 6, 7, 8, 8][..], &[8, 8, 8][..]),
        );
    }

    #[test]
    fn copy_within_works_when_distance_equals_to_capacity() {
        let cap = 10;
        let mut rb = RingBuffer::<u8>::new(cap);

        for i in 0..15 {
            rb.push(i);
        }

        let buf = [10, 11, 12, 13, 14, 5, 6, 7, 8, 9];

        // current state of buffer
        // [10, 11, 12, 13, 14, 5, 6, 7, 8, 9]
        //                      ^
        //                      next

        let copied = rb.copy_within(cap, 3);
        assert_eq!(copied, (&[5, 6, 7][..], &[][..]));

        // current state of buffer
        // [10, 11, 12, 13, 14, 5, 6, 7, 8, 9]
        //                               ^
        //                               next
        assert_eq!(rb.as_slices(), (&buf[8..], &buf[..8]));

        let copied = rb.copy_within(cap, 3);
        assert_eq!(copied, (&[8, 9][..], &[10][..]));

        // current state of buffer
        // [10, 11, 12, 13, 14, 5, 6, 7, 8, 9]
        //      ^^
        //      next
        assert_eq!(rb.as_slices(), (&buf[1..], &buf[..1]));

        let copied = rb.copy_within(cap, cap);
        assert_eq!(copied, (&buf[1..], &buf[..1]));

        // current state of buffer
        // [10, 11, 12, 13, 14, 5, 6, 7, 8, 9]
        //      ^^
        //      next
        assert_eq!(rb.as_slices(), (&buf[1..], &buf[..1]));
    }

    #[test]
    fn copy_from_works_when_buffer_is_not_fully_filled() {
        fn setup() -> RingBuffer<u8> {
            let mut rb = RingBuffer::new(10);
            for i in 0..5 {
                rb.push(i);
            }
            rb
        }

        let buf = [20, 21, 22, 23, 24, 25, 26, 27, 28, 29];

        let mut rb = setup();

        let ret = rb.copy_from(&mut &buf[..], 0);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap(), (&[][..], &[][..]));
        assert_eq!(rb.as_slices(), (&[0, 1, 2, 3, 4][..], &[][..]));

        let mut rb = setup();

        let ret = rb.copy_from(&mut &buf[..], 3);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap(), (&[20, 21, 22][..], &[][..]));
        assert_eq!(rb.as_slices(), (&[0, 1, 2, 3, 4, 20, 21, 22][..], &[][..]));

        let mut rb = setup();

        let ret = rb.copy_from(&mut &buf[..], 6);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap(), (&[20, 21, 22, 23, 24][..], &[25][..]));
        assert_eq!(
            rb.as_slices(),
            (&[1, 2, 3, 4, 20, 21, 22, 23, 24][..], &[25][..]),
        );

        let mut rb = setup();

        let ret = rb.copy_from(&mut &buf[..], 10);
        assert!(ret.is_ok());
        assert_eq!(
            ret.unwrap(),
            (&[20, 21, 22, 23, 24][..], &[25, 26, 27, 28, 29][..]),
        );
        assert_eq!(
            rb.as_slices(),
            (&[20, 21, 22, 23, 24][..], &[25, 26, 27, 28, 29][..]),
        );
    }

    #[test]
    fn copy_from_works_when_wrapped() {
        fn setup() -> RingBuffer<u8> {
            let mut rb = RingBuffer::new(10);
            for i in 0..15 {
                rb.push(i);
            }
            rb
        }

        let buf = [20, 21, 22, 23, 24, 25, 26, 27, 28, 29];

        let mut rb = setup();

        let ret = rb.copy_from(&mut &buf[..], 0);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap(), (&[][..], &[][..]));
        assert_eq!(
            rb.as_slices(),
            (&[5, 6, 7, 8, 9][..], &[10, 11, 12, 13, 14][..]),
        );

        let mut rb = setup();

        let ret = rb.copy_from(&mut &buf[..], 3);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap(), (&[20, 21, 22][..], &[][..]));
        assert_eq!(
            rb.as_slices(),
            (&[8, 9][..], &[10, 11, 12, 13, 14, 20, 21, 22][..]),
        );

        let mut rb = setup();

        let ret = rb.copy_from(&mut &buf[..], 6);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap(), (&[20, 21, 22, 23, 24][..], &[25][..]));
        assert_eq!(
            rb.as_slices(),
            (&[11, 12, 13, 14, 20, 21, 22, 23, 24][..], &[25][..]),
        );

        let mut rb = setup();

        let ret = rb.copy_from(&mut &buf[..], 10);
        assert!(ret.is_ok());
        assert_eq!(
            ret.unwrap(),
            (&[20, 21, 22, 23, 24][..], &[25, 26, 27, 28, 29][..]),
        );
        assert_eq!(
            rb.as_slices(),
            (&[20, 21, 22, 23, 24][..], &[25, 26, 27, 28, 29][..]),
        );
    }

    #[test]
    fn copy_from_returns_error_when_reader_cannot_read_length_bytes() {
        fn setup() -> RingBuffer<u8> {
            let mut rb = RingBuffer::new(10);
            for i in 0..15 {
                rb.push(i);
            }
            rb
        }

        let buf = [20, 21, 22, 23, 24, 25, 26, 27, 28, 29];

        let mut rb = setup();

        let ret = rb.copy_from(&mut &buf[..3], 5);
        assert!(ret.is_err());
    }
}
