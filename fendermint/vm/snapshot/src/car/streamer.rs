// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use futures::{AsyncRead, Future, Stream};
use std::pin::Pin;
use std::task::{Context, Poll};

use fvm_ipld_car::CarReader;
use fvm_ipld_car::Error as CarError;

type BlockStreamerItem = Result<(Cid, Vec<u8>), CarError>;
type BlockStreamerRead<R> = (CarReader<R>, Option<BlockStreamerItem>);
type BlockStreamerReadFuture<R> = Pin<Box<dyn Future<Output = BlockStreamerRead<R>> + Send>>;

enum BlockStreamerState<R> {
    Idle(CarReader<R>),
    Reading(BlockStreamerReadFuture<R>),
}

/// Stream the content blocks from a CAR reader.
pub struct BlockStreamer<R> {
    state: Option<BlockStreamerState<R>>,
}

impl<R> BlockStreamer<R>
where
    R: AsyncRead + Send + Unpin,
{
    pub fn new(reader: CarReader<R>) -> Self {
        Self {
            state: Some(BlockStreamerState::Idle(reader)),
        }
    }

    async fn next_block(mut reader: CarReader<R>) -> BlockStreamerRead<R> {
        let res = reader.next_block().await;
        let out = match res {
            Err(e) => Some(Err(e)),
            Ok(Some(b)) => Some(Ok((b.cid, b.data))),
            Ok(None) => None,
        };
        (reader, out)
    }

    fn poll_next_block(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut next_block: BlockStreamerReadFuture<R>,
    ) -> Poll<Option<BlockStreamerItem>> {
        use BlockStreamerState::*;

        match next_block.as_mut().poll(cx) {
            Poll::Pending => {
                self.state = Some(Reading(next_block));
                Poll::Pending
            }
            Poll::Ready((reader, out)) => {
                self.state = Some(Idle(reader));
                Poll::Ready(out)
            }
        }
    }
}

impl<R> Stream for BlockStreamer<R>
where
    R: AsyncRead + Send + Unpin + 'static,
{
    type Item = BlockStreamerItem;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use BlockStreamerState::*;

        match self.state.take() {
            None => Poll::Ready(None),
            Some(Idle(reader)) => {
                let next_block = Self::next_block(reader);
                let next_block = Box::pin(next_block);
                self.poll_next_block(cx, next_block)
            }
            Some(Reading(next_block)) => self.poll_next_block(cx, next_block),
        }
    }
}

#[cfg(test)]
mod tests {

    use fendermint_vm_interpreter::fvm::bundle::bundle_path;
    use futures::{AsyncRead, StreamExt};
    use fvm_ipld_blockstore::MemoryBlockstore;
    use fvm_ipld_car::{load_car, CarReader};
    use tokio_util::compat::TokioAsyncReadCompatExt;

    use super::BlockStreamer;

    async fn bundle_file() -> tokio_util::compat::Compat<tokio::fs::File> {
        let bundle_path = bundle_path();
        tokio::fs::File::open(bundle_path).await.unwrap().compat()
    }

    /// Check that a CAR file can be loaded from a byte reader.
    async fn check_load_car<R>(reader: R)
    where
        R: AsyncRead + Send + Unpin,
    {
        let store = MemoryBlockstore::new();
        load_car(&store, reader).await.expect("failed to load CAR");
    }

    /// Check that a CAR file can be streamed without errors.
    async fn check_block_streamer<R>(reader: R)
    where
        R: AsyncRead + Send + Unpin + 'static,
    {
        let reader = CarReader::new_unchecked(reader)
            .await
            .expect("failed to open CAR reader");

        let streamer = BlockStreamer::new(reader);

        streamer
            .for_each(|r| async move {
                r.expect("should be ok");
            })
            .await;
    }

    /// Sanity check that the test bundle can be loaded with the normal facilities from a file.
    #[tokio::test]
    async fn load_bundle_from_file() {
        let bundle_file = bundle_file().await;
        check_load_car(bundle_file).await;
    }

    #[tokio::test]
    async fn block_streamer_from_file() {
        let bundle_file = bundle_file().await;
        check_block_streamer(bundle_file).await;
    }
}
