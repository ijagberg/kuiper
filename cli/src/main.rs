use clap::Parser;
use kuiper_lib::{KuiperError, Request};
use reqwest::Method;
use std::{path::PathBuf, str::FromStr};

#[derive(clap::Parser)]
struct Args {
    path: PathBuf,
    #[arg(short)]
    env_file: Option<PathBuf>,
}

fn main() -> Result<(), KuiperError> {
    let Args { path, env_file } = Args::parse();
    let request_name = path;

    if let Some(env_file) = env_file {
        dotenv::from_path(env_file.canonicalize()?).unwrap();
    }
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info,kuiper_lib=trace");
    }
    pretty_env_logger::init_timed();
    let request = kuiper_lib::Request::find(request_name)?;

    send_request(&request);

    Ok(())
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
    let request = request.build().unwrap();

    let response = client.execute(request).unwrap();

    println!("{}", response.status());
    println!("{}", response.text().unwrap());
}
