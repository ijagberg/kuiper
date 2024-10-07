use clap::Parser;
use libkuiper::Request;
use reqwest::Method;
use std::{path::PathBuf, str::FromStr};

#[derive(clap::Parser)]
struct Args {
    path: String,
    #[arg(short)]
    env_file: Option<PathBuf>,
    /// Specify this argument to start request evaluation from this directory.
    #[arg(short)]
    dir: Option<PathBuf>,
}

fn main() {
    let Args {
        path,
        env_file,
        dir,
    } = Args::parse();

    if let Some(env_file) = env_file {
        match env_file.canonicalize() {
            Ok(env_file_path) => dotenv::from_path(env_file_path).unwrap(),
            Err(e) => {
                eprintln!("failed to read env file: '{}'", e);
                return;
            }
        }
    }

    let mut file_path = PathBuf::new();
    file_path.push(&path);

    let dir = dir.unwrap_or(std::env::current_dir().expect("should be able to read current dir"));
    file_path = dir.join(file_path);

    match file_path.canonicalize() {
        Ok(existing_path) => {
            if std::env::var("RUST_LOG").is_err() {
                std::env::set_var("RUST_LOG", "info,kuiper_lib=trace");
            }

            pretty_env_logger::init_timed();
            match libkuiper::Request::find(existing_path.clone()) {
                Ok(request) => {
                    send_request(&request);
                }
                Err(e) => {
                    eprintln!("failed to parse request with name: {existing_path:?}: '{e}'");
                }
            }
        }
        Err(_) => {
            // try searching instead of finding
            let mut m = Request::search(dir, &path).expect("failed to search");
            if m.is_empty() {
                eprintln!("no request found for that term '{}'", path);
            } else if m.len() > 1 {
                eprintln!(
                    "multiple candidate requests for term '{}': [{}]",
                    path,
                    m.iter().map(|r| r.name()).collect::<Vec<_>>().join(", ")
                );
            } else {
                let request = m.remove(0);
                send_request(&request);
            }
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

    println!("{}", req.name());
    println!("{}", response.status());
    println!("{}", response.text().unwrap());
}
