use clap::Parser;
use libkuiper::Request;
use reqwest::Method;
use std::{path::PathBuf, str::FromStr};

#[derive(clap::Parser)]
struct Args {
    path: PathBuf,
    #[arg(short)]
    env_file: Option<PathBuf>,
}

fn main() {
    let Args { path, env_file } = Args::parse();
    let request_name = path;

    if let Some(env_file) = env_file {
        match env_file.canonicalize() {
            Ok(path) => dotenv::from_path(path).unwrap(),
            Err(e) => {
                eprintln!("failed to read env file: '{}'", e.to_string());
                return;
            }
        }
    }
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info,kuiper_lib=trace");
    }
    pretty_env_logger::init_timed();
    match libkuiper::Request::find(request_name.clone()) {
        Ok(request) => {
            send_request(&request);
        }
        Err(e) => {
            eprintln!("failed to parse request with name: '{request_name:?}': '{e}'");
            return;
        }
    }
}

fn send_request(req: &Request) {
    let client = reqwest::blocking::Client::new();
    let mut request = client.request(Method::from_str(req.method()).unwrap(), req.uri());
    for (name, value) in req.headers() {
        if let Some(v) = value {
            request = request.header(name, v);
        }
    }

    if let Some(body) = req.body() {
        request = request.json(body);
    }

    request = request.query(&req.params().iter().collect::<Vec<_>>());

    let request = request.build().unwrap();

    let response = client.execute(request).unwrap();

    println!("{}", response.status());
    println!("{}", response.text().unwrap());
}
