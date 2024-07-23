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

use anyhow::{anyhow, Context, Result};
use byteorder::{ByteOrder, LittleEndian, WriteBytesExt};
use tokio::io::{AsyncRead, AsyncReadExt};
use uuid::Uuid;

pub async fn read_utf16_str<S>(stream: &mut S) -> Result<String>
    where S: AsyncRead + Unpin
{
    let length = stream.read_u16_le().await?;
    let mut read_buf = vec![0; (length as usize) * size_of::<u16>()];
    stream.read_exact(&mut read_buf).await?;

    let mut utf16_buf = vec![0; length as usize];
    LittleEndian::read_u16_into(&read_buf, &mut utf16_buf);
    Ok(String::from_utf16_lossy(utf16_buf.as_slice()))
}

pub fn write_utf16_str(stream: &mut dyn Write, value: &str) -> Result<()> {
    let value_utf16: Vec<u16> = value.encode_utf16().collect();
    let utf_len = u16::try_from(value_utf16.len())
            .context("UTF-16 string too long for stream")?;
    stream.write_u16::<LittleEndian>(utf_len)?;
    for ch in value_utf16 {
        stream.write_u16::<LittleEndian>(ch)?;
    }
    Ok(())
}

// Basically the same as StreamRead, but with async streams
pub async fn read_uuid<S>(stream: &mut S) -> Result<Uuid>
    where S: AsyncRead + Unpin
{
    let mut buffer = [0; 16];
    stream.read_exact(&mut buffer).await?;
    Ok(Uuid::from_bytes_le(buffer))
}

pub async fn read_sized_buffer<S>(stream: &mut S, max_size: u32) -> Result<Vec<u8>>
    where S: AsyncRead + Unpin
{
    let data_size = stream.read_u32_le().await?;
    if data_size > max_size {
        return Err(anyhow!("Message payload too large ({} bytes, limit {})",
                           data_size, max_size));
    }
    let mut buffer = vec![0; data_size as usize];
    stream.read_exact(buffer.as_mut_slice()).await?;
    Ok(buffer)
}

pub fn write_sized_buffer(stream: &mut dyn Write, buffer: &Vec<u8>) -> Result<()>
{
    let buffer_size = u32::try_from(buffer.len())
            .map_err(|_| anyhow!("Buffer too large for 32-bit stream ({} bytes)",
                                 buffer.len()))?;
    stream.write_u32::<LittleEndian>(buffer_size)?;
    Ok(stream.write_all(buffer.as_slice())?)
}
