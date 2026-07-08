use std::collections::HashSet;

use imbl_value::imbl::Vector;
use imbl_value::Value;
use selector::utils::PathKey;

use super::utils;

pub(super) struct ValueWalker;

impl<'a> ValueWalker {
    pub fn next_all(vec: &Vector<&'a Value>) -> Vector<&'a Value> {
        vec.iter().fold(Vector::new(), |mut acc, v| {
            match v {
                Value::Object(map) => acc.extend(map.values()),
                Value::Array(vec) => acc.extend(vec),
                _ => {}
            }
            acc
        })
    }

    pub fn next_with_str(vec: &Vector<&'a Value>, key: &'a str) -> Vector<&'a Value> {
        vec.iter().fold(Vector::new(), |mut acc, v| {
            if let Value::Object(map) = v {
                if let Some(v) = map.get(key) {
                    acc.push_back(v);
                }
            }
            acc
        })
    }

    pub fn next_with_num(vec: &Vector<&'a Value>, index: f64) -> Vector<&'a Value> {
        vec.iter().fold(Vector::new(), |mut acc, v| {
            if let Value::Array(vec) = v {
                if let Some(v) = vec.get(utils::abs_index(index as isize, vec.len())) {
                    acc.push_back(v);
                }
            }
            acc
        })
    }

    pub fn all_with_num(vec: &Vector<&'a Value>, index: f64) -> Vector<&'a Value> {
        Self::walk(vec, &|v, acc| {
            if v.is_array() {
                if let Some(v) = v.get(index as usize) {
                    acc.push_back(v);
                }
            }
        })
    }

    pub fn all_with_str(vec: &Vector<&'a Value>, key: &'a str) -> Vector<&'a Value> {
        let path_key = utils::to_path_str(key);
        Self::walk(vec, &|v, acc| {
            if let Value::Object(map) = v {
                if let Some(v) = map.get(path_key.get_key()) {
                    acc.push_back(v);
                }
            }
        })
    }

    pub fn all_with_strs(vec: &Vector<&'a Value>, keys: &[&'a str]) -> Vector<&'a Value> {
        let path_keys: &Vec<PathKey> = &keys.iter().map(|key| utils::to_path_str(key)).collect();
        vec.iter().fold(Vector::new(), |mut acc, v| {
            if let Value::Object(map) = v {
                path_keys.iter().for_each(|pk| {
                    if let Some(v) = map.get(pk.get_key()) {
                        acc.push_back(v)
                    }
                });
            }
            acc
        })
    }

    pub fn all(vec: &Vector<&'a Value>) -> Vector<&'a Value> {
        Self::walk(vec, &|v, acc| match v {
            Value::Array(ay) => acc.extend(ay),
            Value::Object(map) => {
                acc.extend(map.values());
            }
            _ => {}
        })
    }

    fn walk<F>(vec: &Vector<&'a Value>, fun: &F) -> Vector<&'a Value>
    where
        F: Fn(&'a Value, &mut Vector<&'a Value>),
    {
        vec.iter().fold(Vector::new(), |mut acc, v| {
            Self::_walk(v, &mut acc, fun);
            acc
        })
    }

    fn _walk<F>(v: &'a Value, acc: &mut Vector<&'a Value>, fun: &F)
    where
        F: Fn(&'a Value, &mut Vector<&'a Value>),
    {
        fun(v, acc);

        match v {
            Value::Array(vec) => {
                vec.iter().for_each(|v| Self::_walk(v, acc, fun));
            }
            Value::Object(map) => {
                map.values()
                    .into_iter()
                    .for_each(|v| Self::_walk(v, acc, fun));
            }
            _ => {}
        }
    }

    pub fn walk_dedup_all<F1, F2>(
        vec: &Vector<&'a Value>,
        key: &str,
        visited: &mut HashSet<*const Value>,
        is_contain: &mut F1,
        is_not_contain: &mut F2,
        depth: usize,
    ) where
        F1: FnMut(&'a Value),
        F2: FnMut(usize),
    {
        vec.iter().enumerate().for_each(|(index, v)| {
            Self::walk_dedup(v, key, visited, index, is_contain, is_not_contain, depth)
        });
    }

    fn walk_dedup<F1, F2>(
        v: &'a Value,
        key: &str,
        visited: &mut HashSet<*const Value>,
        index: usize,
        is_contain: &mut F1,
        is_not_contain: &mut F2,
        depth: usize,
    ) where
        F1: FnMut(&'a Value),
        F2: FnMut(usize),
    {
        let ptr = v as *const Value;
        if visited.contains(&ptr) {
            return;
        }

        match v {
            Value::Object(map) => {
                if map.get(key).is_some() {
                    let ptr = v as *const Value;
                    if !visited.contains(&ptr) {
                        visited.insert(ptr);
                        is_contain(v);
                    }
                } else if depth == 0 {
                    is_not_contain(index);
                }
            }
            Value::Array(vec) => {
                if depth == 0 {
                    is_not_contain(index);
                }
                vec.iter().for_each(|v| {
                    Self::walk_dedup(
                        v,
                        key,
                        visited,
                        index,
                        is_contain,
                        is_not_contain,
                        depth + 1,
                    );
                })
            }
            _ => {
                if depth == 0 {
                    is_not_contain(index);
                }
            }
        }
    }
}
