extern crate pdb;
extern crate regex;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate failure;

use std::fs::File;
use std::path::Path;
use std::io::Write;
use std::collections::{HashMap, VecDeque};
use std::cmp;
use std::mem;

use regex::Regex;
use pdb::{
    PDB,
    Error as PdbError,
    FallibleIterator,
    TypeFinder,
    TypeInformation,
    TypeData,
    ClassType,
    ClassKind,
    PrimitiveType,
    PrimitiveKind,
    MemberType,
    BaseClassType,
    TypeIndex,
    VirtualFunctionTablePointerType,
    EnumerationType,
    PointerType,
    BitfieldType,
    UnionType,
    ArrayType,
    EnumerateType,
    RawString,
};

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "pdb error: {}", err)]
    Pdb {
        err: PdbError
    },
}

impl From<PdbError> for Error {
    fn from(err: pdb::Error) -> Self {
        Error::Pdb { err }
    }
}

pub type Result<T> = std::result::Result<T, failure::Error>;

pub struct PdbExtract<P: AsRef<Path>> {
    path: P,
    structs: Vec<String>,
    ignore: Vec<String>,
    replace: Vec<(Regex, String)>,
}

impl<P: AsRef<Path>> PdbExtract<P> {
    pub fn new(path: P) -> PdbExtract<P> {
        PdbExtract {
            path,
            structs: Vec::new(),
            ignore: Vec::new(),
            replace: Vec::new(),
        }
    }

    pub fn add_struct(&mut self, s: String) -> &mut Self {
        self.structs.push(s);
        self
    }

    pub fn ignore_struct(&mut self, s: String) -> &mut Self {
        self.ignore.push(s);
        self
    }

    pub fn replace(&mut self, pat: &str, replace: String) -> &mut Self {
        self.replace.push((Regex::new(pat).unwrap(), replace));
        self
    }

    pub fn write<W: Write>(self, mut writer: W) -> Result<()> {
        let file = File::open(self.path)?;
        let mut pdb = PDB::open(file)?;
        let mut info = pdb.type_information()?;
        let mut writer_internal = Vec::new();
        {
            let mut pdb_writer = Writer::new(&mut info, &mut writer_internal, self.structs, self.ignore, self.replace)?;
            pdb_writer.do_your_thing()?;
        }
        let s = String::from_utf8(writer_internal).unwrap();
        let mut lines = s.lines();
        lazy_static! {
            static ref RE: Regex = Regex::new(r"^(\s+)(\w+): Bitfield(\d+)_(\d+)(\w+),$").unwrap();
            static ref TYP: Regex = Regex::new(r"^(?:i|u)(\d+)$").unwrap();
        }
        let mut bitfield_num = 0;
        let mut indent = "".to_string();
        let mut pos = -1;
        let mut size = -1;
        while let Some(line) = lines.next() {
            if let Some(caps) = RE.captures(line) {
                // bitfield
                let new_indent = &caps[1];
                let name = &caps[2];
                let new_pos = caps[3].parse().unwrap();
                let length: usize = caps[4].parse().unwrap();
                let typ = &caps[5];
                let new_size = if let Some(caps) = TYP.captures(typ) {
                    caps[1].parse().unwrap()
                } else {
                    // TODO: get actual size of non-primitive type
                    1
                };
                if size == -1 {
                    // new bitfield
                    size = new_size;
                    pos = new_pos;
                    indent = new_indent.to_string();
                    writeln!(writer, "{}// {}: Bitfield{}_{}{},", new_indent, name, new_pos, length, typ)?;
                } else if new_pos > pos {
                    // same bitfield
                    size = cmp::max(size, new_size);
                    assert_eq!(indent, new_indent);
                    pos = new_pos;
                    writeln!(writer, "{}// {}: Bitfield{}_{}{},", new_indent, name, new_pos, length, typ)?;
                } else if new_pos < pos {
                    // new bitfield after bitfield
                    size = new_size;
                    writeln!(writer, "{}bitfield{}: u{},", indent, bitfield_num, size)?;
                    bitfield_num += 1;
                    assert_eq!(indent, new_indent);
                    pos = new_pos;
                } else if pos == new_pos {
                    panic!("pos == new_pos ({} == {})\n{:?}", pos, new_pos, caps);
                } else {
                    panic!("WutFace: {},{},{}\n{:?}", indent, pos, size, caps);
                }
            } else {
                // normal field
                if size != -1 {
                    // bitfield must be written
                    writeln!(writer, "{}bitfield{}: u{},", indent, bitfield_num, size)?;
                    bitfield_num += 1;
                    pos = -1;
                    size = -1;
                    indent = "".to_string();
                }
                writeln!(writer, "{}", line)?;
            }
            if line.starts_with("}") {
                bitfield_num = 0;
            }
        }
        Ok(())
    }
}

struct Writer<'t, W: Write> {
    writer: W,
    ignore: Vec<String>,
    replace: Vec<(Regex, String)>,
    finder: TypeFinder<'t>,
    written: Vec<TypeData<'t>>,
    todo: VecDeque<TypeData<'t>>,
    stubs: Vec<TypeData<'t>>,
    name_map: HashMap<String, Option<TypeIndex>>,
    indent: String,
    current_type: Option<String>,
    current_base_class: Option<String>,
}

fn find<'t>(finder: &TypeFinder<'t>, map: &HashMap<String, Option<TypeIndex>>, index: TypeIndex) -> Result<TypeData<'t>> {
    let typ = finder.find(index)?.parse()?;
    if let Some(name) = typ.name() {
        if let Some(Some(index_new)) = map.get(&name.to_string().into_owned()).cloned() {
            if index_new != index {
                return Ok(finder.find(index_new)?.parse()?);
            }
        }
    }
    Ok(typ)
}

impl<'t, 's, W: Write> Writer<'t, W> where 's: 't {
    fn new(info: &'t mut TypeInformation<'s>, writer: W, structs: Vec<String>, ignore: Vec<String>,
            replace: Vec<(Regex, String)>) -> Result<Writer<'t, W>> {
        let mut finder = info.new_type_finder();
        let mut iter = info.iter();
        let mut name_map = HashMap::new();
        while let Some(typ) = iter.next()? {
            finder.update(&iter);
            match typ.parse() {
                Ok(t) => {
                    if t.name().is_none() {
                        continue;
                    }
                    let name = t.name().unwrap().to_string().into_owned();
                    if name.starts_with('<') {
                        // ignore anonymous types
                        continue;
                    }
                    let idx = name_map.get(&name).cloned();
                    if idx.is_none() {
                        name_map.insert(name, Some(typ.type_index()));
                        continue;
                    }
                    let idx = idx.unwrap();
                    if idx.is_none() {
                        // different apparently correct results have already been detected
                        continue;
                    }
                    let index = idx.unwrap();
                    let old = finder.find(index)?.parse()?;
                    match (old, t) {
                        (TypeData::Class(old), TypeData::Class(new)) => {
                            if old.fields.is_none() || old.fields.unwrap() == 0 {
                                name_map.insert(name, Some(typ.type_index()));
                            } else if old != new && new.fields.is_some() && new.fields.unwrap() != 0 {
                                name_map.insert(name, None);
                            }
                        }
                        (old @ TypeData::Class(_), new) => panic!("\nOld: {:?}\nNew: {:?}\n", old, new),
                        (TypeData::Union(old), TypeData::Union(new)) => {
                            if old.fields == 0 {
                                name_map.insert(name, Some(typ.type_index()));
                            } else if old != new && new.fields != 0 {
                                name_map.insert(name, None);
                            }
                        }
                        (old @ TypeData::Union(_), new) => panic!("\nOld: {:?}\nNew: {:?}\n", old, new),
                        _ => {},
                    }
                }
                Err(PdbError::UnimplementedTypeKind(_)) => {},
                Err(e) => Err(e)?,
            }
        }
        let mut todo = VecDeque::with_capacity(structs.len());
        for name in structs {
            match name_map.get(&name).cloned() {
                Some(Some(idx)) => todo.push_back(find(&finder, &name_map, idx)?),
                Some(None) => panic!("{} found multiple times", name),
                None => panic!("{} not found", name)
            }
        }
        Ok(Writer {
            writer,
            ignore,
            replace,
            finder,
            written: Vec::new(),
            todo,
            stubs: Vec::new(),
            name_map,
            indent: "".to_string(),
            current_type: None,
            current_base_class: None,
        })
    }

    /// Finds a type given its index
    ///
    /// There seem to be two types per class, one with and one without fields.
    /// In this method, we try to make sure that the one with fields is returned.
    fn find(&self, index: TypeIndex) -> Result<TypeData<'t>> {
        find(&self.finder, &self.name_map, index)
    }

    /// Converts the passed name to a valid identifier
    ///
    /// If we should ignore a generic type, we should not convert monomorphisations.
    /// Instead, we should keep generic parameters.
    fn ident(&self, name: &RawString) -> String {
        lazy_static! {
            static ref RE: Regex = Regex::new(r"(\w+)\s*\*(const)?").unwrap();
        }
        let mut s = if self.ignore.iter().any(|s| name.to_string().starts_with(s)) {
            RE.replace_all(&name.to_string(), |caps: &regex::Captures| {
                if caps.get(2).is_some() {
                    format!("*const {}", &caps[1])
                } else {
                    format!("*mut {}", &caps[1])
                }
            }).into_owned()
        } else {
            name.to_ident()
        };

        for &(ref re, ref rep) in &self.replace {
            s = re.replace(&s, rep.as_str()).into_owned();
        }
        s
    }

    // TODO: Const Generics for numbers in generics (C++ templates)
    // TODO: Implement types used in generics (in add_type and add_stub extract types with regex and lookup in name_map)

    fn add_todo(&mut self, typ: TypeData<'t>) {
        if let Some(name) = typ.name() {
            if self.ignore.iter().any(|s| name.to_ident().starts_with(s)) {
                return;
            }
        }
        if self.written.contains(&typ) {
            return;
        }
        if self.todo.contains(&typ) {
            return;
        }
        if let Some(pos) = self.stubs.iter().position(|e| *e == typ) {
            self.stubs.remove(pos);
        }
        self.todo.push_back(typ);
    }

    fn add_stub(&mut self, typ: TypeData<'t>) {
        if let Some(name) = typ.name() {
            if self.ignore.iter().any(|s| name.to_ident().starts_with(s)) {
                return;
            }
        }
        if self.written.contains(&typ) {
            return;
        }
        if self.todo.contains(&typ) {
            return;
        }
        if self.stubs.contains(&typ) {
            return;
        }
        self.stubs.push(typ);
    }

    fn cleanup(&mut self) {
        self.current_type = None;
    }

    fn do_your_thing(&mut self) -> Result<()> {
        while !self.todo.is_empty() {
            let next = self.todo.pop_front().unwrap();
            self.written.push(next.clone());
            self.write(next)?;
        }
        while !self.stubs.is_empty() {
            let next = self.stubs.pop().unwrap();
            self.write_stub(next)?;
        }
        writeln!(self.writer)?;
        self.write_bool_types()?;
        Ok(())
    }

    fn write(&mut self, data: TypeData) -> Result<()> {
        match data {
            TypeData::Class(typ) => self.write_class(typ),
            TypeData::Union(typ) => self.write_union(typ),
            TypeData::Enumeration(typ) => self.write_enumeration(typ),
            t => unimplemented!("write: {:?}", t)
        }
    }

    fn write_stub(&mut self, data: TypeData) -> Result<()> {
        match data {
            // Those structs are stubs, you shouldn't be able to instanciate them.
            // Thus, we use Void-like enums.
            TypeData::Class(typ) => writeln!(self.writer, "pub enum {} {{}}", typ.name.to_ident())?,
            t => unimplemented!("write_stub: {:?}", t)
        }
        Ok(())
    }

    fn write_class(&mut self, typ: ClassType) -> Result<()> {
        let ClassType { kind, properties, fields, derived_from, name, size, .. } = typ;
        self.current_type = Some(self.ident(&name));
        assert_eq!(derived_from, None);
        assert_ne!(kind, ClassKind::Interface);
        if properties.packed() {
            writeln!(self.writer, "{}#[repr(C, packed)]", self.indent)?;
        } else {
            writeln!(self.writer, "{}#[repr(C)]", self.indent)?;
        }
        writeln!(self.writer, "{}pub struct {} {{", self.indent, name.to_ident())?;
        if fields.is_none() {
            writeln!(self.writer, "{}}}", self.indent)?;
            writeln!(self.writer)?;
            return Ok(());
        }
        self.indent += "    ";
        self.write_field_list(fields.unwrap())?;
        let len = self.indent.len();
        self.indent.truncate(len - 4);
        writeln!(self.writer, "}} // {:#05x}", size)?;
        writeln!(self.writer)?;
        self.cleanup();
        Ok(())
    }

    fn write_union(&mut self, typ: UnionType) -> Result<()> {
        let UnionType { fields, size, name, ..} = typ;
        self.current_type = Some(self.ident(&name));
        writeln!(self.writer, "{}#[repr(C)]", self.indent)?;
        writeln!(self.writer, "{}pub union {} {{", self.indent, name.to_ident())?;
        self.indent += "    ";
        self.write_field_list(fields)?;
        let len = self.indent.len();
        self.indent.truncate(len - 4);
        writeln!(self.writer, "}} // {:#05x}", size)?;
        writeln!(self.writer)?;
        self.cleanup();
        Ok(())
    }

    fn write_enumeration(&mut self, typ: EnumerationType) -> Result<()> {
        let EnumerationType { underlying_type, fields, name, .. } = typ;
        self.current_type = Some(self.ident(&name));
        write!(self.writer, "{}#[repr(", self.indent)?;
        let repr_typ = self.find(underlying_type)?;
        let size = size(&repr_typ);
        self.write_member_type(repr_typ)?;
        writeln!(self.writer, ")]")?;
        writeln!(self.writer, "{}pub enum {} {{", self.indent, name.to_ident())?;
        self.indent += "    ";
        self.write_field_list(fields)?;
        let len = self.indent.len();
        self.indent.truncate(len - 4);
        writeln!(self.writer, "}} // {:#05x}", size)?;
        writeln!(self.writer)?;
        self.cleanup();
        Ok(())
    }

    fn write_field_list(&mut self, fields: TypeIndex) -> Result<()> {
        match self.find(fields)? {
            TypeData::FieldList(list) => {
                for field in list.fields {
                    match field {
                        TypeData::BaseClass(typ) => self.write_field_base_class(typ)?,
                        TypeData::Member(typ) => self.write_member(typ)?,
                        TypeData::VirtualFunctionTablePointer(typ) => self.write_field_virtual_function_table_pointer(typ)?,
                        TypeData::Enumerate(typ) => self.write_field_enumerate(typ)?,
                        TypeData::MemberFunction(_) => {},
                        TypeData::OverloadedMethod(_) => {},
                        TypeData::Method(_) => {},
                        TypeData::Nested(_) => {},
                        TypeData::StaticMember(_) => {},
                        t => unimplemented!("write_field_list: {:?}", t)
                    }
                }
            }
            _ => panic!("Not a FieldList")
        }
        Ok(())
    }

    fn write_member(&mut self, typ: MemberType) -> Result<()> {
        let MemberType { attributes, field_type, name, .. } = typ;
        if attributes.is_static() || attributes.is_virtual()
                || attributes.is_pure_virtual() || attributes.is_intro_virtual() {
            eprintln!("found nonrelevant member: {}", name);
            return Ok(());
        }
        write!(self.writer, "{}{}: ", self.indent, name.to_ident())?;
        let inner = self.find(field_type)?;
        self.write_member_type(inner)?;
        writeln!(self.writer, ",")?;
        Ok(())
    }

    fn write_member_type(&mut self, typ: TypeData<'t>) -> Result<()> {
        match typ {
            TypeData::Primitive(typ) => self.write_member_primitive(typ)?,
            TypeData::Enumeration(typ) => self.write_member_enumeration(typ)?,
            TypeData::Pointer(typ) => self.write_member_pointer(typ)?,
            TypeData::Class(typ) => self.write_member_class(typ)?,
            TypeData::Bitfield(typ) => self.write_member_bitfield(typ)?,
            TypeData::Union(typ) => self.write_member_union(typ)?,
            TypeData::Array(typ) => self.write_member_array(typ)?,
            t => unimplemented!("write_member: {:?}", t)
        }
        Ok(())
    }

    fn write_field_base_class(&mut self, typ: BaseClassType) -> Result<()> {
        let BaseClassType { kind, attributes, .. } = typ;
        assert_ne!(kind, ClassKind::Interface);
        let typ = if let TypeData::Class(typ) = self.find(typ.base_class)? {
            typ
        } else {
            panic!("BaseClass is not a Class");
        };

        if attributes.is_static() || attributes.is_virtual()
            || attributes.is_pure_virtual() || attributes.is_intro_virtual() {
            eprintln!("found nonrelevant member: {}", typ.name.to_string());
            return Ok(());
        }
        let name = self.ident(&typ.name);
        let old_base_class = mem::replace(&mut self.current_base_class, Some(name));
        writeln!(self.writer, "{}// START base class {}", self.indent, typ.name.to_string())?;
        if let Some(fields) = typ.fields {
            self.write_field_list(fields)?;
        }
        writeln!(self.writer, "{}// END base class {}", self.indent, typ.name.to_string())?;
        self.current_base_class = old_base_class;
        Ok(())
    }

    fn write_field_virtual_function_table_pointer(&mut self, _: VirtualFunctionTablePointerType) -> Result<()> {
        let name = self.current_base_class.as_ref().or(self.current_type.as_ref()).unwrap();
        writeln!(self.writer, "{}vtable_{}: *const (),", self.indent, name)?;
        Ok(())
    }

    fn write_field_enumerate(&mut self, typ: EnumerateType) -> Result<()> {
        writeln!(self.writer, "{}{},", self.indent, typ.name.to_ident())?;
        Ok(())
    }

    fn write_member_primitive(&mut self, typ: PrimitiveType) -> Result<()> {
        match typ.kind {
            PrimitiveKind::Void => write!(self.writer, "()")?,
            PrimitiveKind::Char => write!(self.writer, "i8")?,
            PrimitiveKind::UChar => write!(self.writer, "u8")?,
            PrimitiveKind::RChar => write!(self.writer, "i8")?,
            PrimitiveKind::WChar => write!(self.writer, "u32")?,
            PrimitiveKind::RChar16 => write!(self.writer, "u16")?,
            PrimitiveKind::RChar32 => write!(self.writer, "u32")?,
            PrimitiveKind::I8 => write!(self.writer, "i8")?,
            PrimitiveKind::U8 => write!(self.writer, "u8")?,
            PrimitiveKind::I16 => write!(self.writer, "i16")?,
            PrimitiveKind::U16 => write!(self.writer, "u16")?,
            PrimitiveKind::I32 => write!(self.writer, "i32")?,
            PrimitiveKind::U32 => write!(self.writer, "u32")?,
            PrimitiveKind::I64 => write!(self.writer, "i64")?,
            PrimitiveKind::U64 => write!(self.writer, "u64")?,
            PrimitiveKind::I128 => write!(self.writer, "i128")?,
            PrimitiveKind::U128 => write!(self.writer, "u128")?,
            PrimitiveKind::F16 => panic!("F16"),
            PrimitiveKind::F32 => write!(self.writer, "f32")?,
            PrimitiveKind::F32PP => panic!("F32PP"),
            PrimitiveKind::F48 => panic!("F48"),
            PrimitiveKind::F64 => write!(self.writer, "f64")?,
            PrimitiveKind::F80 => panic!("F80"),
            PrimitiveKind::F128 => panic!("F128"),
            PrimitiveKind::Complex32 => panic!("Complex32"),
            PrimitiveKind::Complex64 => panic!("Complex64"),
            PrimitiveKind::Complex80 => panic!("Complex80"),
            PrimitiveKind::Complex128 => panic!("Complex128"),
            PrimitiveKind::Bool8 => write!(self.writer, "Bool8")?,
            PrimitiveKind::Bool16 => write!(self.writer, "Bool16")?,
            PrimitiveKind::Bool32 => write!(self.writer, "Bool32")?,
            PrimitiveKind::Bool64 => write!(self.writer, "Bool64")?,
            t => unimplemented!("write_member_primitive: {:?}", t)
        }
        Ok(())
    }

    fn write_member_enumeration(&mut self, typ: EnumerationType<'t>) -> Result<()> {
        let name = self.ident(&typ.name);
        write!(self.writer, "{}", name)?;
        self.add_todo(TypeData::Enumeration(typ));
        Ok(())
    }

    fn write_member_pointer(&mut self, typ: PointerType) -> Result<()> {
        if typ.attributes.is_const() {
            write!(self.writer, "*const ")?;
        } else {
            write!(self.writer, "*mut ")?;
        }
        let underlying = self.find(typ.underlying_type)?;
        let name = self.ident(&underlying.name().unwrap());
        write!(self.writer, "{}", name)?;
        self.add_stub(underlying);
        Ok(())
    }

    fn write_member_class(&mut self, typ: ClassType<'t>) -> Result<()> {
        let name = self.ident(&typ.name);
        write!(self.writer, "{}", name)?;
        self.add_todo(TypeData::Class(typ));
        Ok(())
    }

    fn write_member_bitfield(&mut self, typ: BitfieldType) -> Result<()> {
        let underlying = self.find(typ.underlying_type)?;
        write!(self.writer, "Bitfield{}_{}", typ.position, typ.length)?;
        if let TypeData::Primitive(typ) = underlying {
            self.write_member_primitive(typ)?;
        } else if let TypeData::Enumeration(typ) = underlying {
            let name = self.ident(&typ.name);
            write!(self.writer, "{}", name)?;
        } else {
            panic!("Bitfield is {:?}", underlying);
        }
        Ok(())
    }

    fn write_member_union(&mut self, typ: UnionType<'t>) -> Result<()> {
        let name = self.ident(&typ.name);
        write!(self.writer, "{}", name)?;
        self.add_todo(TypeData::Union(typ));
        Ok(())
    }

    fn write_member_array(&mut self, typ: ArrayType) -> Result<()> {
        let ArrayType { element_type, dimensions, .. } = typ;
        // TODO: do this correctly
        assert_eq!(dimensions.len(), 1);
        let underlying = self.find(element_type)?;
        let size = size(&underlying);
        write!(self.writer, "[")?;
        self.write_member_type(underlying)?;
        write!(self.writer, "; {}]", dimensions[0] as usize / size)?;
        Ok(())
    }

    fn write_bool_types(&mut self) -> Result<()> {
        for i in (0..4).map(|i| 2u8.pow(i + 3)) {
            writeln!(self.writer, "{}", bool_fmt(i))?;
            writeln!(self.writer)?;
        }
        Ok(())
    }

    // TODO: Convert bitfields into bitflags
}

fn bool_fmt(size: u8) -> String {
    format!(r#"#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Bool{0}(u{0});

impl From<Bool{0}> for bool {{
    fn from(b: Bool{0}) -> Self {{
        match b.0 {{
            0 => false,
            _ => true,
        }}
    }}
}}"#, size)
}

fn size(typ: &TypeData) -> usize {
    match typ {
        &TypeData::Primitive(typ) => match typ.kind {
            PrimitiveKind::Void => 0,
            PrimitiveKind::Char => 1,
            PrimitiveKind::UChar => 1,
            PrimitiveKind::RChar => 1,
            PrimitiveKind::WChar => 4,
            PrimitiveKind::I8 => 1,
            PrimitiveKind::U8 => 1,
            PrimitiveKind::I16 => 2,
            PrimitiveKind::U16 => 2,
            PrimitiveKind::I32 => 4,
            PrimitiveKind::U32 => 4,
            PrimitiveKind::I64 => 8,
            PrimitiveKind::U64 => 8,
            PrimitiveKind::I128 => 16,
            PrimitiveKind::U128 => 16,
            PrimitiveKind::F16 => 2,
            PrimitiveKind::F32 => 4,
            PrimitiveKind::F48 => 6,
            PrimitiveKind::F64 => 8,
            PrimitiveKind::F80 => 10,
            PrimitiveKind::F128 => 16,
            PrimitiveKind::Bool8 => 1,
            PrimitiveKind::Bool16 => 2,
            PrimitiveKind::Bool32 => 4,
            PrimitiveKind::Bool64 => 8,
            t => unimplemented!("size: primitive: {:?}", t)
        },
        &TypeData::Class(ref class) => class.size as usize,
        t => unimplemented!("size: {:?}", t)
    }
}

trait ToIdent<'a> {
    fn to_ident(&self) -> String;
}

impl<'a> ToIdent<'a> for RawString<'a> {
    fn to_ident(&self) -> String {
        lazy_static! {
            static ref RE: Regex = Regex::new("[^a-zA-z0-9]+").unwrap();
        }
        RE.replace_all(self.to_string().as_ref(), "_").into_owned()
    }
}
