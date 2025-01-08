use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::channel;
use std::thread;
use termios;

struct IncomingData {
    peer_addr: std::net::SocketAddr,
    port: u16,
    data: Vec<u8>,
    timestamp: std::time::Instant,
    extra_message: Option<String>,
}

fn handle_client(
    port: u16,
    mut stream: TcpStream,
    tx: std::sync::mpsc::Sender<IncomingData>,
) -> std::io::Result<()> {
    let message: Option<String> = Some("Connection received".to_string());

    let mut connection = IncomingData {
        peer_addr: stream.peer_addr().unwrap(),
        port,
        data: Vec::new(),
        timestamp: std::time::Instant::now(),
        extra_message: message,
    };

    let peer_addr = stream.peer_addr().unwrap();

    let mut buf = [0; 1024];
    loop {
        let bytes_read = stream.read(&mut buf);
        match bytes_read {
            Ok(bytes_read) => {
                if bytes_read == 0 {
                    break;
                }

                let bytes = IncomingData {
                    peer_addr: peer_addr,
                    port: port,
                    data: buf.to_vec(),
                    timestamp: std::time::Instant::now(),
                    extra_message: None,
                };

                stream.write(&buf[..bytes_read]).unwrap();
                // Send the bytes read as a string of 2-digit uppercase hex values
                tx.send(bytes).unwrap();
            }
            Err(e) => {
                //  If connectionion reset by peer, just break the loop
                if e.kind() == std::io::ErrorKind::ConnectionReset {
                    break;
                }
                return Err(e);
            }
        }
    }

    connection.extra_message = Some("Connection closed".to_string());

    tx.send(connection).unwrap();

    // // Send the shutdown message to the receiver
    // tx.send(IncomingData {
    //     peer_addr: peer_addr,
    //     port: port,
    //     data: Vec::new(),
    //     timestamp: std::time::Instant::now(),
    //     extra_message: Some("Shutdown".to_string()),
    // })
    // .unwrap();

    Ok(())
}

fn start_tcp_server(port: u16, tx: std::sync::mpsc::Sender<IncomingData>) {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).unwrap();

    let local_port = listener.local_addr().unwrap().port();

    for stream in listener.incoming() {
        let stream = stream.unwrap();

        let s = match stream.peer_addr() {
            Ok(s) => s,
            Err(_) => continue,
        };

        let handle_tx = tx.clone();

        thread::spawn(move || {
            handle_client(local_port, stream, handle_tx).unwrap();
        });

        if let Err(e) = tx.send(IncomingData {
            peer_addr: s,
            port: local_port,
            data: Vec::new(),
            timestamp: std::time::Instant::now(),
            extra_message: Some("Connection established".to_string()),
        }) {
            eprintln!("Error sending connection established message: {}", e);
        }

        // Close the listener after the first connection and end the server
        break;
    }
}

fn watch_for_keypress(tx: std::sync::mpsc::Sender<IncomingData>) {
    // Use raw mode to avoid waiting for a newline
    let existing_termios = termios::Termios::from_fd(0).unwrap();
    let mut termios_in_noncanonical_mode = existing_termios.clone();
    termios_in_noncanonical_mode.c_lflag &= !(termios::ICANON | termios::ECHO);

    let _ = termios::tcsetattr(0, termios::TCSANOW, &termios_in_noncanonical_mode);

    loop {
        let mut buf = [0; 1];
        let _ = std::io::stdin().read(&mut buf);

        let key = buf[0] as char;

        if key == 'q' {
            tx.send(IncomingData {
                peer_addr: "0.0.0.0:0".parse().unwrap(),
                port: 0,
                data: Vec::new(),
                timestamp: std::time::Instant::now(),
                extra_message: Some("Shutdown".to_string()),
            })
            .unwrap();
            break;
        }
        if key == '?' {
            println!("Press 'q' to quit");
        }
    }

    // Reset the terminal to normal mode
    let _ = termios::tcsetattr(0, termios::TCSANOW, &existing_termios);
}

fn set_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Reset the terminal to normal mode
        let existing_termios = termios::Termios::from_fd(0).unwrap();
        let mut termios_in_cononical_mode = existing_termios.clone();
        termios_in_cononical_mode.c_lflag |= termios::ICANON | termios::ECHO;
        let _ = termios::tcsetattr(0, termios::TCSANOW, &termios_in_cononical_mode);

        // Call the default panic hook
        default_hook(info);
    }));
}

fn main() -> std::io::Result<()> {
    set_panic_hook();

    println!("Hello, world!");

    let listening_ports = vec![3000, 3001, 3002];

    let (tx, rx) = channel::<IncomingData>();

    for port in listening_ports {
        let tx = tx.clone();
        thread::spawn(move || {
            start_tcp_server(port, tx);
        });
    }

    let keypress_tx = tx.clone();

    let keypress_builder =
        thread::Builder::new()
            .name("keypress".to_string())
            .spawn(move || {
                watch_for_keypress(keypress_tx);
            })?;

    for received in rx {
        match received.extra_message {
            Some(message) => {
                if message == "Shutdown" {
                    println!("{}: {}", received.timestamp.elapsed().as_secs(), message);
                    break;
                }
                println!("{}: {}", received.timestamp.elapsed().as_secs(), message);
            }
            None => {
                println!(
                    "{}: Received {} bytes from {}:{}",
                    received.timestamp.elapsed().as_secs(),
                    received.data.len(),
                    received.peer_addr.ip(),
                    received.port
                );
                println!("Received from port {}: {:?}", received.port, received.data);
            }
        }
    }

    keypress_builder.join().unwrap();

    println!("Goodbye, world!");

    Ok(())
}
