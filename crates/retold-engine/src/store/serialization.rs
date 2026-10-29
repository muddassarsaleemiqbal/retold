//! Hash serialization sorts maps while retaining declared struct fields and sequence order.
use serde::Serialize;
use serde::ser::{self, Serializer};

pub(super) fn bytes<T: Serialize + ?Sized>(value: &T) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(&Ordered(value))
}

struct Ordered<'a, T: ?Sized>(&'a T);

impl<T: Serialize + ?Sized> Serialize for Ordered<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(Sorted(serializer))
    }
}

struct Sorted<S>(S);
struct Compound<S>(S);

macro_rules! primitive {
    ($($method:ident($value:ident: $kind:ty)),* $(,)?) => {$(
        fn $method(self, $value: $kind) -> Result<Self::Ok, Self::Error> { self.0.$method($value) }
    )*};
}

impl<S: Serializer> Serializer for Sorted<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = Compound<S::SerializeSeq>;
    type SerializeTuple = Compound<S::SerializeTuple>;
    type SerializeTupleStruct = Compound<S::SerializeTupleStruct>;
    type SerializeTupleVariant = Compound<S::SerializeTupleVariant>;
    type SerializeMap = Map<S>;
    type SerializeStruct = Compound<S::SerializeStruct>;
    type SerializeStructVariant = Compound<S::SerializeStructVariant>;

    primitive! {
        serialize_bool(value: bool), serialize_i8(value: i8), serialize_i16(value: i16),
        serialize_i32(value: i32), serialize_i64(value: i64), serialize_i128(value: i128),
        serialize_u8(value: u8), serialize_u16(value: u16), serialize_u32(value: u32),
        serialize_u64(value: u64), serialize_u128(value: u128), serialize_f32(value: f32),
        serialize_f64(value: f64), serialize_char(value: char), serialize_str(value: &str),
        serialize_bytes(value: &[u8])
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_none()
    }
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit()
    }
    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit_struct(name)
    }
    fn serialize_unit_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit_variant(name, index, variant)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_some(&Ordered(value))
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_newtype_struct(name, &Ordered(value))
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.0
            .serialize_newtype_variant(name, index, variant, &Ordered(value))
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        self.0.serialize_seq(len).map(Compound)
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.0.serialize_tuple(len).map(Compound)
    }
    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.0.serialize_tuple_struct(name, len).map(Compound)
    }
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        self.0
            .serialize_tuple_variant(name, index, variant, len)
            .map(Compound)
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(Map {
            serializer: self.0,
            entries: Vec::new(),
            key: None,
        })
    }
    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.0.serialize_struct(name, len).map(Compound)
    }
    fn serialize_struct_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        self.0
            .serialize_struct_variant(name, index, variant, len)
            .map(Compound)
    }
    fn collect_str<T: std::fmt::Display + ?Sized>(
        self,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.0.collect_str(value)
    }
    fn is_human_readable(&self) -> bool {
        self.0.is_human_readable()
    }
}

macro_rules! compound {
    ($trait:ident, $method:ident $(, $field:ident: $kind:ty)?) => {
        impl<S: ser::$trait> ser::$trait for Compound<S> {
            type Ok = S::Ok;
            type Error = S::Error;
            fn $method<T: Serialize + ?Sized>(&mut self, $($field: $kind,)? value: &T) -> Result<(), Self::Error> { self.0.$method($($field,)? &Ordered(value)) }
            fn end(self) -> Result<Self::Ok, Self::Error> { self.0.end() }
        }
    };
}

compound!(SerializeSeq, serialize_element);
compound!(SerializeTuple, serialize_element);
compound!(SerializeTupleStruct, serialize_field);
compound!(SerializeTupleVariant, serialize_field);
compound!(SerializeStruct, serialize_field, key: &'static str);
compound!(SerializeStructVariant, serialize_field, key: &'static str);

struct Map<S> {
    serializer: S,
    entries: Vec<(serde_json::Value, Box<serde_json::value::RawValue>)>,
    key: Option<serde_json::Value>,
}

impl<S: Serializer> ser::SerializeMap for Map<S> {
    type Ok = S::Ok;
    type Error = S::Error;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Self::Error> {
        self.key = Some(serde_json::to_value(key).map_err(ser::Error::custom)?);
        Ok(())
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        let key = self
            .key
            .take()
            .ok_or_else(|| ser::Error::custom("Map value has no key"))?;
        let text = serde_json::to_string(&Ordered(value)).map_err(ser::Error::custom)?;
        let raw = serde_json::value::RawValue::from_string(text).map_err(ser::Error::custom)?;
        self.entries.push((key, raw));
        Ok(())
    }

    fn end(mut self) -> Result<Self::Ok, Self::Error> {
        self.entries
            .sort_by(|(left, _), (right, _)| compare_keys(left, right));
        let mut map = self.serializer.serialize_map(Some(self.entries.len()))?;
        for (key, value) in self.entries {
            map.serialize_entry(&key, &value)?;
        }
        map.end()
    }
}

fn compare_keys(left: &serde_json::Value, right: &serde_json::Value) -> std::cmp::Ordering {
    let integers = (integer(left), integer(right));
    if let (Some(left), Some(right)) = integers {
        return left.cmp(&right);
    }
    let left = left
        .as_str()
        .map_or_else(|| left.to_string(), str::to_owned);
    let right = right
        .as_str()
        .map_or_else(|| right.to_string(), str::to_owned);
    left.cmp(&right)
}

fn integer(value: &serde_json::Value) -> Option<i128> {
    value
        .as_i64()
        .map(i128::from)
        .or_else(|| value.as_u64().map(i128::from))
}
