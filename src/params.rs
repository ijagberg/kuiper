use std::{
    collections::HashMap,
    ops::{Deref, DerefMut, Index},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Params(HashMap<String, String>);

impl Params {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter()
    }
}

impl Deref for Params {
    type Target = HashMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Params {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Index<&str> for Params {
    type Output = String;

    fn index(&self, index: &str) -> &Self::Output {
        &self.0[index]
    }
}

impl IntoIterator for Params {
    type IntoIter = <HashMap<String, String> as IntoIterator>::IntoIter;
    type Item = <HashMap<String, String> as IntoIterator>::Item;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Params {
    type Item = <&'a HashMap<String, String> as IntoIterator>::Item;
    type IntoIter = <&'a HashMap<String, String> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl FromIterator<(String, String)> for Params {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(iter: T) -> Self {
        Self(HashMap::from_iter(iter))
    }
}

impl AsRef<HashMap<String, String>> for Params {
    fn as_ref(&self) -> &HashMap<String, String> {
        &self.0
    }
}

impl From<HashMap<String, String>> for Params {
    fn from(value: HashMap<String, String>) -> Self {
        Self(value)
    }
}

impl Serialize for Params {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}
