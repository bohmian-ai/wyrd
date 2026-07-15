//! Bounded source consumption shared by upload protocols.

use bytes::{Buf, Bytes, BytesMut};
use futures_util::stream::{BoxStream, StreamExt};

use crate::error::StorageClientError;

pub(crate) struct SourceReader {
    stream: BoxStream<'static, Result<Bytes, StorageClientError>>,
    pending: Bytes,
}

impl SourceReader {
    pub(crate) fn new(stream: BoxStream<'static, Result<Bytes, StorageClientError>>) -> Self {
        Self {
            stream,
            pending: Bytes::new(),
        }
    }

    pub(crate) async fn next_chunk(
        &mut self,
        max: usize,
    ) -> Result<Option<Bytes>, StorageClientError> {
        let mut output = BytesMut::with_capacity(max);
        while output.len() < max {
            if self.pending.has_remaining() {
                let take = (max - output.len()).min(self.pending.remaining());
                output.extend_from_slice(&self.pending.copy_to_bytes(take));
                continue;
            }
            let Some(next) = self.stream.next().await else {
                break;
            };
            self.pending = next?;
        }
        if output.is_empty() {
            Ok(None)
        } else {
            Ok(Some(output.freeze()))
        }
    }

    pub(crate) async fn has_more(&mut self) -> Result<bool, StorageClientError> {
        if self.pending.has_remaining() {
            return Ok(true);
        }
        match self.stream.next().await {
            Some(next) => {
                self.pending = next?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub(crate) fn stream(self) -> BoxStream<'static, Result<Bytes, StorageClientError>> {
        let Self { stream, pending } = self;
        futures_util::stream::once(async move { pending })
            .filter_map(|pending| async move {
                if pending.is_empty() {
                    None
                } else {
                    Some(Ok(pending))
                }
            })
            .chain(stream)
            .boxed()
    }
}
