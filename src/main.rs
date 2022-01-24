use std::collections::HashMap;
use std::error::Error;
use std::io::{self, Read, Write};

use mio::event::Event;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Registry, Token};

const SERVER: Token = Token(0);

fn main() -> io::Result<()> {
    let mut poll = Poll::new()?;

    let mut events = Events::with_capacity(128);
    let addr = "127.0.0.1:8000".parse().unwrap();
    let mut server = TcpListener::bind(addr)?;
    poll
        .registry()
        .register(&mut server, SERVER, Interest::READABLE)?;

    let mut connections = HashMap::new();
    let mut connection_id = 1;

    loop {
        poll.poll(&mut events, None)?;

        for event in events.iter() {
            match event.token() {
                SERVER => loop {
                    let (mut connection, address) = match server.accept() {
                        Ok(connection_details) => connection_details,
                        Err(e) if would_block(&e) => {
                            // no more events
                            break;
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    };

                    println!("Accepted connection from: {}", address);
                    let token = Token(connection_id);
                    connection_id += 1;

                    poll.registry().register(
                        &mut connection,
                        token,
                        Interest::READABLE | Interest::WRITABLE,
                    );

                    connections.insert(token, connection);
                }
                token => {
                    let done = if let Some(connection) = connections.get_mut(&token) {
                        handle_connection_event(poll.registry(), connection, event)?
                    } else {
                        false
                    };
                    if done {
                        connections.remove(&token);
                    }
                }
            }
        }
    }
}

fn handle_connection_event(
    registry: &Registry,
    connection: &mut TcpStream,
    event: &Event,
) -> io::Result<bool> {
    let message = b"Hello from mio\n";
    if event.is_writable() {
        match connection.write(message) {
            Ok(n) if n < message.len() => {
                return Err(io::ErrorKind::WriteZero.into());
            }
            Ok(_) => {
                registry.reregister(connection, event.token(), Interest::READABLE)?;
            }
            Err(ref e) if would_block(e) => {},
            Err(ref e) if interrupted(e) => {
                return handle_connection_event(registry, connection, event);
            }
            Err(e) => return Err(e),
        }
    }

    if event.is_readable() {
        let mut connection_closed = false;
        let mut received_data = vec![0; 4096];
        let mut bytes_read = 0;

        loop {
            match connection.read(&mut received_data[bytes_read..]) {
                Ok(0) => {
                    connection_closed = true;
                    break;
                }
                Ok(n) => {
                    bytes_read += n;
                    if bytes_read == received_data.len() {
                        received_data.resize(received_data.len() + 1024, 0);
                    }
                }
                Err(ref err) if would_block(err) => break,
                Err(ref err) if interrupted(err) => continue,
                Err(err) => return Err(err),
            }
        }

        if bytes_read != 0 {
            let received_data = &received_data[..bytes_read];
            if let Ok(str_buf) = std::str::from_utf8(received_data) {
                println!("Received data: {}", str_buf.trim_end());
            } else {
                println!("Received (none UTF-8) data: {:?}", received_data);
            }
        }

        if connection_closed {
            println!("Connection closed");
            return Ok(true);
        }

    }


    Ok(false)
}

fn would_block(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::WouldBlock
}

fn interrupted(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::Interrupted
}
