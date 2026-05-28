use libc::{INADDR_ANY, socket};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::{
    error::Error,
    io::{self, Read},
    net::{SocketAddr, TcpListener},
    os::fd::FromRawFd,
};

const SADDR: &str = "127.0.0.1:9999";
const CADDR: &str = "127.0.0.1:8888";
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    unsafe {
        let fd = socket(libc::AF_INET, libc::SOCK_STREAM, 0);
        if fd < 0 {
            return Err(format!("Failed to create socket: {}", io::Error::last_os_error()).into());
        }
        let value: libc::c_int = 1;
        let res = libc::setsockopt(
            fd,
            libc::SOL_IP,
            libc::IP_TRANSPARENT,
            &value as *const libc::c_int as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        );

        if res < 0 {
            return Err(format!(
                "Error in setsockopt TRANSPARENT: {}",
                io::Error::last_os_error()
            )
            .into());
        }

        let res = libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            &value as *const libc::c_int as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        );

        if res < 0 {
            return Err(format!(
                "Error in setsockopt REUSEADDR: {}",
                io::Error::last_os_error()
            )
            .into());
        }

        let mut sockaddr = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: (9999 as u16).to_be(),
            sin_addr: libc::in_addr {
                s_addr: libc::INADDR_ANY,
            },
            sin_zero: [0; 8],
        };

        let res = libc::bind(
            fd,
            &sockaddr as *const libc::sockaddr_in as *const libc::sockaddr,
            size_of::<libc::sockaddr_in>() as libc::socklen_t,
        );

        if res < 0 {
            return Err(format!("Error in binding socket: {}", io::Error::last_os_error()).into());
        }

        if libc::listen(fd, 128) < 0 {
            return Err(format!("Error in listening: {}", io::Error::last_os_error()).into());
        }

        println!("Socket listening on port 9999");

        let listener = TcpListener::from_raw_fd(fd);

        for stream in listener.incoming() {
            match stream {
                Ok(s) => {
                    println!("Connection made.");
                    println!("Client addr: {:#?}", s.peer_addr());
                }
                Err(e) => {
                    eprintln!("Error in stream: {}", e);
                }
            }
        }
    }
    // loop {
    //     let mut buf = [0u8; 1024];
    //     let (mut stream, _) = listener.accept()?;
    //     match stream.read(&mut buf) {
    //         Ok(size) => {
    //             if size == 0 {
    //                 continue;
    //             }
    //             println!("Recieved bytes: {:#?}", &buf[..size]);
    //         }
    //         Err(e) => {
    //             eprintln!("Read Error: {:#?}", e);
    //             break;
    //         }
    //     }
    // }
    Ok(())
}
