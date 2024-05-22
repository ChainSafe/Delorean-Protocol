// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use futures::{AsyncWrite, Future};
use std::io::{Error as IoError, Result as IoResult};
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_util::compat::TokioAsyncWriteCompatExt;

type BoxedFutureFile = Pin<Box<dyn Future<Output = IoResult<tokio::fs::File>> + Send + 'static>>;
type BoxedFile = Pin<Box<tokio_util::compat::Compat<tokio::fs::File>>>;
type StatePoll<T> = (ChunkWriterState, Poll<IoResult<T>>);

enum ChunkWriterState {
    Idle,
    Opening { out: BoxedFutureFile },
    Open { out: BoxedFile, written: usize },
    Closing { out: BoxedFile },
}

impl ChunkWriterState {
    fn ok<T>(self, value: T) -> StatePoll<T> {
        (self, Poll::Ready(Ok(value)))
    }

    fn err<T>(self, err: IoError) -> StatePoll<T> {
        (self, Poll::Ready(Err(err)))
    }

    fn pending<T>(self) -> StatePoll<T> {
        (self, Poll::Pending)
    }
}

/// Write a CAR file to chunks under an output directory:
/// 1. the first chunk is assumed to be just the header and goes into its own file
/// 2. subsequent blocks are assumed to be the contents and go into files with limited size
pub struct ChunkWriter {
    output_dir: PathBuf,
    max_size: usize,
    file_name: Box<dyn Fn(usize) -> String + Send + Sync>,
    next_idx: usize,
    state: ChunkWriterState,
}

impl ChunkWriter {
    pub fn new<F>(output_dir: PathBuf, max_size: usize, file_name: F) -> Self
    where
        F: Fn(usize) -> String + Send + Sync + 'static,
    {
        Self {
            output_dir,
            max_size,
            file_name: Box::new(file_name),
            next_idx: 0,
            state: ChunkWriterState::Idle,
        }
    }

    /// Number of chunks created so far.
    pub fn chunk_created(&self) -> usize {
        self.next_idx
    }

    fn take_state(&mut self) -> ChunkWriterState {
        let mut state = ChunkWriterState::Idle;
        std::mem::swap(&mut self.state, &mut state);
        state
    }

    /// Replace the state with a new one, returning the poll result.
    fn poll_state<F, T>(self: &mut Pin<&mut Self>, f: F) -> Poll<IoResult<T>>
    where
        F: FnOnce(&mut Pin<&mut Self>, ChunkWriterState) -> StatePoll<T>,
    {
        let state = self.take_state();
        let (state, poll) = f(self, state);
        self.state = state;
        poll
    }

    /// Open the file, then do something with it.
    fn state_poll_open<F, T>(cx: &mut Context<'_>, mut out: BoxedFutureFile, f: F) -> StatePoll<T>
    where
        F: FnOnce(&mut Context<'_>, BoxedFile) -> StatePoll<T>,
    {
        use ChunkWriterState::*;

        match out.as_mut().poll(cx) {
            Poll::Pending => Opening { out }.pending(),
            Poll::Ready(Err(e)) => Idle.err(e),
            Poll::Ready(Ok(out)) => {
                let out = Box::pin(out.compat_write());
                f(cx, out)
            }
        }
    }

    /// Write to the open file.
    fn state_poll_write(
        cx: &mut Context<'_>,
        buf: &[u8],
        mut out: BoxedFile,
        sofar: usize,
    ) -> StatePoll<usize> {
        use ChunkWriterState::*;

        match out.as_mut().poll_write(cx, buf) {
            Poll::Pending => Open {
                out,
                written: sofar,
            }
            .pending(),
            Poll::Ready(Ok(written)) => Open {
                out,
                written: sofar + written,
            }
            .ok(written),
            Poll::Ready(Err(e)) => Open {
                out,
                written: sofar,
            }
            .err(e),
        }
    }

    /// Close the file.
    fn state_poll_close(cx: &mut Context<'_>, mut out: BoxedFile) -> StatePoll<()> {
        use ChunkWriterState::*;

        match out.as_mut().poll_close(cx) {
            Poll::Pending => Closing { out }.pending(),
            Poll::Ready(Err(e)) => Idle.err(e),
            Poll::Ready(Ok(())) => Idle.ok(()),
        }
    }

    /// Open the file then write to it.
    fn state_poll_open_write(
        cx: &mut Context<'_>,
        buf: &[u8],
        out: BoxedFutureFile,
    ) -> StatePoll<usize> {
        Self::state_poll_open(cx, out, |cx, out| Self::state_poll_write(cx, buf, out, 0))
    }

    /// Open the next file, then increment the index.
    fn next_file(&mut self) -> BoxedFutureFile {
        let name = (self.file_name)(self.next_idx);
        let out = self.output_dir.join(name);
        self.next_idx += 1;
        Box::pin(tokio::fs::File::create(out))
    }
}

impl AsyncWrite for ChunkWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<IoResult<usize>> {
        use ChunkWriterState::*;

        self.poll_state(|this, state| match state {
            Idle => Self::state_poll_open_write(cx, buf, this.next_file()),
            Opening { out } => Self::state_poll_open_write(cx, buf, out),
            Open { out, written } => Self::state_poll_write(cx, buf, out, written),
            Closing { out } => {
                let (state, poll) = Self::state_poll_close(cx, out);
                if poll.is_ready() {
                    Self::state_poll_open_write(cx, buf, this.next_file())
                } else {
                    state.pending()
                }
            }
        })
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        use ChunkWriterState::*;

        self.poll_state(|this, state| match state {
            Idle => state.ok(()),
            Opening { out } => {
                // When we just opened this file, there is nothing to flush.
                Self::state_poll_open(cx, out, |_cx: &mut Context<'_>, out| {
                    Open { out, written: 0 }.ok(())
                })
            }
            Open { mut out, written } => match out.as_mut().poll_flush(cx) {
                Poll::Pending => Open { out, written }.pending(),
                Poll::Ready(Err(e)) => Open { out, written }.err(e),
                Poll::Ready(Ok(())) => {
                    // Close the file if either:
                    // a) we have written the header, or
                    // b) we exceeded the maximum file size.
                    // The flush is ensured by `fvm_ipld_car::util::ld_write` called by `CarHeader::write_stream_async` with the header.
                    // The file is closed here not in `poll_write` so we don't have torn writes where the varint showing the size is split from the data.
                    let close = this.next_idx == 1 || written >= this.max_size && this.max_size > 0;
                    if close {
                        Self::state_poll_close(cx, out)
                    } else {
                        Open { out, written }.ok(())
                    }
                }
            },
            Closing { out } => Self::state_poll_close(cx, out),
        })
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        use ChunkWriterState::*;

        self.poll_state(|_, state| match state {
            Idle => state.ok(()),
            Opening { out } => Self::state_poll_open(cx, out, Self::state_poll_close),
            Open { out, .. } => Self::state_poll_close(cx, out),
            Closing { out } => Self::state_poll_close(cx, out),
        })
    }
}
