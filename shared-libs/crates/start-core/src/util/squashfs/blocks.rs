use std::io::{self, Read, Write};
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use async_compression::codecs::{Decode, Encode, ZstdDecoder, ZstdEncoder};
use async_compression::core::util::PartialBuffer;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub const METADATA_SIZE: usize = 8192;
const UNCOMPRESSED_METADATA: u16 = 1 << 15;
pub(super) const UNCOMPRESSED_DATA: u32 = 1 << 24;

pub(super) fn invalid(message: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub(super) struct EncodedBlock {
    pub bytes: Vec<u8>,
    pub compressed: bool,
}

pub(super) fn encode_block(bytes: &[u8], level: i32) -> io::Result<EncodedBlock> {
    let mut encoder = ZstdEncoder::new(level);
    let mut input = PartialBuffer::new(bytes);
    let mut result = Vec::new();
    loop {
        let mut output = PartialBuffer::new([0; METADATA_SIZE]);
        encoder.encode(&mut input, &mut output)?;
        result.extend_from_slice(output.written());
        if input.unwritten().is_empty() {
            break;
        }
    }
    loop {
        let mut output = PartialBuffer::new([0; METADATA_SIZE]);
        let done = encoder.finish(&mut output)?;
        result.extend_from_slice(output.written());
        if result.len() >= bytes.len() {
            return Ok(EncodedBlock {
                bytes: bytes.to_vec(),
                compressed: false,
            });
        }
        if done {
            return Ok(EncodedBlock {
                bytes: result,
                compressed: true,
            });
        }
    }
}

pub(super) fn decode_block(bytes: &[u8], compressed: bool, limit: usize) -> io::Result<Vec<u8>> {
    if !compressed {
        if bytes.len() > limit {
            return Err(invalid("uncompressed SquashFS block exceeds its limit"));
        }
        return Ok(bytes.to_vec());
    }
    let mut decoder = ZstdDecoder::new();
    let mut input = PartialBuffer::new(bytes);
    let mut output = PartialBuffer::new(vec![0; limit + 1]);
    loop {
        let before = (input.written().len(), output.written().len());
        let done = decoder.decode(&mut input, &mut output)?;
        if output.written().len() > limit {
            return Err(invalid("decompressed SquashFS block exceeds its limit"));
        }
        if done {
            if !input.unwritten().is_empty() {
                return Err(invalid("trailing bytes in compressed SquashFS block"));
            }
            return Ok(output.written().to_vec());
        }
        if before == (input.written().len(), output.written().len()) {
            return Err(invalid("incomplete compressed SquashFS block"));
        }
    }
}

/// Encodes consecutive SquashFS metadata blocks without seeking the output.
pub struct MetadataBlocksWriter<W> {
    writer: W,
    input: Vec<u8>,
    output: Vec<u8>,
    output_position: usize,
    level: i32,
}

impl<W> MetadataBlocksWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            input: Vec::with_capacity(METADATA_SIZE),
            output: Vec::new(),
            output_position: 0,
            level: 3,
        }
    }

    pub(super) fn with_level(writer: W, level: i32) -> Self {
        Self {
            level,
            ..Self::new(writer)
        }
    }

    fn prepare_block(&mut self) -> io::Result<()> {
        if self.output.is_empty() && !self.input.is_empty() {
            let block = encode_block(&self.input, self.level)?;
            let header = block.bytes.len() as u16
                | if block.compressed {
                    0
                } else {
                    UNCOMPRESSED_METADATA
                };
            self.output.extend_from_slice(&header.to_le_bytes());
            self.output.extend_from_slice(&block.bytes);
            self.input.clear();
        }
        Ok(())
    }

    fn accept(&mut self, bytes: &[u8]) -> usize {
        let count = bytes.len().min(METADATA_SIZE - self.input.len());
        self.input.extend_from_slice(&bytes[..count]);
        count
    }

    fn advance_output(&mut self, count: usize) -> io::Result<()> {
        if count == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }
        self.output_position += count;
        if self.output_position == self.output.len() {
            self.output.clear();
            self.output_position = 0;
        }
        Ok(())
    }
}

impl<W: Write> Write for MetadataBlocksWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.input.len() == METADATA_SIZE || !self.output.is_empty() {
            self.flush()?;
        }
        Ok(self.accept(bytes))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.prepare_block()?;
        while !self.output.is_empty() {
            let count = match self.writer.write(&self.output[self.output_position..]) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            self.advance_output(count)?;
        }
        self.writer.flush()
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for MetadataBlocksWriter<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if bytes.is_empty() {
            return Poll::Ready(Ok(0));
        }
        if self.input.len() == METADATA_SIZE || !self.output.is_empty() {
            ready!(self.as_mut().poll_flush(cx))?;
        }
        Poll::Ready(Ok(self.accept(bytes)))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.prepare_block()?;
        while !this.output.is_empty() {
            let count = ready!(
                Pin::new(&mut this.writer).poll_write(cx, &this.output[this.output_position..])
            )?;
            this.advance_output(count)?;
        }
        Pin::new(&mut this.writer).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.as_mut().poll_flush(cx))?;
        Pin::new(&mut self.get_mut().writer).poll_shutdown(cx)
    }
}

pub(super) struct MetadataTable {
    writer: MetadataBlocksWriter<Vec<u8>>,
}

impl MetadataTable {
    pub fn new(level: i32) -> Self {
        Self {
            writer: MetadataBlocksWriter::with_level(Vec::new(), level),
        }
    }

    pub fn reference(&mut self) -> io::Result<u64> {
        if self.writer.input.len() == METADATA_SIZE {
            Write::flush(&mut self.writer)?;
        }
        let block = u32::try_from(self.writer.writer.len())
            .map_err(|_| invalid("SquashFS metadata table exceeds 4 GiB"))?;
        Ok((u64::from(block) << 16) | self.writer.input.len() as u64)
    }

    pub fn finish(mut self) -> io::Result<Vec<u8>> {
        Write::flush(&mut self.writer)?;
        Ok(self.writer.writer)
    }
}

impl Write for MetadataTable {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        Write::write(&mut self.writer, bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        Write::flush(&mut self.writer)
    }
}

struct MetadataDecoder {
    header: [u8; 2],
    header_position: usize,
    payload: Vec<u8>,
    payload_position: usize,
    compressed: bool,
    output: Vec<u8>,
    output_position: usize,
}

impl MetadataDecoder {
    fn new() -> Self {
        Self {
            header: [0; 2],
            header_position: 0,
            payload: Vec::new(),
            payload_position: 0,
            compressed: false,
            output: Vec::new(),
            output_position: 0,
        }
    }

    fn accept_header(&mut self) -> io::Result<()> {
        let header = u16::from_le_bytes(self.header);
        let size = usize::from(header & !UNCOMPRESSED_METADATA);
        if size == 0 || size > METADATA_SIZE {
            return Err(invalid("invalid SquashFS metadata block size"));
        }
        self.compressed = header & UNCOMPRESSED_METADATA == 0;
        self.payload.resize(size, 0);
        self.payload_position = 0;
        Ok(())
    }

    fn decode(&mut self) -> io::Result<()> {
        self.output = decode_block(&self.payload, self.compressed, METADATA_SIZE)?;
        if self.output.is_empty() {
            return Err(invalid("empty SquashFS metadata block"));
        }
        self.output_position = 0;
        self.header_position = 0;
        self.payload.clear();
        Ok(())
    }

    fn copy_output(&mut self, bytes: &mut [u8]) -> usize {
        let count = bytes.len().min(self.output.len() - self.output_position);
        bytes[..count]
            .copy_from_slice(&self.output[self.output_position..self.output_position + count]);
        self.output_position += count;
        count
    }
}

/// Decodes consecutive Zstd or uncompressed SquashFS metadata blocks.
pub struct MetadataBlocksReader<R> {
    reader: R,
    decoder: MetadataDecoder,
}

impl<R> MetadataBlocksReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            decoder: MetadataDecoder::new(),
        }
    }
}

impl<R: Read> Read for MetadataBlocksReader<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        loop {
            let count = self.decoder.copy_output(bytes);
            if count != 0 {
                return Ok(count);
            }
            if self.decoder.header_position < 2 {
                let count = match self
                    .reader
                    .read(&mut self.decoder.header[self.decoder.header_position..])
                {
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    result => result?,
                };
                if count == 0 {
                    return if self.decoder.header_position == 0 {
                        Ok(0)
                    } else {
                        Err(io::ErrorKind::UnexpectedEof.into())
                    };
                }
                self.decoder.header_position += count;
                if self.decoder.header_position != 2 {
                    continue;
                }
                self.decoder.accept_header()?;
            }
            self.reader
                .read_exact(&mut self.decoder.payload[self.decoder.payload_position..])?;
            self.decoder.decode()?;
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for MetadataBlocksReader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if bytes.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let this = self.get_mut();
        loop {
            let count = this.decoder.copy_output(bytes.initialize_unfilled());
            if count != 0 {
                bytes.advance(count);
                return Poll::Ready(Ok(()));
            }
            if this.decoder.header_position < 2 {
                let mut buf =
                    ReadBuf::new(&mut this.decoder.header[this.decoder.header_position..]);
                ready!(Pin::new(&mut this.reader).poll_read(cx, &mut buf))?;
                let count = buf.filled().len();
                if count == 0 {
                    return Poll::Ready(if this.decoder.header_position == 0 {
                        Ok(())
                    } else {
                        Err(io::ErrorKind::UnexpectedEof.into())
                    });
                }
                this.decoder.header_position += count;
                if this.decoder.header_position != 2 {
                    continue;
                }
                this.decoder.accept_header()?;
            }
            let mut buf = ReadBuf::new(&mut this.decoder.payload[this.decoder.payload_position..]);
            ready!(Pin::new(&mut this.reader).poll_read(cx, &mut buf))?;
            let count = buf.filled().len();
            if count == 0 {
                return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into()));
            }
            this.decoder.payload_position += count;
            if this.decoder.payload_position == this.decoder.payload.len() {
                this.decoder.decode()?;
            }
        }
    }
}

#[cfg(test)]
#[path = "blocks/tests.rs"]
mod tests;
