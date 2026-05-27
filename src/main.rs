use std::error::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:9999").await?;
    loop {
        let (mut stream, clientaddr) = listener.accept().await?;
        println!("Connected with {}", clientaddr);
        let mut res = String::new();
        // let mut res = Vec::new();

        if let Ok(size) = stream.read_to_string(&mut res).await
            && size != 0
        {
            println!("Res: {:#?}", &res[..size]);
            _ = stream.write_all(b"Hello Response").await;
        } else {
            continue;
        };
    }
}
