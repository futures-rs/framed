use bytes::{Buf, BufMut};
use futures::{ready, AsyncRead, AsyncWrite};
use std::fmt;
use std::io;
use std::io::IoSlice;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A wrapper around a byte buffer that is incrementally filled and initialized.
///
/// This type is a sort of "double cursor". It tracks three regions in the
/// buffer: a region at the beginning of the buffer that has been logically
/// filled with data, a region that has been initialized at some point but not
/// yet logically filled, and a region at the end that may be uninitialized.
/// The filled region is guaranteed to be a subset of the initialized region.
///
/// In summary, the contents of the buffer can be visualized as:
///
/// ```not_rust
/// [             capacity              ]
/// [ filled |         unfilled         ]
/// [    initialized    | uninitialized ]
/// ```
///
/// It is undefined behavior to de-initialize any bytes from the uninitialized
/// region, since it is merely unknown whether this region is uninitialized or
/// not, and if part of it turns out to be initialized, it must stay initialized.
pub struct ReadBuf<'a> {
    buf: &'a mut [MaybeUninit<u8>],
    filled: usize,
    initialized: usize,
}

impl<'a> ReadBuf<'a> {
    /// Creates a new `ReadBuf` from a fully initialized buffer.

    /// Creates a new `ReadBuf` from a fully uninitialized buffer.
    ///
    /// Use `assume_init` if part of the buffer is known to be already initialized.
    #[inline]
    pub fn uninit(buf: &'a mut [MaybeUninit<u8>]) -> ReadBuf<'a> {
        ReadBuf {
            buf,
            filled: 0,
            initialized: 0,
        }
    }

    /// Returns the total capacity of the buffer.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    /// Returns a shared reference to the filled portion of the buffer.
    #[inline]
    pub fn filled(&self) -> &[u8] {
        let slice = &self.buf[..self.filled];
        // safety: filled describes how far into the buffer that the
        // user has filled with bytes, so it's been initialized.
        unsafe { slice_assume_init(slice) }
    }

    /// Returns a mutable reference to the unfilled part of the buffer, ensuring it is fully initialized.
    ///
    /// Since `ReadBuf` tracks the region of the buffer that has been initialized, this is effectively "free" after
    /// the first use.
    #[inline]
    pub fn initialize_unfilled(&mut self) -> &mut [u8] {
        self.initialize_unfilled_to(self.remaining())
    }

    /// Returns a mutable reference to the first `n` bytes of the unfilled part of the buffer, ensuring it is
    /// fully initialized.
    ///
    /// # Panics
    ///
    /// Panics if `self.remaining()` is less than `n`.
    #[inline]
    pub fn initialize_unfilled_to(&mut self, n: usize) -> &mut [u8] {
        assert!(self.remaining() >= n, "n overflows remaining");

        // This can't overflow, otherwise the assert above would have failed.
        let end = self.filled + n;

        if self.initialized < end {
            unsafe {
                self.buf[self.initialized..end]
                    .as_mut_ptr()
                    .write_bytes(0, end - self.initialized);
            }
            self.initialized = end;
        }

        let slice = &mut self.buf[self.filled..end];
        // safety: just above, we checked that the end of the buf has
        // been initialized to some value.
        unsafe { slice_assume_init_mut(slice) }
    }

    /// Returns the number of bytes at the end of the slice that have not yet been filled.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.capacity() - self.filled
    }
}

impl fmt::Debug for ReadBuf<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadBuf")
            .field("filled", &self.filled)
            .field("initialized", &self.initialized)
            .field("capacity", &self.capacity())
            .finish()
    }
}

// TODO: This could use `MaybeUninit::slice_assume_init` when it is stable.
unsafe fn slice_assume_init(slice: &[MaybeUninit<u8>]) -> &[u8] {
    &*(slice as *const [MaybeUninit<u8>] as *const [u8])
}

// TODO: This could use `MaybeUninit::slice_assume_init_mut` when it is stable.
unsafe fn slice_assume_init_mut(slice: &mut [MaybeUninit<u8>]) -> &mut [u8] {
    &mut *(slice as *mut [MaybeUninit<u8>] as *mut [u8])
}

pub fn poll_read_buf<T: AsyncRead, B: BufMut>(
    io: Pin<&mut T>,
    cx: &mut Context<'_>,
    buf: &mut B,
) -> Poll<io::Result<usize>> {
    if !buf.has_remaining_mut() {
        return Poll::Ready(Ok(0));
    }

    let n = {
        let dst = buf.chunk_mut();

        // Safety: `chunk_mut()` returns a `&mut UninitSlice`, and `UninitSlice` is a
        // transparent wrapper around `[MaybeUninit<u8>]`.
        let dst = unsafe { &mut *(dst as *mut _ as *mut [MaybeUninit<u8>]) };
        let mut buf = ReadBuf::uninit(dst);
        let ptr = buf.filled().as_ptr();
        let mut_buf = buf.initialize_unfilled();

        let len = ready!(io.poll_read(cx, mut_buf)?);

        // Ensure the pointer does not change from under us
        assert_eq!(ptr, buf.filled().as_ptr());

        len
    };

    // Safety: This is guaranteed to be the number of initialized (and read)
    // bytes due to the invariants provided by `ReadBuf::filled`.
    unsafe {
        buf.advance_mut(n);
    }

    Poll::Ready(Ok(n))
}

pub fn poll_write_buf<T: AsyncWrite, B: Buf>(
    io: Pin<&mut T>,
    cx: &mut Context<'_>,
    buf: &mut B,
) -> Poll<io::Result<usize>> {
    const MAX_BUFS: usize = 64;

    if !buf.has_remaining() {
        return Poll::Ready(Ok(0));
    }

    let mut slices = [IoSlice::new(&[]); MAX_BUFS];
    let cnt = buf.chunks_vectored(&mut slices);
    let n = ready!(io.poll_write_vectored(cx, &slices[..cnt]))?;

    buf.advance(n);

    Poll::Ready(Ok(n))
}
