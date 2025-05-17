use clap::Parser;
use libkuiper::{Error as KuiperLibError, Request};
use log::{error, trace};
use reqwest::{blocking::Response, Method};
use std::{fmt::Display, io, path::PathBuf, str::FromStr};

/// Arguments for the `kuiper` cli.
#[derive(clap::Parser)]
struct Args {
    path: String,
    #[arg(short)]
    env_file: Option<PathBuf>,
    /// Specify this argument to start request evaluation from this directory.
    #[arg(short)]
    dir: Option<PathBuf>,
    /// Disable interpolation. This is 'false' by default (interpolation is enabled).
    #[arg(short, long, default_value = "false")]
    no_interpolation: bool,
    /// Output the request that would be sent.
    #[arg(long, default_value = "false")]
    dry_run: bool,
}

fn main() {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info,kuiper_lib=trace");
    }
    pretty_env_logger::init_timed();

    let args = Args::parse();
    if let Err(e) = run_main(args) {
        error!("{}", e);
        match e {
            Error::KuiperLib(kuiper_error) => eprintln!("{}", kuiper_error),
            Error::Reqwest(_) => eprintln!("error sending request",),
            Error::Dotenv(_) => {
                eprintln!("could not read env file")
            }
            Error::IO(_) => eprintln!("I/O error"),
        }
    }
}

/// Run the CLI tool, and return a result depending on the success.
/// In the case of an error, the main() function will take care of printing an error message.
fn run_main(
    Args {
        path,
        env_file,
        dir,
        no_interpolation,
        dry_run,
    }: Args,
) -> Result<(), Error> {
    if let Some(env_file) = env_file {
        dotenv::from_path(env_file.canonicalize()?)?;
    }

    let mut file_path = PathBuf::new();
    file_path.push(&path);

    let dir = dir.unwrap_or(std::env::current_dir()?);
    file_path = dir.join(file_path);

    let request = libkuiper::Request::from_file(file_path.canonicalize()?, !no_interpolation)?;

    if dry_run {
        println!("would send request to the following uri:");
        println!("{}", request.uri());
    } else {
        let response = send_request(request)?;
        println!("{}", response.status());
        if let Ok(text) = response.text() {
            println!("{}", text);
        } else {
            eprintln!("response was not text");
        }
    }
    Ok(())
}

fn send_request(req: Request) -> Result<Response, Error> {
    let client = reqwest::blocking::Client::new();
    let mut request = client.request(Method::from_str(req.method()).unwrap(), req.uri());
    for (name, value) in req.headers() {
        if let Some(v) = value {
            request = request.header(name, v);
        }
    }

    request = request.query(&req.params().iter().collect::<Vec<_>>());

    if let Some(body) = req.body() {
        request = request.body(body);
    }

    let request = request.build()?;

    trace!("sending request to '{}'", request.url());
    let response = client.execute(request)?;

    Ok(response)
}

#[derive(Debug)]
enum Error {
    KuiperLib(KuiperLibError),
    Reqwest(reqwest::Error),
    Dotenv(dotenv::Error),
    IO(std::io::Error),
}

impl std::error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Error::KuiperLib(kuiper_error) => format!("kuiper lib error: '{}'", kuiper_error),
                Error::Reqwest(error) => format!("reqwest error: '{}'", error),
                Error::Dotenv(error) => format!("dotenv error: '{}'", error),
                Error::IO(error) => format!("I/O error: '{}'", error),
            }
        )
    }
}

impl From<KuiperLibError> for Error {
    fn from(value: KuiperLibError) -> Self {
        Self::KuiperLib(value)
    }
}

impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        Self::Reqwest(value)
    }
}

impl From<dotenv::Error> for Error {
    fn from(value: dotenv::Error) -> Self {
        Self::Dotenv(value)
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::IO(value)
    }
}
