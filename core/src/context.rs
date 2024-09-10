//! The context logic of the Arcana Templating Engine
// Copyright (C) 2024  Frankie Baffa
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>

use {
    crate::{
        error::{
            Error,
            Result,
        },
        file::read_file,
        path::clean_path,
    },
    std::{
        collections::HashMap,
        fmt::{
            Display,
            Formatter,
            Result as FmtResult,
        },
        path::{
            Path,
            PathBuf,
        },
        slice::Iter,
    },
    serde_json::{
        from_str as from_json_str,
        Value as JsonValue,
        Map as JsonMap,
    },
};

const SCOPESEP: char = '.';

/// A path to a defined variable.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub
struct Alias {
    scope: Vec<String>,
}

struct AliasIter<'a> {
    iter: Iter<'a, String>,
    up_to: Vec<String>,
}

struct AliasIterItem {
    segment: String,
    alias: Alias,
}

impl<S: AsRef<str>> From<S> for Alias {
    fn from(input: S) -> Self {
        let i = input.as_ref();

        let scope = if i == crate::parser::consts::ROOT {
            vec![]
        }
        else {
            i.split(SCOPESEP)
                .map(|seg| seg.to_owned())
                .collect::<Vec<String>>()
        };

        Self { scope, }
    }
}

impl<'a> Alias {
    fn iter(&'a self) -> AliasIter<'a> {
        AliasIter::<'a> {
            iter: self.scope.iter(),
            up_to: Vec::new(),
        }
    }
}

impl<'a> Iterator for AliasIter<'a> {
    type Item = AliasIterItem;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(segment) = self.iter.next() {
            self.up_to.push(segment.to_owned());

            return Some(AliasIterItem {
                segment: segment.to_owned(),
                alias: self.up_to.join(".").into(),
            });
        }

        None
    }
}

impl Alias {
    fn reversed(&self) -> Self {
        let mut cl = self.clone();
        cl.scope.reverse();
        cl
    }
}

impl Display for Alias {
    fn fmt(&self, fmtr: &mut Formatter<'_>) -> FmtResult {
        fmtr.write_str(&self.scope.join("."))
    }
}

#[derive(Clone, Debug)]
pub
struct JsonContext {
    properties: JsonValue,
    scoped_paths: HashMap<Alias, PathBuf>,
}

impl JsonContext {
    pub(crate)
    fn faux_context<P: AsRef<Path>>(p: P) -> Result<Self> {
        let p = clean_path(p);

        if p.is_relative() {
            return Err(Error::IllegalRelativePath(p));
        }
        else if p.is_dir() {
            return Err(Error::IllegalDirPath(p));
        }

        let mut scoped_paths = HashMap::new();
        let mut dir: PathBuf = p.clone();
        dir.pop();
        scoped_paths.insert(Alias::default(), dir);

        Ok(Self {
            properties: JsonValue::Object(JsonMap::new()),
            scoped_paths,
        })
    }

    pub(crate)
    fn parse_json<P: AsRef<Path>, S: AsRef<str>>(path: P, source: S) -> Result<JsonValue> {
        let p: PathBuf = path.as_ref().into();
        from_json_str::<JsonValue>(source.as_ref()).map_err(|e| Error::JsonParse(e, p))
    }

    pub(crate)
    fn read_from_string<P: AsRef<Path>, S: AsRef<str>, A: Into<Alias>>(path: P, source: S, alias: Option<A>) -> Result<Self> {
        let p: PathBuf = path.as_ref().into();

        let mut properties = Self::parse_json(path, source)?;

        if !matches!(properties, JsonValue::Object(_)) {
            return Err(Error::NotAMap(p));
        };

        if let Some(alias) = alias {
            let a: Alias = alias.into();
            let reversed = a.reversed();

            for item in reversed.iter() {
                properties = JsonValue::Object({
                    let mut new_map = JsonMap::new();
                    new_map.insert(item.segment, properties);
                    new_map
                });
            }
        }

        let mut scoped_paths = HashMap::new();
        let mut dir: PathBuf = p.clone();
        dir.pop();
        scoped_paths.insert(Alias::default(), dir);

        Ok(Self {
            properties,
            scoped_paths,
        })
    }

    fn read_internal<P: AsRef<Path>, A: Into<Alias>>(p: P, alias: Option<A>) -> Result<Self> {
        let p = clean_path(p);

        if p.is_relative() {
            return Err(Error::IllegalRelativePath(p));
        }
        else if p.is_dir() {
            return Err(Error::IllegalDirPath(p));
        }

        let file = read_file(&p)?;

        Self::read_from_string(p, file, alias)
    }

    pub
    fn read<P: AsRef<Path>>(p: P) -> Result<Self> {
        Self::read_internal::<P, Alias>(p, None)
    }

    pub(crate)
    fn merge<P>(&mut self, source_path: P, ctx: JsonContext) -> Result<()>
    where
        P: AsRef<Path>
    {
        let ctx_map = if let JsonValue::Object(map) = ctx.properties {
            map
        }
        else {
            return Err(Error::NotAMap(source_path.as_ref().into()));
        };

        let path_to_scope = if let Some(path) = ctx.scoped_paths.get(&Alias::default()) {
            path
        }
        else {
            return Err(Error::NoScopedPath(Alias::default()));
        };

        for (k, v) in ctx_map.into_iter() {
            self.scoped_paths.insert(k.clone().into(), path_to_scope.to_owned());
            self.properties[&k] = v;
        }

        Ok(())
    }

    fn read_in_internal<P, A>(&mut self, path: P, alias: Option<A>) -> Result<()>
    where
        P: AsRef<Path>,
        A: Into<Alias>
    {
        let ctx = Self::read_internal(path.as_ref(), alias)?;

        self.merge(path, ctx)
    }

    pub(crate)
    fn read_in<P: AsRef<Path>>(&mut self, p: P) -> Result<()> {
        self.read_in_internal::<P, Alias>(p, None)
    }

    pub(crate)
    fn read_as<P: AsRef<Path>, A: Into<Alias>>(p: P, alias: A) -> Result<Self> {
        Self::read_internal(p, Some(alias))
    }

    pub(crate)
    fn read_in_as<P, A>(&mut self, p: P, alias: A) -> Result<()>
    where
        P: AsRef<Path>,
        A: Into<Alias>
    {
        self.read_in_internal(p, Some(alias))
    }

    pub(crate)
    fn remove<A>(&mut self, alias: A)
    where
        A: Into<Alias>
    {
        let a: Alias = alias.into();

        let mut value = &mut self.properties;
        let len = a.scope.len();
        for (idx, seg) in a.iter().enumerate() {
            if let None|Some(JsonValue::Null) = value.get(&seg.segment) {
                return;
            }

            if idx != len - 1 {
                value = value.get_mut(&seg.segment).unwrap();
            }
            else if value.is_object() {
                value.as_object_mut().unwrap().remove(&seg.segment);
                return;
            }
        }
    }

    pub(crate)
    fn set_value<A>(&mut self, alias: A, val: JsonValue) -> Result<()>
    where
        A: Into<Alias>
    {
        let a: Alias =  alias.into();

        let mut value = &mut self.properties;
        let len = a.scope.len();
        for (idx, seg) in a.iter().enumerate() {
            // not the last iteration, make sure the segment is an object
            if idx != len - 1 {
                // if the value is not an object
                if !matches!(value.get(&seg.segment), Some(JsonValue::Object(_))) {
                    value.as_object_mut().unwrap()
                        .insert(
                            seg.segment.to_owned(),
                            JsonValue::Object(JsonMap::new())
                        );
                }

                value = value.get_mut(&seg.segment).unwrap();
            }
            // last iteration, set the path value
            else {
                value.as_object_mut().unwrap()
                    .insert(seg.segment.to_owned(), val);
                break;
            }
        }

        Ok(())
    }

    fn get_internal<A: Into<Alias>>(&self, alias: A) -> Result<(&JsonValue, PathBuf)> {
        // default scoped path
        let mut path = self.scoped_paths.get(&Alias::default());

        let a: Alias = alias.into();

        let mut value = &self.properties;
        for item in a.iter() {
            if let Some(abs_path) = self.scoped_paths.get(&item.alias) {
                path = Some(abs_path);
            }

            value = &value[&item.segment];

            if let JsonValue::Null = value {
                break;
            }
        }

        if let Some(abs_path) = path {
            Ok((value, abs_path.to_owned()))
        }
        else {
            Err(Error::NoScopedPath(a))
        }
    }

    pub
    fn get_as_context<A, A2>(&self, alias: A, inner_alias: Option<A2>) -> Result<Self>
    where
        A: Into<Alias>,
        A2: Into<Alias>,
    {
        let (value, path) = self.get_internal(alias)?;

        if let Some(inner_alias) = inner_alias {
            let mut new = self.clone();
            let inner = inner_alias.into();
            new.set_value(inner.clone(), value.clone())?;
            new.scoped_paths.insert(inner.clone(), path);
            return Ok(new);
        }

        let mut scoped_paths = HashMap::new();
        scoped_paths.insert(Alias::default(), path);

        Ok(Self {
            properties: value.clone(),
            scoped_paths,
        })
    }

    pub
    fn get_each_as_context<A, A2>(&self, alias: A, inner_alias: Option<A2>) -> Result<Vec<Self>>
    where
        A: Into<Alias>,
        A2: Into<Alias>,
    {
        let alias: Alias = alias.into();
        let (value, path) = self.get_internal(alias.clone())?;

        if let Some(inner_alias) = inner_alias {
            if let JsonValue::Array(a) = value {
                let inner: Alias = inner_alias.into();

                return a.clone().into_iter()
                    .map(|v| {
                        let mut new = self.clone();
                        new.set_value(inner.clone(), v)?;
                        new.scoped_paths.insert(inner.clone(), path.clone());
                        Ok(new)
                    })
                    .collect::<Result<Vec<Self>>>();
            }
            else {
                return Err(Error::ValueNotArray(alias));
            }
        }

        if let JsonValue::Array(a) = value {
            Ok(a.clone().into_iter()
                .map(|v| {
                    let mut scoped_paths = HashMap::new();
                    scoped_paths.insert(Alias::default(), path.clone());

                    Self {
                        properties: v,
                        scoped_paths,
                    }
                })
                .collect::<Vec<Self>>()
            )
        }
        else {
            Err(Error::ValueNotArray(alias))
        }
    }

    pub(crate)
    fn get_value<A: Into<Alias>>(&self, alias: A) -> Result<&JsonValue> {
        self.get_internal(alias).map(|v| v.0)
    }

    fn normalize_path(mut base: PathBuf, path: PathBuf) -> PathBuf {
        if path.is_absolute() {
            return path;
        }

        base.push(path);

        clean_path(base)
    }

    pub(crate)
    fn get_path_opt<A: Into<Alias>>(&self, alias: A) -> Result<Option<PathBuf>> {
        let a = alias.into();
        let (value, abs_path,) = self.get_internal(a.clone())?;

        if let JsonValue::String(value) = value {
            Ok(Some(Self::normalize_path(abs_path, value.into())))
        }
        else if !value.is_null() {
            Err(Error::ValueNotPath(a))
        }
        else {
            Ok(None)
        }
    }

    pub(crate)
    fn get_path<A: Into<Alias>>(&self, alias: A) -> Result<PathBuf> {
        let a = alias.into();
        self.get_path_opt(a.clone())?.ok_or(Error::ValueNotPath(a))
    }

    pub(crate)
    fn get_stringlike_opt<A: Into<Alias>>(&self, alias: A) -> Result<Option<String>> {
        let a = alias.into();
        let val = self.get_internal(a.clone())?.0;

        match val {
            JsonValue::String(s) => Ok(Some(s.to_owned())),
            JsonValue::Null => Ok(None),
            v => Ok(Some(v.to_string())),
        }
    }

    pub(crate)
    fn get_stringlike<A: Into<Alias>>(&self, alias: A) -> Result<String> {
        let a = alias.into();
        self.get_stringlike_opt(a.clone())?.ok_or(Error::ValueNotString(a))
    }

    fn get_array_internal<A>(&self, alias: A, as_paths: bool, nullable: bool) -> Result<Vec<JsonValue>>
    where
        A: Into<Alias>
    {
        let a = alias.into();
        let (val, abs_path,) = self.get_internal(a.clone())?;

        if let JsonValue::Array(arr) = val {
            if !as_paths {
                Ok(arr.clone())
            }
            else {
                Ok(arr.iter()
                    .map(|p| if let JsonValue::String(p) = p {
                        let path = Self::normalize_path(abs_path.to_owned(), p.into())
                            .to_str()
                            .unwrap_or("")
                            .to_owned();

                        Ok(JsonValue::String(path))
                    }
                    else {
                        Err(Error::ValuesNotPath(a.clone()))
                    })
                    .collect::<Result<Vec<JsonValue>>>()?
                )
            }
        }
        else if let JsonValue::Null = val {
            if nullable {
                Ok(vec![])
            }
            else {
                Err(Error::ValueNotArray(a))
            }
        }
        else {
            Err(Error::ValueNotArray(a))
        }
    }

    pub(crate)
    fn get_array<A>(&mut self, alias: A) -> Result<Vec<JsonValue>>
    where
        A: Into<Alias>
    {
        self.get_array_internal(alias, false, false)
    }

    pub(crate)
    fn get_array_opt<A>(&mut self, alias: A) -> Result<Vec<JsonValue>>
    where
        A: Into<Alias>
    {
        self.get_array_internal(alias, false, true)
    }

    pub(crate)
    fn get_array_as_paths<A>(&mut self, alias: A) -> Result<Vec<JsonValue>>
    where
        A: Into<Alias>
    {
        self.get_array_internal(alias, true, false)
    }

    pub(crate)
    fn get_array_opt_as_paths<A>(&mut self, alias: A) -> Result<Vec<JsonValue>>
    where
        A: Into<Alias>
    {
        self.get_array_internal(alias, true, true)
    }

    pub(crate)
    fn get<A: Into<Alias>>(&self, alias: A) -> Result<&JsonValue> {
        Ok(self.get_internal(alias)?.0)
    }

    pub(crate)
    fn is_empty<A: Into<Alias>>(&self, alias: A) -> Result<bool> {
        match self.get(alias)? {
            JsonValue::Null => Ok(true),
            JsonValue::Object(map) => Ok(map.is_empty()),
            JsonValue::String(s) => Ok(s.is_empty()),
            _ => Ok(false),
        }
    }

    pub(crate)
    fn exists<A: Into<Alias>>(&self, alias: A) -> Result<bool> {
        match self.get(alias)? {
            JsonValue::Null => Ok(false),
            _ => Ok(true),
        }
    }

    pub(crate)
    fn truthy<A: Into<Alias>>(&self, alias: A) -> Result<bool> {
        match self.get(alias)? {
            JsonValue::Null => Ok(false),
            JsonValue::Bool(b) => Ok(*b),
            JsonValue::Number(n) => if n.is_i64() {
                Ok(n.as_i64().unwrap() > 0)
            }
            else if n.is_u64() {
                Ok(n.as_u64().unwrap() > 0)
            }
            else {
                Ok(n.as_f64().unwrap() > 0_f64)
            }
            JsonValue::String(_)|JsonValue::Object(_)|JsonValue::Array(_) => Ok(true),
        }
    }

    pub(crate)
    fn eq<A: Into<Alias>, B: Into<Alias>>(&self, a: A, b: B) -> Result<bool> {
        Ok(self.get(a)?.eq(self.get(b)?))
    }

    pub(crate)
    fn ne<A: Into<Alias>, B: Into<Alias>>(&self, a: A, b: B) -> Result<bool> {
        Ok(self.get(a)?.ne(self.get(b)?))
    }

    pub(crate)
    fn gt<A: Into<Alias>, B: Into<Alias>>(&self, a: A, b: B) -> Result<bool> {
        let a: Alias = a.into();
        let b: Alias = b.into();
        let a_val = self.get(a.clone())?;
        let b_val = self.get(b.clone())?;

        match (a_val, b_val) {
            (JsonValue::String(a), JsonValue::String(b)) => Ok(a.gt(b)),
            (JsonValue::Number(a), JsonValue::Number(b)) => if a.is_i64() && b.is_i64() {
                Ok(a.as_i64().unwrap().gt(&b.as_i64().unwrap()))
            }
            else if a.is_u64() && b.is_u64() {
                Ok(a.as_u64().unwrap().gt(&b.as_u64().unwrap()))
            }
            else {
                Ok(a.as_f64().unwrap().gt(&b.as_f64().unwrap()))
            },
            (JsonValue::Bool(a), JsonValue::Bool(b)) => Ok(a.gt(b)),
            _ => Err(Error::CannotCompare(a, b)),
        }
    }

    pub(crate)
    fn ge<A: Into<Alias>, B: Into<Alias>>(&self, a: A, b: B) -> Result<bool> {
        let a: Alias = a.into();
        let b: Alias = b.into();
        let a_val = self.get(a.clone())?;
        let b_val = self.get(b.clone())?;

        match (a_val, b_val) {
            (JsonValue::String(a), JsonValue::String(b)) => Ok(a.ge(b)),
            (JsonValue::Number(a), JsonValue::Number(b)) => if a.is_i64() && b.is_i64() {
                Ok(a.as_i64().unwrap().ge(&b.as_i64().unwrap()))
            }
            else if a.is_u64() && b.is_u64() {
                Ok(a.as_u64().unwrap().ge(&b.as_u64().unwrap()))
            }
            else {
                Ok(a.as_f64().unwrap().ge(&b.as_f64().unwrap()))
            },
            (JsonValue::Bool(a), JsonValue::Bool(b)) => Ok(a.ge(b)),
            _ => Err(Error::CannotCompare(a, b)),
        }
    }

    pub(crate)
    fn lt<A: Into<Alias>, B: Into<Alias>>(&self, a: A, b: B) -> Result<bool> {
        let a: Alias = a.into();
        let b: Alias = b.into();
        let a_val = self.get(a.clone())?;
        let b_val = self.get(b.clone())?;

        match (a_val, b_val) {
            (JsonValue::String(a), JsonValue::String(b)) => Ok(a.lt(b)),
            (JsonValue::Number(a), JsonValue::Number(b)) => if a.is_i64() && b.is_i64() {
                Ok(a.as_i64().unwrap().lt(&b.as_i64().unwrap()))
            }
            else if a.is_u64() && b.is_u64() {
                Ok(a.as_u64().unwrap().lt(&b.as_u64().unwrap()))
            }
            else {
                Ok(a.as_f64().unwrap().lt(&b.as_f64().unwrap()))
            },
            (JsonValue::Bool(a), JsonValue::Bool(b)) => Ok(a.lt(b)),
            _ => Err(Error::CannotCompare(a, b)),
        }
    }

    pub(crate)
    fn le<A: Into<Alias>, B: Into<Alias>>(&self, a: A, b: B) -> Result<bool> {
        let a: Alias = a.into();
        let b: Alias = b.into();
        let a_val = self.get(a.clone())?;
        let b_val = self.get(b.clone())?;

        match (a_val, b_val) {
            (JsonValue::String(a), JsonValue::String(b)) => Ok(a.le(b)),
            (JsonValue::Number(a), JsonValue::Number(b)) => if a.is_i64() && b.is_i64() {
                Ok(a.as_i64().unwrap().le(&b.as_i64().unwrap()))
            }
            else if a.is_u64() && b.is_u64() {
                Ok(a.as_u64().unwrap().le(&b.as_u64().unwrap()))
            }
            else {
                Ok(a.as_f64().unwrap().le(&b.as_f64().unwrap()))
            },
            (JsonValue::Bool(a), JsonValue::Bool(b)) => Ok(a.le(b)),
            _ => Err(Error::CannotCompare(a, b)),
        }
    }
}
