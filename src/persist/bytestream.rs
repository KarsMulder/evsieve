// SPDX-License-Identifier: GPL-2.0-or-later

//! A miniature serde-like implementation because the metric ton of dependencies serde brings
//! with it are unacceptable in this project.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::io::{Read, Write};

use crate::capability::Capabilities;

trait Serializable where Self: Sized {
    fn serialize(&self, stream: &mut dyn Write) -> std::io::Result<()>;
    fn deserialize(stream: &mut dyn Read) -> std::io::Result<Self>;
}

// We use a macro instead of a template implementation because there is no trait that guarantees
// the existence of to_le_bytes() and from_le_bytes().
macro_rules! impl_serialize_num {
    ($name:ident) => {
        impl Serializable for $name {
            fn serialize(&self, stream: &mut dyn Write) -> std::io::Result<()> {
                stream.write_all(&self.to_le_bytes())
            }
            fn deserialize(stream: &mut dyn Read) -> std::io::Result<Self> {
                let mut buffer: [u8; std::mem::size_of::<Self>()] = [0; std::mem::size_of::<Self>()];
                stream.read_exact(&mut buffer)?;
                Ok(Self::from_le_bytes(buffer))
            }
        }
    }
}

impl_serialize_num!(i64);
impl_serialize_num!(i32);
impl_serialize_num!(i16);
impl_serialize_num!(i8);
impl_serialize_num!(u64);
impl_serialize_num!(u32);
impl_serialize_num!(u16);
impl_serialize_num!(u8);

/// Format: an u64 denoting the length of the array, followed up by the members of the array.
impl<T: Serializable> Serializable for Vec<T> {
    fn serialize(&self, stream: &mut dyn Write) -> std::io::Result<()> {
        (self.len() as u64).serialize(stream)?;
        for item in self {
            item.serialize(stream)?;
        }
        Ok(())
    }
    fn deserialize(stream: &mut dyn Read) -> std::io::Result<Self> {
        let len = u64::deserialize(stream)? as usize;
        let mut result: Vec<T> = Vec::with_capacity(len);
        for _ in 0 .. len {
            result.push(T::deserialize(stream)?);
        }
        Ok(result)
    }
}

/// Format: an u64 denoting the amount of items in the set, followed up by the members of the set.
impl<T: Serializable + Eq + Hash> Serializable for HashSet<T> {
    fn serialize(&self, stream: &mut dyn Write) -> std::io::Result<()> {
        (self.len() as u64).serialize(stream)?;
        for item in self {
            item.serialize(stream)?;
        }
        Ok(())
    }
    fn deserialize(stream: &mut dyn Read) -> std::io::Result<Self> {
        let len = u64::deserialize(stream)? as usize;
        let mut result: HashSet<T> = HashSet::with_capacity(len);
        for _ in 0 .. len {
            let entry = T::deserialize(stream)?;
            if result.contains(&entry) {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                    "HashSet contains multiple copies of the same element."
                ));
            }
            result.insert(entry);
        }
        Ok(result)
    }
}

/// Format: an u64 denoting the amount of bytes in the string, followed up by the string in UTF-8 encoding.
impl Serializable for String {
    fn serialize(&self, stream: &mut dyn Write) -> std::io::Result<()> {
        let bytes_vec: Vec<u8> = self.as_bytes().to_vec();
        bytes_vec.serialize(stream)
    }
    fn deserialize(stream: &mut dyn Read) -> std::io::Result<Self> {
        let bytes_vec: Vec<u8> = Vec::<u8>::deserialize(stream)?;
        String::from_utf8(bytes_vec).map_err(|error| std::io::Error::new(
            std::io::ErrorKind::InvalidData, error
        ))
    }
}

impl<T: Serializable + Eq + Hash, U: Serializable> Serializable for HashMap<T, U> {
    fn serialize(&self, stream: &mut dyn Write) -> std::io::Result<()> {
        (self.len() as u64).serialize(stream)?;
        for (key, value) in self {
            key.serialize(stream)?;
            value.serialize(stream)?;
        }
        Ok(())
    }
    fn deserialize(stream: &mut dyn Read) -> std::io::Result<Self> {
        let len = u64::deserialize(stream)? as usize;
        let mut result: HashMap<T, U> = HashMap::with_capacity(len);
        for _ in 0 .. len {
            let key = T::deserialize(stream)?;
            if result.contains_key(&key) {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                    "HashMap contains multiple copies of the same key."
                ));
            }
            let value = U::deserialize(stream)?;
            result.insert(key, value);
        }
        Ok(result)
    }
}

