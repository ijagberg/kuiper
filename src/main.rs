use bytes::Bytes;
use clap::Parser;
use headers::Headers;
use interpolation::{
    InterpolationError, interpolate_body, interpolate_headers, interpolate_params, interpolate_uri,
};
use log::{error, trace};
use params::Params;
use request::{RequestFile, get_request_file};
use reqwest::{Method, StatusCode, blocking::Response};
use std::{
    fmt::Display,
    io,
    path::{Path, PathBuf},
    str::FromStr,
};

pub(crate) mod body;
pub(crate) mod headers;
pub(crate) mod interpolation;
pub(crate) mod params;
pub(crate) mod request;

type KuiperResult<T> = Result<T, Error>;

/// Arguments for the `kuiper` cli.
#[derive(clap::Parser)]
#[command(version, about)]
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
    /// Additional header files to use.
    /// kuiper will first apply header files by looking in each directory on the way to the
    /// specified request, and then each header file specified in this argument.
    #[arg(long, short('H'), action = clap::ArgAction::Append)]
    header_files: Option<Vec<String>>,
    /// If 'true', `kuiper` will omit the status code of the response. This is 'false' by default.
    #[arg(long, default_value = "false")]
    omit_response_code: bool,
}

fn main() {
    // Trigger a rebuild when any change is made to Cargo.toml, to make sure the --version argument
    // will output the correct value.
    include_str!("../Cargo.toml");

    pretty_env_logger::init_timed();

    let args = Args::parse();
    if let Err(e) = run_main(args) {
        error!("{}", e);
        match e {
            Error::Reqwest(_) => eprintln!("error sending request",),
            Error::Dotenv(_) => {
                eprintln!("could not read env file")
            }
            Error::IO(_) => eprintln!("I/O error"),
            Error::Interpolation(_) => eprintln!("interpolation error"),
            Error::Json(_) => eprintln!("invalid JSON"),
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
        header_files,
        omit_response_code,
    }: Args,
) -> Result<(), Error> {
    if let Some(env_file) = env_file {
        dotenv::from_path(env_file.canonicalize()?)?;
    }

    let canon_file_path = get_canon_file_path(dir, &path)?;
    let request = build_request(canon_file_path, header_files, !no_interpolation)?;
    if dry_run {
        println!("{} {}", request.method(), request.uri(),);
        println!(
            "params: {}",
            serde_json::to_string(request.params()).unwrap()
        );
        println!(
            "headers: {}",
            serde_json::to_string(request.headers()).unwrap()
        );
    } else {
        let response = send_request(request)?;
        print_response_status_code(response.status(), !omit_response_code);
        if let Ok(text) = response.text() {
            println!("{}", text);
        } else {
            eprintln!("response was not text");
        }
    }
    Ok(())
}

fn print_response_status_code(status: StatusCode, enabled: bool) {
    if enabled {
        println!("{}", status);
    }
}

fn get_canon_file_path(dir: Option<PathBuf>, path: &String) -> KuiperResult<PathBuf> {
    let mut file_path = PathBuf::new();
    file_path.push(path);
    let dir = dir.unwrap_or(std::env::current_dir()?);
    file_path = dir.join(file_path);
    if file_path.is_relative() {
        file_path = file_path.canonicalize()?;
    }

    Ok(file_path)
}

fn send_request(
    FinishedRequest {
        uri,
        method,
        params,
        headers,
        body,
    }: FinishedRequest,
) -> Result<Response, Error> {
    let client = reqwest::blocking::Client::new();
    let mut request = client.request(method, uri);
    for (name, value) in headers {
        if let Some(v) = value {
            request = request.header(name, v);
        }
    }

    request = request.query(&params.into_iter().collect::<Vec<_>>());

    if let Some(body) = body {
        request = request.body(body);
    }

    let request = request.build()?;

    trace!("sending request to '{}'", request.url());
    let response = client.execute(request)?;

    Ok(response)
}

/// This is a request that `kuiper` can actually send.
#[derive(Debug, Clone, PartialEq)]
struct FinishedRequest {
    uri: String,
    method: reqwest::Method,
    params: Params,
    headers: Headers,
    body: Option<Bytes>,
}

impl FinishedRequest {
    fn new(
        uri: String,
        method: reqwest::Method,
        params: Params,
        headers: Headers,
        body: Option<Bytes>,
    ) -> Self {
        Self {
            uri,
            method,
            params,
            headers,
            body,
        }
    }

    fn uri(&self) -> &str {
        &self.uri
    }

    fn method(&self) -> &str {
        self.method.as_ref()
    }

    fn params(&self) -> &Params {
        &self.params
    }

    fn headers(&self) -> &Headers {
        &self.headers
    }

    #[allow(unused)]
    fn body(&self) -> Option<&Bytes> {
        self.body.as_ref()
    }
}

fn build_request(
    canon_file_path: impl AsRef<Path>,
    header_files: Option<Vec<String>>,
    interpolate: bool,
) -> KuiperResult<FinishedRequest> {
    let canon_file_path = canon_file_path.as_ref();
    let RequestFile {
        mut uri,
        method,
        headers,
        body_file,
        mut params,
    } = get_request_file(canon_file_path)?;

    // Evaluate headers from root to request, plus additional header files
    let mut headers = headers::get_headers(canon_file_path, &headers, header_files)?;

    let body = if let Some(bp) = body_file {
        let mut b = canon_file_path.to_path_buf();
        b.pop(); // Remove the request file 
        b.push(bp);
        body::get_body(&b)?
    } else {
        None
    };

    let mut body_bytes = None;
    if interpolate {
        uri = interpolate_uri(&uri)?;
        interpolate_params(&mut params)?;
        interpolate_headers(&mut headers)?;
        if let Some(b) = body {
            body_bytes = Some(interpolate_body(&b)?.into_bytes().into());
        }
    }

    Ok(FinishedRequest::new(
        uri,
        Method::from_str(&method).unwrap(),
        params,
        headers,
        body_bytes,
    ))
}

#[derive(Debug)]
enum Error {
    Reqwest(reqwest::Error),
    Dotenv(dotenv::Error),
    IO(std::io::Error),
    Interpolation(InterpolationError),
    Json(serde_json::Error),
}

impl std::error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Error::Reqwest(error) => format!("reqwest error: '{}'", error),
                Error::Dotenv(error) => format!("dotenv error: '{}'", error),
                Error::IO(error) => format!("I/O error: '{}'", error),
                Error::Interpolation(interpolation_error) => format!("{}", interpolation_error),
                Error::Json(error) => format!("json error: '{}'", error),
            }
        )
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

impl From<InterpolationError> for Error {
    fn from(value: InterpolationError) -> Self {
        Self::Interpolation(value)
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::Timestamp;
    use std::{collections::HashMap, fmt::Debug, hash::Hash, path::Path};
    use test_log::test;
    use uuid::Uuid;

    fn make_params(pairs: impl IntoIterator<Item = (&'static str, &'static str)>) -> Params {
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn make_headers(
        pairs: impl IntoIterator<Item = (&'static str, Option<&'static str>)>,
    ) -> Headers {
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.map(|s| s.to_string())))
            .collect()
    }

    fn assert_requests_are_equal(actual: &FinishedRequest, expected: &FinishedRequest) {
        assert_eq!(actual.uri(), expected.uri(), "uri");
        assert_eq!(actual.method(), expected.method(), "method");
        assert_hash_map_eq(actual.params(), expected.params());
        assert_hash_map_eq(actual.headers(), expected.headers());
        assert_eq!(actual.body(), expected.body(), "body");
    }

    fn assert_hash_map_eq<K, V>(left: &HashMap<K, V>, right: &HashMap<K, V>)
    where
        K: Hash + Eq + Debug,
        V: Debug + PartialEq,
    {
        assert_eq!(
            left.len(),
            right.len(),
            "left: {:#?}, right: {:#?}",
            left,
            right
        );
        for (left_key, left_value) in left {
            let (right_key, right_value) = right
                .get_key_value(left_key)
                .unwrap_or_else(|| panic!("right HashMap does not contain key '{:?}'", left_key));
            assert_eq!(left_key, right_key);
            assert_eq!(
                left_value, right_value,
                "HashMaps differ at key '{:?}', left: '{:?}', right: '{:?}'",
                left_key, left_value, right_value
            );
        }
    }

    #[test]
    fn ancestors_rev_test() {
        let path = PathBuf::from("x/y/z/f.kuiper");
        let v: Vec<_> = path.ancestors().collect();
        let reversed: Vec<_> = v.into_iter().skip(1).rev().skip(1).collect();

        assert_eq!(
            reversed,
            vec![Path::new("x"), Path::new("x/y"), Path::new("x/y/z"),]
        );
    }

    #[test]
    fn root_request_test() {
        let request = build_request("requests/request_in_root.json", None, true).unwrap();

        let expected = FinishedRequest::new(
            "http://www.example.com".to_string(),
            reqwest::Method::GET,
            make_params([]),
            make_headers([
                ("root_header_1", Some("root_value_1")),
                ("root_header_2", Some("root_value_2")),
                ("root_header_3", None),
            ]),
            None,
        );

        assert_requests_are_equal(&request, &expected);
    }

    #[test]
    fn subdir_request_test() {
        dotenv::from_path("requests/example.env").unwrap();

        let request_path = "requests/subdir/request_in_subdir.json";
        let request = build_request(request_path, None, true).unwrap();

        let expected = FinishedRequest::new(
            "http://localhost/api/user/1".to_string(),
            reqwest::Method::GET,
            make_params([]),
            make_headers([
                ("root_header_1", Some("root_value_1")),
                ("root_header_2", Some("subdir_value_2")),
                ("root_header_3", Some("root_value_3")),
                ("subdir_header_1", Some("subdir_value_1")),
                (
                    "request_specific_header_1",
                    Some("request_specific_header_value_1"),
                ),
            ]),
            Some("text12345".to_string().into_bytes().into()),
        );

        assert_requests_are_equal(&request, &expected);
    }

    #[test]
    fn interpolation_test() {
        dotenv::from_path("requests/example.env").unwrap();

        let interpolated_request =
            build_request("requests/interpolation.json", None, true).unwrap();

        assert_eq!(interpolated_request.params().len(), 3);
        assert_eq!(interpolated_request.params()["env_1"], "123");

        // A new Uuid is generated every time the test is ran,
        // so just assert that it is a Uuids
        assert!(
            interpolated_request.params()["expr_uuid"]
                .parse::<Uuid>()
                .is_ok()
        );
        assert!(
            interpolated_request.params()["expr_now"]
                .parse::<Timestamp>()
                .is_ok()
        );

        assert_eq!(interpolated_request.uri(), "http://localhost/route_value");

        let expected_headers: HashMap<String, Option<String>> = [
            ("root_header_1", Some("root_value_1")),
            ("root_header_2", Some("root_value_2")),
            ("root_header_3", Some("root_value_3")),
            ("interpolated_header", Some("1234")),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.map(|v| v.to_string())))
        .collect();
        assert_hash_map_eq(&interpolated_request.headers(), &expected_headers);
    }

    #[test]
    fn additional_header_file_test() {
        dotenv::from_path("requests/example.env").unwrap();

        let request_path = "requests/subdir/request_in_subdir.json";
        let additional_header = "requests/additional_header_file.json".to_string();

        let request = build_request(request_path, Some(vec![additional_header]), true).unwrap();

        // The request should have the "additional_header",
        // but _not_ have the "root_header_3", since that is disabled in the additional header
        // file.
        let expected = FinishedRequest::new(
            "http://localhost/api/user/1".to_string(),
            reqwest::Method::GET,
            make_params([]),
            make_headers([
                ("root_header_1", Some("root_value_1")),
                ("root_header_2", Some("subdir_value_2")),
                ("subdir_header_1", Some("subdir_value_1")),
                (
                    "request_specific_header_1",
                    Some("request_specific_header_value_1"),
                ),
                ("additional_header", Some("123")),
                ("root_header_3", None),
            ]),
            Some("text12345".to_string().into_bytes().into()),
        );

        assert_requests_are_equal(&request, &expected);
    }
}
