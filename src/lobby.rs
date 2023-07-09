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
use crate::gate_keeper::GateKeeper;

use std::io::{BufRead, Cursor, Result, Error, ErrorKind};
use tokio::net::{TcpListener, TcpStream};
use byteorder::{LittleEndian, ReadBytesExt};
use uuid::Uuid;

struct ConnectionHeader {
    conn_type: u8,
    // sock_header_size: u16,
    build_id: u32,
    build_type: u32,
    branch_id: u32,
    product_id: Uuid,
}

impl ConnectionHeader {
    const CONN_HEADER_SIZE: usize = 31;

    pub async fn read(sock: &mut TcpStream) -> Result<Self> {
        use tokio::io::AsyncReadExt;

        let mut buffer = [0u8; Self::CONN_HEADER_SIZE];
        sock.read_exact(&mut buffer).await?;

        let mut stream = Cursor::new(buffer);
        ConnectionHeader::stream_read(&mut stream)
    }
}

impl StreamRead for ConnectionHeader {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let conn_type = stream.read_u8()?;
        let sock_header_size = stream.read_u16::<LittleEndian>()?;
        if sock_header_size != Self::CONN_HEADER_SIZE as u16 {
            return Err(Error::new(ErrorKind::Other,
                       format!("Invalid socket header size: {}", sock_header_size)));
        }
        let build_id = stream.read_u32::<LittleEndian>()?;
        let build_type = stream.read_u32::<LittleEndian>()?;
        let branch_id = stream.read_u32::<LittleEndian>()?;
        let product_id = Uuid::stream_read(stream)?;

        Ok(Self {
            conn_type, /* sock_header_size, */ build_id, build_type, branch_id,
            product_id
        })
    }
}

const CONN_CLI_TO_AUTH: u8 = 10;
const CONN_CLI_TO_GAME: u8 = 11;
const CONN_CLI_TO_FILE: u8 = 16;
const CONN_CLI_TO_CSR: u8 = 20;
const CONN_CLI_TO_GATE_KEEPER: u8 = 22;

fn connection_type_name(conn_type: u8) -> String {
    match conn_type {
        CONN_CLI_TO_AUTH => "Cli2Auth".to_string(),
        CONN_CLI_TO_GAME => "Cli2Game".to_string(),
        CONN_CLI_TO_FILE => "Cli2File".to_string(),
        CONN_CLI_TO_CSR => "Cli2Csr".to_string(),
        CONN_CLI_TO_GATE_KEEPER => "Cli2GateKeeper".to_string(),
        _ => format!("Unknown ({})", conn_type)
    }
}

pub async fn lobby_server(listen_addr: &str) {
    let listener = match TcpListener::bind(listen_addr).await {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("Failed to bind on address {}: {:?}", listen_addr, err);
            std::process::exit(1);
        }
    };

    let mut gate_keeper = GateKeeper::start();

    loop {
        let (mut sock, sock_addr) = match listener.accept().await {
            Ok((sock, sock_addr)) => (sock, sock_addr),
            Err(err) => {
                eprintln!("Failed to accept from socket: {:?}", err);
                continue;
            }
        };
        let header = match ConnectionHeader::read(&mut sock).await {
            Ok(header) => header,
            Err(err) => {
                eprintln!("Failed to read connection header: {:?}", err);
                continue;
            }
        };

        println!("[Lobby] {} connection from {}: Build {} ({}), Branch {}, Product {}",
                 connection_type_name(header.conn_type), sock_addr,
                 header.build_id, header.build_type, header.branch_id,
                 header.product_id);

        match header.conn_type {
            CONN_CLI_TO_GATE_KEEPER => {
                gate_keeper.add(sock).await;
            }
            CONN_CLI_TO_FILE => {
                todo!();
            }
            CONN_CLI_TO_AUTH => {
                todo!();
            }
            CONN_CLI_TO_GAME => {
                todo!();
            }
            CONN_CLI_TO_CSR => {
                eprintln!("[Lobby] {} - Got CSR client; rejecting", sock_addr);
                continue;
            }
            _ => {
                eprintln!("[Lobby] {} - Unknown connection type {}; rejecting",
                          sock_addr, header.conn_type);
                continue;
            }
        }
    }
}
