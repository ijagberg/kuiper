use log::trace;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
    fmt::Display,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

pub type Headers = HashMap<String, Option<String>>;
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
    pub fn find(path: PathBuf) -> KuiperResult<Self> {
        trace!("finding request at '{path:?}");
        if !path.is_relative() {
            return Err(KuiperError::PathError);
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

    fn from_file(path: &Path) -> KuiperResult<Self> {
        let file = File::open(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => KuiperError::RequestNotFound,
            _ => e.into(),
        })?;
        let reader = BufReader::new(file);
        let request: Request = serde_json::from_reader(reader)?;
        trace!("successfully parsed request at '{path:?}'");
        Ok(request)
    }

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

    fn interpolate(&mut self) -> KuiperResult<()> {
        let new_url = interpolate(&self.uri)?;
        for (_, value) in self.headers.iter_mut() {
            if let Some(v) = value {
                let new_value = interpolate(&v.clone())?;
                *v = new_value;
            }
        }
        self.uri = new_url;

        if let Some(body) = &self.body {
            let s = body.to_string();
            let new_body_s = interpolate(&s)?;
            self.body = serde_json::from_str(&new_body_s)?;
        }

        // TODO: params
        trace!("successfully interpolated request");
        Ok(())
    }

    pub fn body(&self) -> Option<&Value> {
        self.body.as_ref()
    }
}

fn overwrite_headers(path: &Path, headers: &mut Headers) -> KuiperResult<()> {
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
    trace!("successfully parsed headers at '{path:?}");
    Ok(())
}

fn interpolate(input: &str) -> KuiperResult<String> {
    let mut result = input.to_owned();
    for (start_idx, _) in input.match_indices("${{") {
        let (end_idx, _) = input[start_idx..]
            .match_indices("}}")
            .next()
            .ok_or(KuiperError::InterpolationError)?;
        let interpolated_name = &input[start_idx + 3..start_idx + end_idx];
        let interpolated_value =
            std::env::var(interpolated_name).map_err(|_| KuiperError::InterpolationError)?;
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
    RequestNotFound,
    InterpolationError,
    FileFormatError,
    PathError,
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
                KuiperError::RequestNotFound => format!("request not found"),
                KuiperError::InterpolationError => format!("interpolation error"),
                KuiperError::FileFormatError => format!("file format error"),
                KuiperError::PathError => format!("path error"),
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
    use std::path::Path;

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
}
