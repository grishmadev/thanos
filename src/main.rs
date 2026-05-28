use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::{error::Error, io::Read, net::SocketAddr};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const LB_ADDR: &str = "0.0.0.0:9999";
const SR_ADDR: &str = "127.0.0.1:8888";
const CL_ADDR: &str = "127.0.0.1:8800";
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let lb_addr: SockAddr = LB_ADDR.parse::<SocketAddr>()?.into();
    let sr_addr: SockAddr = SR_ADDR.parse::<SocketAddr>()?.into();
    let cl_addr: SockAddr = CL_ADDR.parse::<SocketAddr>()?.into();
    let lb_listener = TcpListener::bind(LB_ADDR).await?;
    let (mut lb_stream, _) = lb_listener.accept().await?;
    loop {
        let mut buf = [0u8; 1024];
        let size = lb_stream.read(&mut buf).await?;
        if size == 0 {
            continue;
        }
        let mut sr_stream = TcpStream::connect(SR_ADDR).await?;
        println!("Data: {:?}", &buf[..size]);
        sr_stream.write_all(&buf[..size]).await?;

        // Reading from the Server
        let size = sr_stream.read(&mut buf).await?;
        lb_stream.write_all(&buf[..size]).await?;
    }
}
