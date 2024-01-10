use std::{
    io,
    mem::size_of,
    hash::Hasher,
};

use twox_hash::XxHash32;

use crate::sink::{SliceSink, vec_sink_for_decompression};

use super::header::{BlockInfo, BlockMode};
use super::Error;

pub struct FrameDecompressor {
    content_hasher: XxHash32,
    ext_dict_offset: usize,
    ext_dict_len: usize,
    dst_start: usize,
}

impl FrameDecompressor {
    pub fn new() -> FrameDecompressor {
        FrameDecompressor {
            content_hasher: XxHash32::with_seed(0),
            ext_dict_offset: 0,
            ext_dict_len: 0,
            dst_start: 0,
        }
    }

    pub fn decompress_frame(
        &mut self,
        input: &[u8],
        output: &mut [u8],
        block_size: usize,
        block_mode: BlockMode,
    ) -> Result<usize, io::Error> {
        // Initialize variables
        self.ext_dict_offset = 0;
        self.ext_dict_len = 0;
        self.dst_start = 0;
        let max_block_size = block_size;

        // Decompress block
        let block_info = BlockInfo::Compressed(input.len() as u32);
        let with_dict_mode = block_mode == BlockMode::Linked && self.ext_dict_len != 0;

        let decompressed_size = if with_dict_mode {
            debug_assert!(self.dst_start + max_block_size <= self.ext_dict_offset);
            let (head, tail) = output.split_at_mut(self.ext_dict_offset);
            let ext_dict = &tail[..self.ext_dict_len];

            crate::block::decompress::decompress_internal(
                input,
                &mut SliceSink::new(head, self.dst_start),
                ext_dict,
            )
        } else {
            debug_assert!(output.len() - self.dst_start >= max_block_size);
            crate::block::decompress::decompress_internal(
                input,
                &mut vec_sink_for_decompression(
                    output,
                    0,
                    self.dst_start,
                    self.dst_start + max_block_size,
                ),
                b"",
            )
        }
        .map_err(Error::DecompressionError)?;

        // Update content checksum
        self.content_hasher.write(&output[self.dst_start..self.dst_start + decompressed_size]);

        // Update indices
        self.dst_start += decompressed_size;

        Ok(decompressed_size)
    }

    pub fn finish_frame(&mut self, input: &[u8]) -> Result<usize, io::Error> {
        // Finalize content checksum
        let checksum = self.content_hasher.finish() as u32;
        let checksum_bytes = &checksum.to_le_bytes();

        if input.len() < size_of::<u32>() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }

        // Check content checksum
        if &input[self.dst_start..self.dst_start + size_of::<u32>()] != checksum_bytes.as_slice() {
            return Err(Error::ContentChecksumError.into());
        }

        // Update indices
        self.dst_start += size_of::<u32>();

        Ok(size_of::<u32>())
    }
}

// Additional utility functions and traits from the original FrameDecoder (e.g., io::Read, io::BufRead) could be added if needed.
