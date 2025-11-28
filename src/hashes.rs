/* This file is part of moulars.
 *
 * moulars is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * moulars is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with moulars.  If not, see <http://www.gnu.org/licenses/>.
 */

use std::io::Write;
use std::mem::size_of;

use anyhow::{anyhow, Result};
use data_encoding::{HEXLOWER, HEXLOWER_PERMISSIVE};
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::plasma::StreamWrite;

// This is used for both Sha0 and Sha1 digests
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct ShaDigest{
    data: [u8; 20]
}

macro_rules! sha_common {
    ($hv:ident, $f:ident, $k:ident, $w:ident) => {
        let temp = $hv[0].rotate_left(5).wrapping_add($f).wrapping_add($hv[4])
                         .wrapping_add($k).wrapping_add(*$w);
        $hv[4] = $hv[3];
        $hv[3] = $hv[2];
        $hv[2] = $hv[1].rotate_left(30);
        $hv[1] = $hv[0];
        $hv[0] = temp;
    }
}

impl ShaDigest {
    pub fn from_hex(hex: &str) -> Result<Self> {
        let data: [u8; 20] = HEXLOWER_PERMISSIVE.decode(hex.as_bytes())
                                .map_err(|err| anyhow!("Invalid hex literal: {err}"))?
                                .try_into().map_err(|_| anyhow!("Invalid SHA digest length"))?;
        Ok(Self { data })
    }

    pub fn as_hex(&self) -> String {
        HEXLOWER.encode(&self.data)
    }

    pub async fn read<S>(stream: &mut S) -> Result<Self>
        where S: AsyncRead + Unpin
    {
        let mut data = [0; 20];
        stream.read_exact(&mut data).await?;
        Ok(Self { data })
    }

    pub fn sha0(data: &[u8]) -> Self {
        const BLOCK_SIZE: usize = 64;

        // Hand-rolled implementation (based on DirtSand's) since RustCrypto
        // doesn't currently support Sha0
        let mut hash = [
            0x67452301_u32,
            0xefcdab89_u32,
            0x98badcfe_u32,
            0x10325476_u32,
            0xc3d2e1f0_u32
        ];

        // This implementation is only called for a small fixed-size buffer.
        // Therefore, for simplicity, we just store the whole message in memory.
        // However, we need to pad it up to 512 bits and append the size in
        // bits (BE) to the end of the buffer.
        let buf_size = (data.len() + 1 + size_of::<u64>()).div_ceil(BLOCK_SIZE) * BLOCK_SIZE;
        let mut buffer = vec![0; buf_size];
        {
            let (data_part, suffix) = buffer.split_at_mut(data.len());
            data_part.copy_from_slice(data);
            let size_in_bits = data.len() as u64 * 8;

            suffix[0] = 0x80;
            let size_pos = suffix.len() - size_of::<u64>();
            let (_, size_buf) = suffix.split_at_mut(size_pos);
            size_buf.copy_from_slice(&size_in_bits.to_be_bytes());
        }

        let mut work = [0u32; 80];
        for chunk in buffer.chunks_exact(BLOCK_SIZE) {
            for (src, dest) in chunk.chunks_exact(size_of::<u32>()).zip(work[0..16].iter_mut()) {
                *dest = u32::from_be_bytes(src.try_into().unwrap());
            }
            for i in 16..80 {
                // SHA-1 difference: no work[i].rotate_left(1)
                work[i] = work[i-3] ^ work[i-8] ^ work[i-14] ^ work[i-16];
            }

            let mut hv = hash;

            // Main SHA loop
            for w in &work[0..20] {
                const K: u32 = 0x5a827999;
                let f = (hv[1] & hv[2]) | (!hv[1] & hv[3]);
                sha_common!(hv, f, K, w);
            }
            for w in &work[20..40] {
                const K: u32 = 0x6ed9eba1;
                let f = hv[1] ^ hv[2] ^ hv[3];
                sha_common!(hv, f, K, w);
            }
            for w in &work[40..60] {
                const K: u32 = 0x8f1bbcdc;
                let f = (hv[1] & hv[2]) | (hv[1] & hv[3]) | (hv[2] & hv[3]);
                sha_common!(hv, f, K, w);
            }
            for w in &work[60..80] {
                const K: u32 = 0xca62c1d6;
                let f = hv[1] ^ hv[2] ^ hv[3];
                sha_common!(hv, f, K, w);
            }

            hash[0] = hash[0].wrapping_add(hv[0]);
            hash[1] = hash[1].wrapping_add(hv[1]);
            hash[2] = hash[2].wrapping_add(hv[2]);
            hash[3] = hash[3].wrapping_add(hv[3]);
            hash[4] = hash[4].wrapping_add(hv[4]);
        }

        let mut data = [0; 20];
        for (dest, src) in data.chunks_exact_mut(size_of::<u32>()).zip(hash.iter()) {
            dest.copy_from_slice(&src.to_be_bytes());
        }
        Self { data }
    }

    pub fn sha1(data: &[u8]) -> Self {
        use sha1::{Sha1, Digest};

        let mut hash = Sha1::new();
        hash.update(data);
        let result = hash.finalize();
        Self { data: result.into() }
    }

    #[must_use]
    pub fn endian_swap(&self) -> Self {
        let mut swapped = [0; 20];
        for (src, dest) in self.data.chunks_exact(size_of::<u32>())
                            .zip(swapped.chunks_exact_mut(size_of::<u32>())) {
            dest[0] = src[3];
            dest[1] = src[2];
            dest[2] = src[1];
            dest[3] = src[0];
        }
        Self { data: swapped }
    }
}

impl StreamWrite for ShaDigest {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        Ok(stream.write_all(&self.data)?)
    }
}

#[test]
fn test_sha_hashes() {
    // Sanity check with known-good SHA-1 implementation
    assert_eq!("da39a3ee5e6b4b0d3255bfef95601890afd80709",
               ShaDigest::sha1(b"").as_hex().as_str());
    assert_eq!("a9993e364706816aba3e25717850c26c9cd0d89d",
               ShaDigest::sha1(b"abc").as_hex().as_str());

    // Now the SHA-0 tests
    assert_eq!("f96cea198ad1dd5617ac084a3d92c6107708c0ef",
               ShaDigest::sha0(b"").as_hex().as_str());
    assert_eq!("37f297772fae4cb1ba39b6cf9cf0381180bd62f2",
               ShaDigest::sha0(b"a").as_hex().as_str());
    assert_eq!("0164b8a914cd2a5e74c4f7ff082c4d97f1edf880",
               ShaDigest::sha0(b"abc").as_hex().as_str());
    assert_eq!("d2516ee1acfa5baf33dfc1c471e438449ef134c8",
               ShaDigest::sha0(
                   b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")
               .as_hex().as_str());
    assert_eq!("459f83b95db2dc87bb0f5b513a28f900ede83237",
               ShaDigest::sha0(
                   b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmn\
                     hijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu")
               .as_hex().as_str());
}
