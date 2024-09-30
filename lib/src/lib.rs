use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
    fmt::Display,
    fs::{self, File},
    io::BufReader,
    path::PathBuf,
};

pub struct Requests {
    _root: String,
    requests: HashMap<String, Request>,
}

impl Requests {
    pub fn evaluate(root: &str, env: Option<String>) -> KuiperResult<Self> {
        let environment = if let Some(env) = env {
            read_env(root.into(), env.as_str())?
        } else {
            Env::new()
        };
        let map = evaluate_dir(root.into(), Headers::new())?;
        let mut stripped_map: HashMap<String, Request> = map
            .into_iter()
            .map(|(key, value)| {
                let without_root = key.strip_prefix(root).unwrap_or(&key);
                let without_slash = without_root.strip_prefix('/').unwrap_or(&without_root);
                let without_ext = without_slash
                    .strip_suffix(".kuiper")
                    .unwrap_or(without_slash);
                (without_ext.to_owned(), value)
            })
            .collect();

        for (_, request) in stripped_map.iter_mut() {
            request.interpolate(&environment)?;
        }

        Ok(Requests {
            _root: root.to_string(),
            requests: stripped_map,
        })
    }

    pub fn get(&self, request_name: &str) -> Option<&Request> {
        self.requests.get(request_name)
    }
}

impl IntoIterator for Requests {
    type Item = <HashMap<String, Request> as IntoIterator>::Item;
    type IntoIter = <HashMap<String, Request> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.requests.into_iter()
    }
}

impl<'a> IntoIterator for &'a Requests {
    type Item = (&'a String, &'a Request);
    type IntoIter = std::collections::hash_map::Iter<'a, String, Request>;

    fn into_iter(self) -> Self::IntoIter {
        self.requests.iter()
    }
}

pub type Headers = HashMap<String, Option<String>>;
pub type Env = HashMap<String, String>;
pub type KuiperResult<T> = Result<T, KuiperError>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Request {
    uri: String,
    headers: Headers,
    params: Value,
    method: String,
    body: Option<Value>,
}

impl Request {
    fn add_header_if_not_exists(&mut self, header_name: String, header_value: Option<String>) {
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

    fn interpolate(&mut self, env: &Env) -> KuiperResult<()> {
        let new_url = interpolate(&self.uri, env)?;
        for (_, value) in self.headers.iter_mut() {
            if let Some(v) = value {
                let new_value = interpolate(&v.clone(), env)?;
                *v = new_value;
            }
        }
        self.uri = new_url;

        if let Some(body) = &self.body {
            let s = body.to_string();
            let new_body_s = interpolate(&s, env)?;
            self.body = serde_json::from_str(&new_body_s)?;
        }
        // TODO: params, body

        Ok(())
    }
    
    pub fn body(&self) -> Option<&Value> {
        self.body.as_ref()
    }
}

fn read_env(dir: PathBuf, env: &str) -> KuiperResult<Env> {
    let file_contents = std::fs::read_to_string(dir.join(format!("{}.env", env)))?;

    let mut env = Env::new();
    for line in file_contents.lines() {
        let (key, value) = line.split_once('=').ok_or(KuiperError::FileFormatError)?;
        env.insert(key.to_owned(), value.to_owned());
    }

    Ok(env)
}

fn evaluate_dir(path: PathBuf, mut headers: Headers) -> KuiperResult<HashMap<String, Request>> {
    // look for a header file in the dir
    read_headers(path.join("headers.json"), &mut headers)?;

    let mut requests = HashMap::new(); // TODO: capacity

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            for (name, value) in evaluate_dir(path, headers.clone())? {
                requests.insert(name, value);
            }
        } else if let Some(ext) = path.extension() {
            if ext == "kuiper" {
                let file = File::open(&path)?;
                let reader = BufReader::new(file);

                let mut request: Request = serde_json::from_reader(reader)?;
                // insert headers
                for (header_name, header_value) in headers.clone() {
                    request.add_header_if_not_exists(header_name, header_value);
                }
                requests.insert(
                    path.to_str().unwrap().to_owned(), // TODO: this looks like shit
                    request,
                );
            }
        }
    }

    Ok(requests)
}

fn read_headers(path: PathBuf, headers: &mut Headers) -> KuiperResult<()> {
    match File::open(&path) {
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
    Ok(())
}

fn interpolate(input: &str, env: &Env) -> KuiperResult<String> {
    let mut result = input.to_owned();
    for (start_idx, _) in input.match_indices("${{") {
        let (end_idx, _) = input[start_idx..]
            .match_indices("}}")
            .next()
            .ok_or(KuiperError::InterpolationError)?;
        let interpolated_name = &input[start_idx + 3..start_idx + end_idx];
        let interpolated_value = env
            .get(interpolated_name)
            .ok_or(KuiperError::InterpolationError)?;
        // println!(
        //     "in '{}', found interpolation '{}', replacing with value '{}'",
        //     input, interpolated_name, interpolated_value
        // );
        result = result.replace(
            &input[start_idx..start_idx + end_idx + 2],
            &interpolated_value,
        );
    }

    Ok(result)
}

#[derive(Debug)]
pub enum KuiperError {
    IoError(std::io::Error),
    JsonError(serde_json::Error),
    RequestNotFound(String),
    InterpolationError,
    FileFormatError,
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
                KuiperError::RequestNotFound(name) => format!("request not found: {name}"),
                KuiperError::InterpolationError => format!("interpolation error"),
                KuiperError::FileFormatError => format!("file format error"),
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

        let requests = Requests::evaluate(dir.into(), None).unwrap();

        println!(
            "{}",
            serde_json::to_string_pretty(&requests.requests).unwrap()
        );

        {
            // `request_in_root.kuiper`
            // root_header_1 from `requests/headers.json`
            // root_header_2 from `requests/headers.json`
            // root_header_3 from `requests/headers.json`,
            //     with value `null` because it is overwritten by `request_in_root.kuiper`

            let request_in_root = requests.get("request_in_root").unwrap();
            assert_eq!(
                request_in_root.headers().len(),
                3,
                "there should be 3 headers in request_in_root.kuiper"
            );
            assert_eq!(
                request_in_root.headers()["root_header_1"].as_ref().unwrap(),
                "root_value_1"
            );
            assert_eq!(
                request_in_root.headers()["root_header_2"].as_ref().unwrap(),
                "root_value_2"
            );
            assert_eq!(request_in_root.headers()["root_header_3"], None);
        }

        {
            // `request_in_subdir.kuiper`
            // root_header_1 from `requests/headers.json`
            // root_header_2 from `requests/headers.json`, overwritten by `requests/subdir/headers.json`
            // root_header_3 from `requests/headers.json`
            // subdir_header_1 from `requests/subdir/headers.json`
            // request_specific_header_1 from `requests/subdir/request_in_subdir.kuiper`

            let request_in_subdir = requests.get("subdir/request_in_subdir").unwrap();
            assert_eq!(
                request_in_subdir.headers().len(),
                5,
                "there should be 5 headers in request_in_subdir.kuiper"
            );
            assert_eq!(
                request_in_subdir.headers()["root_header_1"]
                    .as_ref()
                    .unwrap(),
                "root_value_1"
            );
            assert_eq!(
                request_in_subdir.headers()["root_header_2"]
                    .as_ref()
                    .unwrap(),
                "subdir_value_2"
            );
            assert_eq!(
                request_in_subdir.headers()["root_header_3"]
                    .as_ref()
                    .unwrap(),
                "root_value_3"
            );
            assert_eq!(
                request_in_subdir.headers()["subdir_header_1"]
                    .as_ref()
                    .unwrap(),
                "subdir_value_1"
            );
            assert_eq!(
                request_in_subdir.headers()["request_specific_header_1"]
                    .as_ref()
                    .unwrap(),
                "request_specific_header_value_1"
            );
        }
    }
}
