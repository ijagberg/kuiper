use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    ops::{Deref, DerefMut},
    path::Path,
};

use crate::KuiperResult;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Headers(HashMap<String, Option<String>>);

impl Headers {
    pub fn new() -> Self {
        Self(HashMap::new())
    }
}

impl Deref for Headers {
    type Target = HashMap<String, Option<String>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Headers {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for Headers {
    type Item = <HashMap<String, Option<String>> as IntoIterator>::Item;

    type IntoIter = <HashMap<String, Option<String>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Headers {
    type Item = <&'a HashMap<String, Option<String>> as IntoIterator>::Item;

    type IntoIter = <&'a HashMap<String, Option<String>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl FromIterator<(String, Option<String>)> for Headers {
    fn from_iter<T: IntoIterator<Item = (String, Option<String>)>>(iter: T) -> Self {
        Self(HashMap::from_iter(iter))
    }
}

impl Serialize for Headers {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

/// Evaluate headers.
///
/// Overwrites headers in the following order:
/// 1. Check all "headers.json" files on the way to the `request_path`.
/// 2. Check headers in the request file at `request_path`.
/// 3. Read any additional `header_files`.
pub fn get_headers(
    request_path: impl AsRef<Path>,
    request_file_headers: &Headers,
    header_files: Option<Vec<String>>,
) -> KuiperResult<Headers> {
    let mut final_headers = Headers::new();

    // 1
    for h in read_header_files_on_the_way_to(request_path)? {
        overwrite_headers(&h, &mut final_headers);
    }

    // 2
    overwrite_headers(&request_file_headers, &mut final_headers);

    // 3
    if let Some(header_files) = header_files {
        for hf in header_files {
            if let Some(h) = read_header_file(Path::new(&hf))? {
                overwrite_headers(&h, &mut final_headers);
            }
        }
    }

    Ok(final_headers)
}

fn read_header_files_on_the_way_to(path: impl AsRef<Path>) -> KuiperResult<Vec<Headers>> {
    let path = path.as_ref();
    let mut headers = Vec::new();
    let ancestors: Vec<_> = path.ancestors().collect();

    for subdir in ancestors.into_iter().skip(1).rev().skip(1) {
        let hf = subdir.join("headers.json");
        if let Some(header_file) = read_header_file(&hf)? {
            headers.push(header_file);
        }
    }

    Ok(headers)
}

fn read_header_file(path: &Path) -> KuiperResult<Option<Headers>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => return Ok(None),
            _ => return Err(e.into()),
        },
    };
    let reader = BufReader::new(file);
    let headers: Headers = serde_json::from_reader(reader)?;
    Ok(Some(headers))
}

/// Iterate over the pairs in `new_headers` and insert each pair into `headers`, overwriting
/// existing keys.
fn overwrite_headers(new_headers: &Headers, headers: &mut Headers) {
    for (name, value) in new_headers {
        headers.insert(name.to_owned(), value.to_owned());
    }
}
