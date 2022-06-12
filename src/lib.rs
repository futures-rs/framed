//! The futures adaptors from [`AsyncRead`]/[`AsyncWrite`] to [`Stream`]/[`Sink`]
//!
//! This lib forked from [`tokio_util`], modified to support futures library
//!
//! [`AsyncRead`]: futures::AsyncRead
//! [`AsyncWrite`]: futures::AsyncWrite
//! [`Stream`]: futures::Stream
//! [`Sink`]: futures::Sink
//! [`tokio_util/codec`]: https://docs.rs/tokio-util/latest/tokio_util/codec/index.html

pub mod codec;
pub mod framed;
mod util;

pub use codec::*;
pub use framed::*;

mod length_delimited;

pub use length_delimited::*;

mod bytes_codec;

pub use bytes_codec::*;

mod lines_codec;

pub use lines_codec::*;

mod any_delimiter_codec;

pub use any_delimiter_codec::*;
