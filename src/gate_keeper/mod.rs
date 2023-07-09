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

use crate::plasma::StreamRead;

use std::io::{Cursor, Result, Error, ErrorKind, BufRead};

use byteorder::{LittleEndian, ReadBytesExt};
use tokio::sync::mpsc;
use tokio::net::TcpStream;
use uuid::Uuid;

pub struct GateKeeper {
    incoming_send: mpsc::Sender<TcpStream>,
}

struct GateKeeperConnHeader {
    header_size: u32,
    uuid: Uuid,
}

impl GateKeeperConnHeader {
    const HEADER_SIZE: usize = 20;
}

impl StreamRead for GateKeeperConnHeader {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let header_size = stream.read_u32::<LittleEndian>()?;
        if header_size != Self::HEADER_SIZE as u32 {
            return Err(Error::new(ErrorKind::Other,
                       format!("[GateKeeper] Invalid connection header size {}", header_size)));
        }
        // Null UUID
        let uuid = Uuid::stream_read(stream)?;
        Ok(Self { header_size, uuid })
    }
}

async fn init_client(sock: &mut TcpStream) -> Result<()> {
    use tokio::io::AsyncReadExt;

    let mut buffer = [0u8; GateKeeperConnHeader::HEADER_SIZE];
    sock.read_exact(&mut buffer).await?;
    let mut stream = Cursor::new(buffer);
    let _ = GateKeeperConnHeader::stream_read(&mut stream)?;

    crate::crypt::init_crypt(sock).await?;

    Ok(())
}

async fn gate_keeper(mut incoming_recv: mpsc::Receiver<TcpStream>) {
    while let Some(mut sock) = incoming_recv.recv().await {
        match init_client(&mut sock).await {
            Ok(()) => {
                tokio::task::spawn(async move {
                    todo!();
                });
            }
            Err(err) => {
                eprintln!("[GateKeeper] Failed to initialize client: {:?}", err);
            }
        }
    }
}

impl GateKeeper {
    pub fn start() -> GateKeeper {
        let (incoming_send, incoming_recv) = mpsc::channel(5);

        tokio::spawn(async move {
            gate_keeper(incoming_recv).await;
        });
        GateKeeper { incoming_send }
    }

    pub async fn add(&mut self, sock: TcpStream) {
        if let Err(err) = self.incoming_send.send(sock).await {
            eprintln!("[GateKeeper] Failed to add client: {:?}", err);
            std::process::exit(1);
        }
    }
}
