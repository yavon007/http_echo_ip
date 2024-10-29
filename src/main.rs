use chrono::{FixedOffset, Local};
use clap::Parser;
use regex::Regex;
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use reqwest::Client;

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
    let forwarded_for_regex = Regex::new(r"X-Forwarded-For:\s*(?<ip>(?:\d{1,3}\.){3}\d{1,3})").unwrap();
    let real_ip_regex = Regex::new(r"X-Real-IP:\s*(?<ip>(?:\d{1,3}\.){3}\d{1,3})").unwrap();
    let remote_host_regex = Regex::new(r"REMOTE-HOST:\s*(?<ip>(?:\d{1,3}\.){3}\d{1,3})").unwrap();
    let regexes = Arc::new(Vec::from([forwarded_for_regex, real_ip_regex, remote_host_regex]));

    let path_regex = Arc::new(Regex::new(r".*/(?<path>[^?#/]+)\sHTTP").unwrap());

    let request_client = Arc::new(Client::builder().build().unwrap());

    loop {
        let (stream, _) = server.accept().await?;
        let regexes = Arc::clone(&regexes);
        let path_regex = Arc::clone(&path_regex);
        let cache_times = Arc::clone(&cache_times);
        let request_client = Arc::clone(&request_client);
        tokio::spawn(async move {
            if let Err(e) = process(stream, regexes, cache_times, path_regex, request_client).await {
                println!("failed to process connection; error = {}", e);
            }
        });
    }
}

async fn process(mut stream: TcpStream, regexes: Arc<Vec<Regex>>, cache_times: Arc<Mutex<HashMap<String, usize>>>, path_regex: Arc<Regex>, request_client: Arc<Client>) -> Result<(), Box<dyn Error>> {
    let (reader, mut writer) = stream.split();
    //先读取路径
    let mut http_request = BufReader::new(reader).lines();
    let first_line = http_request.next_line().await?.unwrap();
    if let Some(capture) = path_regex.captures(&first_line) {
        if let Some(path) = capture.name("path") {
            if path.as_str() != "ip" {
                //成功读取到路径，请求并进行返回
                let response = request_client.get(format!("http://ip-api.com/json/{}?lang=zh-CN", path.as_str())).send().await?.text().await?;
                writer.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json; charset=utf-8\r\n\r\n{}\r\n", response.len(), response).as_bytes()).await?;
                let times = increment(cache_times, path.as_str());
                println!("{}Request from: {:<15}({})", get_time(), path.as_str(), times);
                return Ok(());
            }
        }
    }

    //直接读链接的地址
    let mut ip = http_request.get_ref().get_ref().peer_addr().unwrap().ip().to_string();
    //从header读一遍看有没有转发头
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
    writer.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\n\r\n{}\r\n", ip.len(), ip).as_bytes()).await?;
    let times = increment(cache_times, ip.as_str());
    println!("{}Request from: {:<15}({})", get_time(), ip.as_str(), times);
    Ok(())
}

fn get_time() -> String {
    let offset = FixedOffset::east_opt(8 * 60 * 60).unwrap();
    let shanghai_time = Local::now().with_timezone(&offset);
    shanghai_time.format("[%Y-%m-%d %H:%M:%S%.3f]").to_string()
}

fn increment(cache_times: Arc<Mutex<HashMap<String, usize>>>, ip: &str) -> usize {
    let mut map = cache_times.lock().unwrap();
    map.entry(ip.to_string()).and_modify(|v| *v += 1).or_insert(1).clone()
}