use std::str::FromStr;

use clap::Parser;
use kuiper_lib::Request;
use reqwest::Method;

#[derive(clap::Parser)]
struct Args {
    request_name: String,
}

fn main() {
    let args = Args::parse();

    let requests = kuiper_lib::evaluate_requests("requests".into());

    if let Some(request) = requests.get(&args.request_name) {
        println!("{:#?}", request);
        send_request(request);
    } else {
        eprintln!("could not find request with name '{}'", args.request_name);
    }
}

fn send_request(req: &Request) {
    let client = reqwest::blocking::Client::new();
    let mut request = client.request(Method::from_str(req.method()).unwrap(), req.uri());
    for (name, value) in req.headers() {
        request = request.header(name, value);
    }
    let request = request.build().unwrap();

    let response = client.execute(request).unwrap();

    println!("{}", response.status());
    println!("{}", response.text().unwrap());
}
