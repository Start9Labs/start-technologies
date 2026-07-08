use std::collections::HashSet;

use imbl_value::imbl::{vector, Vector};
use imbl_value::Value;

pub(super) struct ValueWalker;

impl<'a> ValueWalker {
    pub fn all_with_num(vec: &Vector<&'a Value>, tmp: &mut Vector<&'a Value>, index: f64) {
        Self::walk(vec, tmp, &|v| {
            if v.is_array() {
                v.get(index as usize).map(|item| vector![item])
            } else {
                None
            }
        });
    }

    pub fn all_with_str(
        vec: &Vector<&'a Value>,
        tmp: &mut Vector<&'a Value>,
        key: &str,
        is_filter: bool,
    ) {
        if is_filter {
            Self::walk(vec, tmp, &|v| match v {
                Value::Object(map) if map.contains_key(key) => Some(vector![v]),
                _ => None,
            });
        } else {
            Self::walk(vec, tmp, &|v| match v {
                Value::Object(map) => map.get(key).map(|v| vector![v]),
                _ => None,
            });
        }
    }

    pub fn all(vec: &Vector<&'a Value>, tmp: &mut Vector<&'a Value>) {
        Self::walk(vec, tmp, &|v| match v {
            Value::Array(vec) => Some(vec.iter().collect()),
            Value::Object(map) => {
                let mut tmp = Vector::new();
                for (_, v) in map {
                    tmp.push_back(v);
                }
                Some(tmp)
            }
            _ => None,
        });
    }

    fn walk<F>(vec: &Vector<&'a Value>, tmp: &mut Vector<&'a Value>, fun: &F)
    where
        F: Fn(&Value) -> Option<Vector<&Value>>,
    {
        for v in vec {
            Self::_walk(v, tmp, fun);
        }
    }

    fn _walk<F>(v: &'a Value, tmp: &mut Vector<&'a Value>, fun: &F)
    where
        F: Fn(&Value) -> Option<Vector<&Value>>,
    {
        if let Some(ret) = fun(v) {
            tmp.append(ret);
        }

        match v {
            Value::Array(vec) => {
                for v in vec {
                    Self::_walk(v, tmp, fun);
                }
            }
            Value::Object(map) => {
                for (_, v) in map {
                    Self::_walk(v, tmp, fun);
                }
            }
            _ => {}
        }
    }

    pub fn walk_dedup(
        v: &'a Value,
        tmp: &mut Vector<&'a Value>,
        key: &str,
        visited: &mut HashSet<*const Value>,
    ) {
        match v {
            Value::Object(map) => {
                if map.contains_key(key) {
                    let ptr = v as *const Value;
                    if !visited.contains(&ptr) {
                        visited.insert(ptr);
                        tmp.push_back(v)
                    }
                }
            }
            Value::Array(vec) => {
                for v in vec {
                    Self::walk_dedup(v, tmp, key, visited);
                }
            }
            _ => {}
        }
    }
}
