use std::collections::HashMap;
use std::io::{self, Read, Write, Cursor};

/// NBT (Named Binary Tag) データ形式のパーサー実装
/// Minecraftで使用される圧縮バイナリデータフォーマット

#[derive(Debug, Clone, PartialEq)]
pub enum NbtValue {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(Vec<NbtValue>),
    Compound(HashMap<String, NbtValue>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

#[derive(Debug)]
pub struct NbtTag {
    pub name: String,
    pub value: NbtValue,
}

const TAG_END: u8 = 0;
const TAG_BYTE: u8 = 1;
const TAG_SHORT: u8 = 2;
const TAG_INT: u8 = 3;
const TAG_LONG: u8 = 4;
const TAG_FLOAT: u8 = 5;
const TAG_DOUBLE: u8 = 6;
const TAG_BYTE_ARRAY: u8 = 7;
const TAG_STRING: u8 = 8;
const TAG_LIST: u8 = 9;
const TAG_COMPOUND: u8 = 10;
const TAG_INT_ARRAY: u8 = 11;
const TAG_LONG_ARRAY: u8 = 12;

pub struct NbtReader<R: Read> {
    reader: R,
}

impl<R: Read> NbtReader<R> {
    pub fn new(reader: R) -> Self {
        NbtReader { reader }
    }

    pub fn read_tag(&mut self) -> io::Result<Option<NbtTag>> {
        let tag_type = self.read_u8()?;
        
        if tag_type == TAG_END {
            return Ok(None);
        }

        let name = self.read_string()?;
        let value = self.read_value(tag_type)?;

        Ok(Some(NbtTag { name, value }))
    }

    fn read_value(&mut self, tag_type: u8) -> io::Result<NbtValue> {
        match tag_type {
            TAG_BYTE => Ok(NbtValue::Byte(self.read_i8()?)),
            TAG_SHORT => Ok(NbtValue::Short(self.read_i16()?)),
            TAG_INT => Ok(NbtValue::Int(self.read_i32()?)),
            TAG_LONG => Ok(NbtValue::Long(self.read_i64()?)),
            TAG_FLOAT => Ok(NbtValue::Float(self.read_f32()?)),
            TAG_DOUBLE => Ok(NbtValue::Double(self.read_f64()?)),
            TAG_BYTE_ARRAY => {
                let length = self.read_i32()? as usize;
                let mut data = vec![0i8; length];
                for i in 0..length {
                    data[i] = self.read_i8()?;
                }
                Ok(NbtValue::ByteArray(data))
            }
            TAG_STRING => Ok(NbtValue::String(self.read_string()?)),
            TAG_LIST => {
                let element_type = self.read_u8()?;
                let length = self.read_i32()? as usize;
                let mut list = Vec::with_capacity(length);
                for _ in 0..length {
                    list.push(self.read_value(element_type)?);
                }
                Ok(NbtValue::List(list))
            }
            TAG_COMPOUND => {
                let mut compound = HashMap::new();
                loop {
                    let tag_type = self.read_u8()?;
                    if tag_type == TAG_END {
                        break;
                    }
                    let name = self.read_string()?;
                    let value = self.read_value(tag_type)?;
                    compound.insert(name, value);
                }
                Ok(NbtValue::Compound(compound))
            }
            TAG_INT_ARRAY => {
                let length = self.read_i32()? as usize;
                let mut data = vec![0i32; length];
                for i in 0..length {
                    data[i] = self.read_i32()?;
                }
                Ok(NbtValue::IntArray(data))
            }
            TAG_LONG_ARRAY => {
                let length = self.read_i32()? as usize;
                let mut data = vec![0i64; length];
                for i in 0..length {
                    data[i] = self.read_i64()?;
                }
                Ok(NbtValue::LongArray(data))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unknown NBT tag type: {}", tag_type),
            )),
        }
    }

    fn read_u8(&mut self) -> io::Result<u8> {
        let mut buf = [0u8; 1];
        self.reader.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    fn read_i8(&mut self) -> io::Result<i8> {
        Ok(self.read_u8()? as i8)
    }

    fn read_i16(&mut self) -> io::Result<i16> {
        let mut buf = [0u8; 2];
        self.reader.read_exact(&mut buf)?;
        Ok(i16::from_be_bytes(buf))
    }

    fn read_i32(&mut self) -> io::Result<i32> {
        let mut buf = [0u8; 4];
        self.reader.read_exact(&mut buf)?;
        Ok(i32::from_be_bytes(buf))
    }

    fn read_i64(&mut self) -> io::Result<i64> {
        let mut buf = [0u8; 8];
        self.reader.read_exact(&mut buf)?;
        Ok(i64::from_be_bytes(buf))
    }

    fn read_f32(&mut self) -> io::Result<f32> {
        let mut buf = [0u8; 4];
        self.reader.read_exact(&mut buf)?;
        Ok(f32::from_be_bytes(buf))
    }

    fn read_f64(&mut self) -> io::Result<f64> {
        let mut buf = [0u8; 8];
        self.reader.read_exact(&mut buf)?;
        Ok(f64::from_be_bytes(buf))
    }

    fn read_string(&mut self) -> io::Result<String> {
        let length = self.read_i16()? as usize;
        let mut buf = vec![0u8; length];
        self.reader.read_exact(&mut buf)?;
        String::from_utf8(buf).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("Invalid UTF-8: {}", e))
        })
    }
}

pub struct NbtWriter<W: Write> {
    writer: W,
}

impl<W: Write> NbtWriter<W> {
    pub fn new(writer: W) -> Self {
        NbtWriter { writer }
    }

    pub fn write_tag(&mut self, tag: &NbtTag) -> io::Result<()> {
        let tag_type = self.get_tag_type(&tag.value);
        self.write_u8(tag_type)?;
        self.write_string(&tag.name)?;
        self.write_value(&tag.value)?;
        Ok(())
    }

    fn write_value(&mut self, value: &NbtValue) -> io::Result<()> {
        match value {
            NbtValue::Byte(v) => self.write_i8(*v),
            NbtValue::Short(v) => self.write_i16(*v),
            NbtValue::Int(v) => self.write_i32(*v),
            NbtValue::Long(v) => self.write_i64(*v),
            NbtValue::Float(v) => self.write_f32(*v),
            NbtValue::Double(v) => self.write_f64(*v),
            NbtValue::ByteArray(v) => {
                self.write_i32(v.len() as i32)?;
                for &byte in v {
                    self.write_i8(byte)?;
                }
                Ok(())
            }
            NbtValue::String(v) => self.write_string(v),
            NbtValue::List(v) => {
                if v.is_empty() {
                    self.write_u8(TAG_END)?;
                    self.write_i32(0)?;
                } else {
                    let element_type = self.get_tag_type(&v[0]);
                    self.write_u8(element_type)?;
                    self.write_i32(v.len() as i32)?;
                    for value in v {
                        self.write_value(value)?;
                    }
                }
                Ok(())
            }
            NbtValue::Compound(v) => {
                for (name, value) in v {
                    let tag_type = self.get_tag_type(value);
                    self.write_u8(tag_type)?;
                    self.write_string(name)?;
                    self.write_value(value)?;
                }
                self.write_u8(TAG_END)?;
                Ok(())
            }
            NbtValue::IntArray(v) => {
                self.write_i32(v.len() as i32)?;
                for &int in v {
                    self.write_i32(int)?;
                }
                Ok(())
            }
            NbtValue::LongArray(v) => {
                self.write_i32(v.len() as i32)?;
                for &long in v {
                    self.write_i64(long)?;
                }
                Ok(())
            }
        }
    }

    fn get_tag_type(&self, value: &NbtValue) -> u8 {
        match value {
            NbtValue::Byte(_) => TAG_BYTE,
            NbtValue::Short(_) => TAG_SHORT,
            NbtValue::Int(_) => TAG_INT,
            NbtValue::Long(_) => TAG_LONG,
            NbtValue::Float(_) => TAG_FLOAT,
            NbtValue::Double(_) => TAG_DOUBLE,
            NbtValue::ByteArray(_) => TAG_BYTE_ARRAY,
            NbtValue::String(_) => TAG_STRING,
            NbtValue::List(_) => TAG_LIST,
            NbtValue::Compound(_) => TAG_COMPOUND,
            NbtValue::IntArray(_) => TAG_INT_ARRAY,
            NbtValue::LongArray(_) => TAG_LONG_ARRAY,
        }
    }

    fn write_u8(&mut self, value: u8) -> io::Result<()> {
        self.writer.write_all(&[value])
    }

    fn write_i8(&mut self, value: i8) -> io::Result<()> {
        self.write_u8(value as u8)
    }

    fn write_i16(&mut self, value: i16) -> io::Result<()> {
        self.writer.write_all(&value.to_be_bytes())
    }

    fn write_i32(&mut self, value: i32) -> io::Result<()> {
        self.writer.write_all(&value.to_be_bytes())
    }

    fn write_i64(&mut self, value: i64) -> io::Result<()> {
        self.writer.write_all(&value.to_be_bytes())
    }

    fn write_f32(&mut self, value: f32) -> io::Result<()> {
        self.writer.write_all(&value.to_be_bytes())
    }

    fn write_f64(&mut self, value: f64) -> io::Result<()> {
        self.writer.write_all(&value.to_be_bytes())
    }

    fn write_string(&mut self, value: &str) -> io::Result<()> {
        let bytes = value.as_bytes();
        self.write_i16(bytes.len() as i16)?;
        self.writer.write_all(bytes)
    }
}

// ヘルパー関数
pub fn parse_nbt<R: Read>(reader: R) -> io::Result<Option<NbtTag>> {
    let mut nbt_reader = NbtReader::new(reader);
    nbt_reader.read_tag()
}

pub fn write_nbt<W: Write>(writer: W, tag: &NbtTag) -> io::Result<()> {
    let mut nbt_writer = NbtWriter::new(writer);
    nbt_writer.write_tag(tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_nbt_roundtrip() {
        let mut compound = HashMap::new();
        compound.insert("name".to_string(), NbtValue::String("Test".to_string()));
        compound.insert("x".to_string(), NbtValue::Int(10));
        compound.insert("y".to_string(), NbtValue::Int(64));
        compound.insert("z".to_string(), NbtValue::Int(-5));

        let original_tag = NbtTag {
            name: "Level".to_string(),
            value: NbtValue::Compound(compound),
        };

        // Write NBT
        let mut buffer = Vec::new();
        write_nbt(&mut buffer, &original_tag).unwrap();

        // Read NBT back
        let cursor = Cursor::new(buffer);
        let parsed_tag = parse_nbt(cursor).unwrap().unwrap();

        assert_eq!(original_tag.name, parsed_tag.name);
        assert_eq!(original_tag.value, parsed_tag.value);
    }

    #[test]
    fn test_nbt_list() {
        let list_data = vec![
            NbtValue::Int(1),
            NbtValue::Int(2),
            NbtValue::Int(3),
        ];

        let tag = NbtTag {
            name: "list_test".to_string(),
            value: NbtValue::List(list_data),
        };

        let mut buffer = Vec::new();
        write_nbt(&mut buffer, &tag).unwrap();

        let cursor = Cursor::new(buffer);
        let parsed_tag = parse_nbt(cursor).unwrap().unwrap();

        assert_eq!(tag.name, parsed_tag.name);
        assert_eq!(tag.value, parsed_tag.value);
    }

    #[test]
    fn test_nbt_arrays() {
        let mut compound = HashMap::new();
        compound.insert("byte_array".to_string(), 
                       NbtValue::ByteArray(vec![1, 2, 3, -1, -128]));
        compound.insert("int_array".to_string(), 
                       NbtValue::IntArray(vec![1000, -2000, 0]));
        compound.insert("long_array".to_string(), 
                       NbtValue::LongArray(vec![1000000000000i64, -2000000000000i64]));

        let tag = NbtTag {
            name: "arrays_test".to_string(),
            value: NbtValue::Compound(compound),
        };

        let mut buffer = Vec::new();
        write_nbt(&mut buffer, &tag).unwrap();

        let cursor = Cursor::new(buffer);
        let parsed_tag = parse_nbt(cursor).unwrap().unwrap();

        assert_eq!(tag.name, parsed_tag.name);
        assert_eq!(tag.value, parsed_tag.value);
    }
}