use bytes::Bytes;
use jiff::Timestamp;
use log::{error, trace};
use serde::Deserialize;
use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
    fmt::Display,
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub type Headers = HashMap<String, Option<String>>;
pub type Params = HashMap<String, String>;
pub type KuiperResult<T> = Result<T, KuiperError>;

#[derive(Deserialize, Debug)]
pub struct RequestFile {
    uri: String,
    method: String,
    headers: Headers,
    params: Params,
    body: Option<String>,
}

fn interpolate_params(params: &mut Params) -> KuiperResult<()> {
    for (_, value) in params.iter_mut() {
        let new_value = interpolate_str(&value)?;
        *value = new_value;
    }
    Ok(())
}

fn interpolate_headers(headers: &mut Headers) -> KuiperResult<()> {
    for (_, value) in headers.iter_mut() {
        if let Some(v) = value {
            let new_value = interpolate_str(&v.clone())?;
            *v = new_value;
        }
    }

    Ok(())
}

fn interpolate_str(input: &str) -> KuiperResult<String> {
    let mut result = input.to_owned();
    for (start_idx, _) in input.match_indices("{{") {
        let (end_idx, _) = input[start_idx..]
            .match_indices("}}")
            .next()
            .ok_or(InterpolationError::InvalidFormat)?;
        let interpolated_name = &input[start_idx + 2..start_idx + end_idx];

        let (interpolation_type, name) = interpolated_name
            .split_once(':')
            .ok_or(InterpolationError::InvalidFormat)?;

        let value = match interpolation_type {
            "env" => std::env::var(name)
                .map_err(|_| InterpolationError::MissingEnvVar(name.to_string()))?,
            "expr" => interpolation_expr(name)?,
            s => {
                error!(
                    "parsing Request from file failed, tried to interpolate the following '{}'",
                    s
                );
                return Err(InterpolationError::InvalidFormat.into());
            }
        };

        result = result.replace(&input[start_idx..start_idx + end_idx + 2], &value);
    }

    Ok(result)
}

fn interpolation_expr(expr: &str) -> KuiperResult<String> {
    match expr {
        "uuid" => Ok(Uuid::new_v4().to_string()),
        "now" => Ok(Timestamp::now().to_string()),
        invalid => Err(KuiperError::InvalidExpr(invalid.to_string())),
    }
}

impl RequestFile {
    pub fn find(path: impl Into<PathBuf>) -> KuiperResult<(Self, Option<File>)> {
        let mut path: PathBuf = path.into();
        trace!("finding request at '{path:?}");
        if path.is_relative() {
            path = path.canonicalize()?;
            trace!("request is at '{path:?}'");
            // return Err(KuiperError::PathError);
        }

        let mut request = Self::from_file(&path)?;
        let ancestors: Vec<_> = path.ancestors().collect();
        let mut headers = Headers::new();
        for subdir in ancestors.into_iter().skip(1).rev().skip(1) {
            overwrite_headers(&subdir.join("headers.json"), &mut headers)?;
        }

        for (name, value) in headers {
            request.add_header_if_not_exists(name, value);
        }

        let mut body_file = None;
        if let Some(body_path) = &request.body {
            let mut body_dir = path.parent().unwrap_or(Path::new("/")).to_path_buf();
            body_dir.push(body_path);
            path = body_dir.canonicalize()?;
            body_file = Some(File::open(path)?);
        }

        Ok((request, body_file))
    }

    fn from_file(path: &Path) -> KuiperResult<Self> {
        let file = File::open(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => KuiperError::RequestNotFound,
            _ => e.into(),
        })?;
        let reader = BufReader::new(file);
        let request: RequestFile = serde_json::from_reader(reader)?;
        trace!("successfully parsed request at '{path:?}'");
        Ok(request)
    }

    fn add_header_if_not_exists(&mut self, header_name: String, header_value: Option<String>) {
        if let Entry::Vacant(vacant_entry) = self.headers.entry(header_name) {
            vacant_entry.insert(header_value);
        }
    }
}

pub struct Request {
    uri: String,
    headers: Headers,
    params: HashMap<String, String>,
    method: String,
    body: Option<Bytes>,
}

impl Request {
    pub fn new(
        uri: String,
        headers: Headers,
        params: HashMap<String, String>,
        method: String,
        body: Option<Bytes>,
    ) -> Self {
        Self {
            uri,
            headers,
            params,
            method,
            body,
        }
    }

    pub fn from_file(path: impl Into<PathBuf>) -> KuiperResult<Self> {
        let (request_file, body_file) = RequestFile::find(path)?;
        let RequestFile {
            uri,
            method,
            mut headers,
            mut params,
            body: _,
        } = request_file;

        let uri = interpolate_str(&uri)?;
        let method = method;
        interpolate_headers(&mut headers)?;
        interpolate_params(&mut params)?;
        let mut request = Request::new(uri, headers, params, method, None);
        if let Some(mut f) = body_file {
            let mut buf = String::new(); // TODO: capacity
            f.read_to_string(&mut buf)?;
            let body_interp = interpolate_str(&buf)?;
            request.body = Some(body_interp.into_bytes().into());
        }

        Ok(request)
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    pub fn params(&self) -> &HashMap<String, String> {
        &self.params
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn body(self) -> Option<Bytes> {
        self.body
    }
}

fn overwrite_headers(path: &Path, headers: &mut Headers) -> KuiperResult<()> {
    match File::open(path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            let file_headers: Headers = serde_json::from_reader(reader)?;
            for (name, value) in file_headers {
                // TODO: handle interpolation
                headers.insert(name.to_owned(), value.to_owned());
            }
        }
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => return Ok(()),
            _ => return Err(KuiperError::IoError(e)),
        },
    }
    trace!("successfully parsed headers at '{path:?}");
    Ok(())
}

/// Various errors that can occur.
#[derive(Debug)]
pub enum KuiperError {
    /// IO error.
    IoError(std::io::Error),
    /// JSON error.
    JsonError(serde_json::Error),
    /// Failed to find the request file.
    RequestNotFound,
    /// Request file had the wrong format.
    FileFormatError,
    PathError,
    /// Request file contained an invalid expression.
    InvalidExpr(String),
    /// Request file contained an invalid interpolation.
    InterpolationError(InterpolationError),
}

impl Error for KuiperError {}

impl Display for KuiperError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                KuiperError::IoError(error) => format!("I/O error: {error}"),
                KuiperError::JsonError(error) => format!("JSON error: {error}"),
                KuiperError::RequestNotFound => "request not found".to_string(),
                KuiperError::InterpolationError(error) =>
                    format!("interpolation error '{}'", error),
                KuiperError::FileFormatError => "file format error".to_string(),
                KuiperError::PathError => "path error".to_string(),
                KuiperError::InvalidExpr(expr) => format!("invalid expr: '{}'", expr),
            }
        )
    }
}

impl From<std::io::Error> for KuiperError {
    fn from(value: std::io::Error) -> Self {
        Self::IoError(value)
    }
}

impl From<serde_json::Error> for KuiperError {
    fn from(value: serde_json::Error) -> Self {
        Self::JsonError(value)
    }
}

impl From<InterpolationError> for KuiperError {
    fn from(value: InterpolationError) -> Self {
        Self::InterpolationError(value)
    }
}

/// Various errors that can occur when interpolating expressions.
#[derive(Debug)]
pub enum InterpolationError {
    /// Environment variable is missing.
    MissingEnvVar(String),
    /// Interpolation has an invalid format.
    InvalidFormat,
}

impl Error for InterpolationError {}

impl Display for InterpolationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                InterpolationError::MissingEnvVar(var) => format!("missing env var: '{var}'"),
                InterpolationError::InvalidFormat => "invalid interpolation format".to_string(),
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fmt::Debug, hash::Hash, path::Path};
    use test_log::test;

    fn assert_hash_map_eq<K, V>(left: &HashMap<K, V>, right: &HashMap<K, V>)
    where
        K: Hash + Eq + Debug,
        V: Debug + PartialEq,
    {
        assert_eq!(left.len(), right.len());
        for (left_key, left_value) in left {
            let (right_key, right_value) = right
                .get_key_value(left_key)
                .unwrap_or_else(|| panic!("right HashMap does not contain key '{:?}'", left_key));
            assert_eq!(left_key, right_key);
            assert_eq!(
                left_value, right_value,
                "headers differ at key '{:?}', left: '{:?}', right: '{:?}'",
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
        let request = Request::from_file("requests/request_in_root.kuiper").unwrap();
        assert_eq!(request.uri(), "http://www.example.com");
        let expected_headers: Headers = [
            ("root_header_1", Some("root_value_1")),
            ("root_header_2", Some("root_value_2")),
            ("root_header_3", None),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.map(|s| s.to_string())))
        .collect();

        assert_hash_map_eq(request.headers(), &expected_headers);
    }

    #[test]
    fn subdir_request_test() {
        let request = Request::from_file("requests/subdir/request_in_subdir.kuiper").unwrap();
        assert_eq!(request.uri(), "http://localhost/api/user/1");
        let expected_headers: Headers = [
            ("root_header_1", Some("root_value_1")),
            ("root_header_2", Some("subdir_value_2")),
            ("root_header_3", Some("root_value_3")),
            ("subdir_header_1", Some("subdir_value_1")),
            (
                "request_specific_header_1",
                Some("request_specific_header_value_1"),
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.map(|s| s.to_string())))
        .collect();

        assert_hash_map_eq(request.headers(), &expected_headers);
    }

    #[test]
    fn interpolation_test() {
        dotenv::from_path("requests/example.env").unwrap();
        let interpolated_request = Request::from_file("requests/interpolation.kuiper").unwrap();

        assert_eq!(interpolated_request.params.len(), 3);
        assert_eq!(interpolated_request.params["env_1"], "123");
        // a new Uuid is generated every time the test is ran,
        // so just assert that it is a Uuids
        assert!(interpolated_request.params["expr_uuid"]
            .parse::<Uuid>()
            .is_ok());
        assert!(interpolated_request.params["expr_now"]
            .parse::<Timestamp>()
            .is_ok());

        assert_eq!(interpolated_request.uri, "http://localhost/route_value");

        let expected_headers: HashMap<String, Option<String>> = [
            ("root_header_1", Some("root_value_1")),
            ("root_header_2", Some("root_value_2")),
            ("root_header_3", Some("root_value_3")),
            ("interpolated_header", Some("1234")),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.map(|v| v.to_string())))
        .collect();
        assert_hash_map_eq(&interpolated_request.headers, &expected_headers);
    }

    #[test]
    fn body_interpolation_test() {
        dotenv::from_path("requests/example.env").unwrap();
        let request = Request::from_file("requests/subdir/request_in_subdir.kuiper").unwrap();
        let body = request.body().unwrap();

        let s = String::from_utf8(body.to_vec()).unwrap();
        assert!(s.contains("12345"));
    }

    #[test]
    fn interpolation_error_test() {
        let result = interpolate_str("asd{{env:{{env:abc}}");
        assert!(
            matches!(&result, Err(KuiperError::InterpolationError(InterpolationError::MissingEnvVar(var))) if var == "{{env:abc"),
            "{:?}",
            result
        );

        let result = interpolate_str("{{e{{nv:hello}}}}");
        assert!(
            matches!(
                &result,
                Err(KuiperError::InterpolationError(
                    InterpolationError::InvalidFormat
                ))
            ),
            "{:?}",
            result
        );
    }
}
