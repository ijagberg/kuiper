use std::str::FromStr;

use clap::Parser;
use kuiper_lib::{KuiperError, Request};
use reqwest::Method;

#[derive(clap::Parser)]
struct Args {
    request_name: String,
    #[arg(short, default_value = "Requests")]
    dir: String,
    #[arg(short)]
    env: Option<String>,
}

fn main() -> Result<(), KuiperError> {
    let args = Args::parse();

    let requests = kuiper_lib::Requests::evaluate(&args.dir, args.env)?;
    // println!("found the following requests:");
    // for (name, _) in &requests {
    //     println!("{}", name);
    // }

    if let Some(request) = requests.get(&args.request_name) {
        // println!("{:#?}", request);
        send_request(request);
    } else {
        return Err(KuiperError::RequestNotFound(args.request_name));
    }

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
