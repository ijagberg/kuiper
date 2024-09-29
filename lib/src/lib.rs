use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{hash_map::Entry, HashMap},
    fs,
    path::PathBuf,
};

pub type Requests = HashMap<PathBuf, Request>;
pub type Headers = HashMap<String, String>;
pub type Env = HashMap<String, String>;
pub type KuiperResult<T> = Result<T, &'static str>;

#[derive(Serialize, Deserialize)]
pub struct Request {
    uri: String,
    headers: Headers,
    params: Value,
    method: String,
}

impl Request {
    fn add_header_if_not_exists(&mut self, header_name: String, header_value: String) {
        if let Entry::Vacant(vacant_entry) = self.headers.entry(header_name) {
            vacant_entry.insert(header_value);
        }
    }
}

pub fn evaluate_requests(path: PathBuf) -> Requests {
    let mut headers = Headers::new();

    read_headers(path.join("headers"), &mut headers);

    let mut requests = Requests::new();

    for entry in fs::read_dir(&path).unwrap() {
        let entry = entry.expect("entry in ReadDir should exist");
        if entry.path().is_dir() {
            let dir_requests = evaluate_dir(entry.path(), headers.clone(), Env::new());
            for (name, value) in dir_requests {
                requests.insert(name, value);
            }
        }
    }

    requests
}

fn evaluate_dir(path: PathBuf, mut headers: Headers, _env: Env) -> Requests {
    // look for a header file in the dir
    read_headers(path.join("headers"), &mut headers);

    let mut requests = HashMap::new(); // TODO: capacity

    for entry in fs::read_dir(path).expect("path given to evaluate_dir should exist") {
        let entry = entry.expect("entry in ReadDir should exist");
        let path = entry.path();
        if let Some(ext) = path.extension() {
            if ext == "kuiper" {
                let file_contents =
                    fs::read_to_string(&path).expect("entry in ReadDir should exist");
                let mut request: Request =
                    serde_json::from_str(&file_contents).expect("file should contain valid json");
                // insert headers
                for (header_name, header_value) in headers.clone() {
                    request.add_header_if_not_exists(header_name, header_value);
                }
                requests.insert(path, request);
            }
        }
    }

    requests
}

fn read_headers(path: PathBuf, headers: &mut Headers) {
    if !fs::exists(&path).unwrap_or(false) {
        return;
    }

    let header_file = fs::read_to_string(path).expect("path given to read_headers should exist");

    for line in header_file.lines() {
        let (name, value) = line
            .split_once('=')
            .expect("line in header file should have two parts when split at '='");
        // TODO: handle interpolation
        headers.insert(name.to_owned(), value.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_test_dir() {
        let dir = "../requests";

        let requests = evaluate_requests(dir.into());

        println!("{}", serde_json::to_string_pretty(&requests).unwrap());

        let get_user = requests
            .iter()
            .find_map(|(k, v)| {
                if k.ends_with("get_user.kuiper") {
                    Some(v)
                } else {
                    None
                }
            })
            .unwrap();

        // the get_user request should have headers from parent directory
        let header = get_user.headers.get("base_header_name_1").unwrap();
        assert_eq!(header, "base_header_value_1");
        let header = get_user.headers.get("base_header_name_2").unwrap();
        assert_eq!(header, "users_specific_value");
        let header = get_user.headers.get("users_specific_header_name").unwrap();
        assert_eq!(header, "asd");
        let header = get_user.headers.get("request_specific_header_1").unwrap();
        assert_eq!(header, "request_specific_header_value_1");
    }
}
