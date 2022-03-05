use std::ops::Deref;
use std::hash::{Hash, Hasher};
use std::cmp::Ordering;
use once_cell::sync::Lazy;

use regex::Regex;
use pdb::RawString;

#[derive(Debug, Clone, Eq, Ord)]
pub struct Name {
    pub name: String,
    pub ident: String,
    pub generics: Vec<String>,
}

impl<'t> From<RawString<'t>> for Name {
    fn from(name: RawString<'t>) -> Name {
        name.to_string().into_owned().into()
    }
}

impl From<String> for Name {
    fn from(name: String) -> Name {
        let mut generics = Vec::new();
        if let Some((_, inner)) = get_between(&name, '<', '>') {
            for name in split_list(inner) {
                let mut name = name.to_string();
                name = name.trim().to_string();
                // if name.starts_with("enum ") {
                //     name = &name[5..];
                // }
                if name.ends_with(" *") || name.ends_with(" &") {
                    name = name.replace('*', "star");
                    name = name.replace('&', "amp");
                    // let len = name.len();
                    // name = &name[.. len - 2];
                }
                // if name.ends_with(" const") {
                //     let len = name.len();
                //     name = &name[.. len-6];
                // }
                if name.parse::<u64>().is_ok() {
                    // number
                    continue;
                }
                // blacklist of primitive types
                match name.as_str() {
                    "void" | "bool" | "char" | "short" | "int" | "long" | "wchar_t"
                    | "float" | "double" | "unnamed-tag" => continue,
                    name if name.starts_with("unsigned") => continue,
                    // TODO: let's just hope that this catches only function pointers
                    name if name.contains("(") && name.contains(")") => continue,
                    _ => {}
                }
                generics.push(name);
            }
        }
        static RE: Lazy<Regex> = Lazy::new(|| Regex::new("[^a-zA-z0-9]+").unwrap());
        let ident = name.to_string()
            .replace("*", "star")
            .replace("&", "amp");
        let ident = RE.replace_all(ident.to_string().as_ref(), "_").into_owned();
        Name {
            name,
            ident,
            generics,
        }
    }
}

impl Deref for Name {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.name
    }
}

impl PartialEq for Name {
    fn eq(&self, other: &Name) -> bool {
        self.name.eq(&other.name)
    }
}

impl PartialOrd for Name {
    fn partial_cmp(&self, other: &Name) -> Option<Ordering> {
        self.name.partial_cmp(&other.name)
    }
}

impl Hash for Name {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}

/// Returns a tuple of the first index of start and everything between the start and end characters
/// including inner appearances of start and end.
fn get_between(s: &str, start: char, end: char) -> Option<(usize, &str)> {
    let mut level = 1;
    let start_index = s.find(start)?;
    let search_in = &s[start_index+1 ..];
    for (i, c) in search_in.char_indices() {
        match c {
            c if c == start => level += 1,
            c if c == end => level -= 1,
            _ => {},
        }
        if level == 0 {
            assert!(search_in.len() == i + 1 || search_in.chars().nth(i+1) == Some(':'));
            return Some((start_index, &search_in[.. i]));
        }
    }
    None
}

fn split_list(s: &str) -> Vec<&str> {
    let mut level = 0;
    let mut last_split = 0;
    let mut res = Vec::new();
    for (i, c) in s.char_indices() {
        match c {
            '<' => level += 1,
            '>' => level -= 1,
            ',' if level == 0 => {
                res.push(&s[last_split..i]);
                last_split = i + 1;
            }
            _ => {}
        }
    }
    res.push(&s[last_split..]);
    res
}
