use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
    fmt::Display,
    fs,
    path::PathBuf,
};

pub type Requests = HashMap<String, Request>;
pub type Headers = HashMap<String, String>;
pub type Env = HashMap<String, String>;
pub type KuiperResult<T> = Result<T, KuiperError>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Request {
    uri: String,
    headers: Headers,
    params: Value,
    method: String,
    body: Option<Vec<u8>>,
}

impl Request {
    fn add_header_if_not_exists(&mut self, header_name: String, header_value: String) {
        if let Entry::Vacant(vacant_entry) = self.headers.entry(header_name) {
            vacant_entry.insert(header_value);
        }
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
}

pub fn evaluate_requests(path: PathBuf) -> KuiperResult<Requests> {
    evaluate_dir(path, Headers::new(), Env::new())
}

fn evaluate_dir(path: PathBuf, mut headers: Headers, _env: Env) -> KuiperResult<Requests> {
    // look for a header file in the dir
    read_headers(path.join("headers.json"), &mut headers)?;

    let mut requests = HashMap::new(); // TODO: capacity

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            for (name, value) in evaluate_dir(path, headers.clone(), _env.clone())? {
                requests.insert(name, value);
            }
        } else if let Some(ext) = path.extension() {
            if ext == "kuiper" {
                println!("found request: {:?}", path);
                let file_contents = fs::read_to_string(&path)?;
                let mut request: Request = serde_json::from_str(&file_contents)?;
                // insert headers
                for (header_name, header_value) in headers.clone() {
                    request.add_header_if_not_exists(header_name, header_value);
                }
                requests.insert(
                    path.file_name().unwrap().to_str().unwrap().to_owned(), // TODO: this looks like shit
                    request,
                );
            }
        }
    }

    Ok(requests)
}

fn read_headers(path: PathBuf, headers: &mut Headers) -> KuiperResult<()> {
    if let Ok(header_file_contents) = fs::read_to_string(path) {
        // missing header file simply means we dont add any headers
        let file_headers: Headers = serde_json::from_str(&header_file_contents)?;

        for (name, value) in file_headers {
            // TODO: handle interpolation
            headers.insert(name.to_owned(), value.to_owned());
        }
    }
    Ok(())
}

#[derive(Debug)]
pub enum KuiperError {
    IoError(std::io::Error),
    JsonError(serde_json::Error),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_test_dir() {
        let dir = "../requests";

        let requests = evaluate_requests(dir.into()).unwrap();

        println!("{}", serde_json::to_string_pretty(&requests).unwrap());

        {
            // `request_in_root.kuiper` should have headers from `requests/`

            let request_in_root = requests.get("request_in_root.kuiper").unwrap();
            assert_eq!(
                request_in_root.headers().len(),
                2,
                "there are two headers in request_in_root.kuiper"
            );
            assert_eq!(
                request_in_root.headers().get("root_header_1").unwrap(),
                "root_value_1"
            );
            assert_eq!(
                request_in_root.headers().get("root_header_2").unwrap(),
                "root_value_2"
            );
        }

        {
            // `request_in_subdir.kuiper`
            // root_header_1 from `requests/headers.json`
            // root_header_2 from `requests/headers.json`, overwritten by `requests/subdir/headers.json`
            // subdir_header_1 from `requests/subdir/headers.json`
            // request_specific_header_1 from `requests/subdir/request_in_subdir.kuiper`

            let request_in_subdir = requests.get("request_in_subdir.kuiper").unwrap();
            assert_eq!(
                request_in_subdir.headers().len(),
                4,
                "there are four headers in request_in_subdir.kuiper"
            );
            assert_eq!(
                request_in_subdir.headers().get("root_header_1").unwrap(),
                "root_value_1"
            );
            assert_eq!(
                request_in_subdir.headers().get("root_header_2").unwrap(),
                "subdir_value_2"
            );
            assert_eq!(
                request_in_subdir.headers().get("subdir_header_1").unwrap(),
                "subdir_value_1"
            );
            assert_eq!(
                request_in_subdir
                    .headers()
                    .get("request_specific_header_1")
                    .unwrap(),
                "request_specific_header_value_1"
            );
        }
    }
}
