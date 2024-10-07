use jiff::Timestamp;
use log::{error, trace};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    error::Error,
    ffi::OsStr,
    fmt::Display,
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub type Headers = HashMap<String, Option<String>>;
pub type KuiperResult<T> = Result<T, KuiperError>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Request {
    #[serde(skip)]
    name: String,
    uri: String,
    headers: Headers,
    params: HashMap<String, String>,
    method: String,
    body: Option<Value>,
}

impl Request {
    pub fn find(path: impl Into<PathBuf>) -> KuiperResult<Self> {
        let mut path: PathBuf = path.into();
        trace!("finding request at '{path:?}");
        if path.is_relative() {
            path = path.canonicalize()?;
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

        request.interpolate()?;

        Ok(request)
    }

    pub fn search(root: impl Into<PathBuf>, term: &str) -> KuiperResult<Vec<Self>> {
        let root: PathBuf = root.into();
        let mut matches = Vec::with_capacity(10);
        let mut dirs = VecDeque::new();
        dirs.push_back(root);
        while let Some(dir) = dirs.pop_front() {
            let contents = fs::read_dir(dir)?;
            for entry in contents {
                let entry = entry?.path();
                if entry.is_dir() {
                    dirs.push_back(entry);
                } else if entry.is_file() && entry.extension().unwrap_or(OsStr::new("")) == "kuiper"
                {
                    let name = entry.to_str().unwrap();
                    if name.contains(term) {
                        matches.push(entry.clone());
                    }
                }
            }
        }

        matches
            .into_iter()
            .map(Self::find)
            .collect::<Result<_, _>>()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    pub fn body(&self) -> Option<&Value> {
        self.body.as_ref()
    }

    pub fn params(&self) -> &HashMap<String, String> {
        &self.params
    }

    fn interpolate(&mut self) -> KuiperResult<()> {
        self.interpolate_uri()?;
        self.interpolate_params()?;
        self.interpolate_headers()?;
        self.interpolate_body()?;
        trace!("successfully interpolated request");
        Ok(())
    }

    fn interpolate_uri(&mut self) -> KuiperResult<()> {
        let new_url = Self::interpolate_str(&self.uri)?;
        self.uri = new_url;

        Ok(())
    }

    fn interpolate_headers(&mut self) -> KuiperResult<()> {
        for (_, value) in self.headers.iter_mut() {
            if let Some(v) = value {
                let new_value = Self::interpolate_str(&v.clone())?;
                *v = new_value;
            }
        }

        Ok(())
    }

    fn interpolate_body(&mut self) -> KuiperResult<()> {
        if let Some(body) = &self.body {
            let s = body.to_string();
            let new_body_s = Self::interpolate_str(&s)?;
            self.body = serde_json::from_str(&new_body_s)?;
        }

        Ok(())
    }

    fn interpolate_params(&mut self) -> KuiperResult<()> {
        for (_name, value) in self.params.iter_mut() {
            *value = Self::interpolate_str(value)?;
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
                "expr" => Self::interpolation_expr(name)?,
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

    fn add_header_if_not_exists(&mut self, header_name: String, header_value: Option<String>) {
        if let Entry::Vacant(vacant_entry) = self.headers.entry(header_name) {
            vacant_entry.insert(header_value);
        }
    }

    fn from_file(path: &Path) -> KuiperResult<Self> {
        let file = File::open(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => KuiperError::RequestNotFound,
            _ => e.into(),
        })?;
        let reader = BufReader::new(file);
        let mut request: Request = serde_json::from_reader(reader)?;
        trace!("successfully parsed request at '{path:?}'");
        request.name = path.to_str().ok_or(KuiperError::PathError)?.to_string();
        Ok(request)
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

#[derive(Debug)]
pub enum KuiperError {
    IoError(std::io::Error),
    JsonError(serde_json::Error),
    RequestNotFound,
    FileFormatError,
    PathError,
    InvalidExpr(String),
    InterpolationError(InterpolationError),
}

impl KuiperError {
    /// Returns `true` if the kuiper error is [`FileFormatError`].
    ///
    /// [`FileFormatError`]: KuiperError::FileFormatError
    #[must_use]
    pub fn is_file_format_error(&self) -> bool {
        matches!(self, Self::FileFormatError)
    }
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

#[derive(Debug)]
pub enum InterpolationError {
    MissingEnvVar(String),
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
        let request = Request::find("../requests/request_in_root.kuiper").unwrap();
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
        let request = Request::find("../requests/subdir/request_in_subdir.kuiper").unwrap();
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
        dotenv::from_path("../requests/example.env").unwrap();
        let interpolated_request = Request::find("../requests/interpolation.kuiper").unwrap();

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
    fn interpolation_error_test() {
        let result = Request::interpolate_str("asd{{env:{{env:abc}}");
        assert!(
            matches!(&result, Err(KuiperError::InterpolationError(InterpolationError::MissingEnvVar(var))) if var == "{{env:abc"),
            "{:?}",
            result
        );

        let result = Request::interpolate_str("{{e{{nv:hello}}}}");
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
