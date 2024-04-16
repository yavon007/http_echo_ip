use std::collections::HashMap;
use chrono::{FixedOffset, Local};
use clap::Parser;
use regex::Regex;
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// 监听地址
    #[arg(short, long, default_value = "127.0.0.1")]
    listen: String,

    /// 监听端口号
    #[arg(short, long, default_value_t = 80)]
    port: usize,
}

impl Args {
    fn to_string(&self) -> String {
        format!("{}:{}", self.listen, self.port)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let server = TcpListener::bind(args.to_string()).await?;
    println!("{}Listening on: {}", get_time(), args.to_string());

    let cache_times = Arc::new(Mutex::new(HashMap::new()));

    //正则从header读 nginx转发带的
    let forwarded_for_regex = Regex::new(r"X-Forwarded-For:\s*(?P<ip>(?:\d{1,3}\.){3}\d{1,3})").unwrap();
    let real_ip_regex = Regex::new(r"X-Real-IP:\s*(?P<ip>(?:\d{1,3}\.){3}\d{1,3})").unwrap();
    let remote_host_regex = Regex::new(r"REMOTE-HOST:\s*(?P<ip>(?:\d{1,3}\.){3}\d{1,3})").unwrap();
    let regexes = Arc::new(Vec::from([forwarded_for_regex, real_ip_regex, remote_host_regex]));

    loop {
        let regexes = Arc::clone(&regexes);
        let cache_times = Arc::clone(&cache_times);
        let (stream, _) = server.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = process(stream, regexes, cache_times).await {
                println!("failed to process connection; error = {}", e);
            }
        });
    }
}

async fn process(mut stream: TcpStream, regexes: Arc<Vec<Regex>>, cache_times: Arc<Mutex<HashMap<String, usize>>>) -> Result<(), Box<dyn Error>> {
    //直接读链接的地址
    let mut ip = stream.peer_addr()?.ip().to_string();
    let mut http_request = BufReader::new(&mut stream).lines();

    //接收数据从header再读一遍
    while let Some(line) = http_request.next_line().await? {
        if line.len() == 0 {
            break;
        }
        for regex in regexes.iter() {
            if let Some(capture) = regex.captures(&line) {
                if let Some(v) = capture.name("ip") {
                    ip = v.as_str().to_string();
                }
            }
        }
    }
    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\n\r\n{}\r\n", ip.len(), ip).as_bytes()).await?;
    let mut map = cache_times.lock().unwrap();
    let mut times = 1;
    match map.get_mut(&ip) {
        None => {
            map.insert(ip.clone(), 1);
        }
        Some(v) => {
            *v += 1;
            times = *v;
        }
    }
    println!("{}Request from: {:<15}({})", get_time(), ip, times);
    Ok(())
}

fn get_time() -> String {
    let offset = FixedOffset::east_opt(8 * 60 * 60).unwrap();
    let shanghai_time = Local::now().with_timezone(&offset);
    shanghai_time.format("[%Y-%m-%d %H:%M:%S%.3f]").to_string()
}