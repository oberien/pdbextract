use std::io::Write;
use std::collections::VecDeque;
use std::mem;
use std::borrow::Cow;

use crate::ir::*;
use crate::{Alignment, Result};

pub struct Writer<'a, W: Write> {
    w: W,
    arena: &'a Arena,
    todo: VecDeque<TypeIndex>,
    stubs: VecDeque<TypeIndex>,
    written: Vec<TypeIndex>,
    indent: String,
    current_type_name: Option<String>,
    current_base_class_name: Option<String>,
    union_number: usize,
    current_fields: Vec<String>,
    is_pointer_field: bool,
}

impl<'a, W: Write> Writer<'a, W> {
    pub fn new(mut w: W, arena: &'a Arena) -> Result<Writer<'a, W>> {
        writeln!(w, "#![allow(non_camel_case_types, non_snake_case)]")?;
        Ok(Writer {
            w,
            arena,
            todo: VecDeque::new(),
            stubs: VecDeque::new(),
            written: Vec::new(),
            indent: String::new(),
            current_type_name: None,
            current_base_class_name: None,
            union_number: 0,
            current_fields: Vec::new(),
            is_pointer_field: false,
        })
    }

    fn indent(&mut self) {
        self.indent += "    ";
    }
    fn dedent(&mut self) {
        self.indent.truncate(self.indent.len() - 4);
    }

    fn add_written(&mut self, index: TypeIndex) {
        if let Some(pos) = self.todo.iter().position(|e| *e == index) {
            self.todo.remove(pos);
        }
        if let Some(pos) = self.stubs.iter().position(|e| *e == index) {
            self.stubs.remove(pos);
        }
        self.written.push(index);
    }

    fn add_todo(&mut self, index: TypeIndex) {
        let index = self.arena.get_largest_type_index(index);
        if self.written.contains(&index) {
            return;
        }
        if self.todo.contains(&index) {
            return;
        }
        if let Some(pos) = self.stubs.iter().position(|e| *e == index) {
            self.stubs.remove(pos);
        }
        self.add_generics(index, Self::add_todo);
        self.todo.push_back(index);
    }

    fn add_stub(&mut self, index: TypeIndex) {
        let index = self.arena.get_largest_type_index(index);
        if self.written.contains(&index) {
            return;
        }
        if self.todo.contains(&index) {
            return;
        }
        if self.stubs.contains(&index) {
            return;
        }
        self.add_generics(index, Self::add_stub);
        self.stubs.push_back(index);
    }

    fn add_generics<F: Fn(&mut Self, TypeIndex)>(&mut self, index: TypeIndex, add_fn: F) {
        let index = self.arena.get_largest_type_index(index);
        let name = match index {
            TypeIndex::Class(c) => &self.arena[c].name,
            TypeIndex::Union(u) => &self.arena[u].name,
            TypeIndex::Enum(e) => &self.arena[e].name,
        };
        for generic in &name.generics {
            match self.arena.get_type_by_name(generic) {
                Some(type_index) => add_fn(self, *type_index),
                None => eprintln!("Can't find generic type with name {:?}", generic),
            }
        }
    }

    pub fn write_type(&mut self, index: TypeIndex) -> Result<()> {
        let index = self.arena.get_largest_type_index(index);
        self.add_written(index);
        match index {
            TypeIndex::Class(c) => self.write_class(&self.arena[c]),
            TypeIndex::Union(u) => self.write_union(&self.arena[u]),
            TypeIndex::Enum(e) => self.write_enum(&self.arena[e]),
        }
    }

    pub fn write_exact_type(&mut self, index: TypeIndex) -> Result<()> {
        self.add_written(index);
        match index {
            TypeIndex::Class(c) => self.write_class(&self.arena[c]),
            TypeIndex::Union(u) => self.write_union(&self.arena[u]),
            TypeIndex::Enum(e) => self.write_enum(&self.arena[e]),
        }
    }

    pub fn write_todos(&mut self) -> Result<()> {
        while let Some(index) = self.todo.pop_front() {
            self.add_written(index);
            self.write_type(index)?;
        }
        Ok(())
    }

    pub fn write_stubs(&mut self) -> Result<()> {
        while let Some(index) = self.stubs.pop_front() {
            self.add_written(index);
            match index {
                // Those structs are stubs, you shouldn't be able to instanciate them.
                // Thus, we use Void-like enums.
                TypeIndex::Class(c) => {
                    let class = &self.arena[c];
                    writeln!(self.w, "pub enum {} {{}}", class.name.ident)?;
                }
                t => unimplemented!("write_stubs: {:?}", t)
            }
        }
        Ok(())
    }

    pub fn write_rest(&mut self) -> Result<()> {
        self.write_todos()?;
        self.write_stubs()?;
        self.write_bool_types()?;
        Ok(())
    }

    fn write_alignment(&mut self, alignment: Alignment) -> Result<()> {
        match alignment {
            Alignment::None => (),
            Alignment::Both(align) => writeln!(self.w, "{}#[repr(align({align}))]", self.indent)?,
            Alignment::Windows(align) => writeln!(self.w, "{}#[repr(cfg_attr(windows, align = {align}))]", self.indent)?,
            Alignment::Linux(align) => writeln!(self.w, r#"{}#[repr(cfg_attr(target_os = "linux", align = {align}))]"#, self.indent)?,
        }
        Ok(())
    }

    fn write_class(&mut self, class: &Class) -> Result<()> {
        let Class { name, kind, members, properties, size, alignment } = class;
        self.current_type_name = Some(name.ident.clone());
        assert_ne!(*kind, ClassKind::Interface);
        writeln!(self.w, "{}// {}", self.indent, name.name)?;
        if properties.packed {
            writeln!(self.w, "{}#[repr(C, packed)]", self.indent)?;
        } else {
            writeln!(self.w, "{}#[repr(C)]", self.indent)?;
        }
        self.write_alignment(*alignment)?;
        writeln!(self.w, "{}#[derive(Clone, Copy)]", self.indent)?;
        writeln!(self.w, "{}pub struct {} {{", self.indent, name.ident)?;
        self.indent();
        self.union_number = 0;
        self.current_fields = Vec::new();
        let mut fields = Vec::new();
        for member in members {
            fields.extend(self.write_class_member(member)?);
        }
        self.dedent();
        self.current_type_name = None;
        writeln!(self.w, "{}}} // size {:#05x}", self.indent, size)?;

        // write layout test
        let struct_name = &name.ident;
        writeln!(self.w, "{}#[test]", self.indent)?;
        writeln!(self.w, "{}pub fn test_{}_layout() {{", self.indent, struct_name)?;
        self.indent();
        for (name, offset) in fields {
            match offset {
                Some(offset) => writeln!(self.w, "{}assert_eq!({offset:#05x}, memoffset::offset_of!({struct_name}, {name}));", self.indent)?,
                None => writeln!(self.w, "{}// skipped field {}", self.indent, name)?,
            }
        }
        writeln!(self.w, "{}assert_eq!({size:#05x}, std::mem::size_of::<{struct_name}>());", self.indent)?;
        self.dedent();
        writeln!(self.w, "{}}}", self.indent)?;

        Ok(())
    }

    fn write_union(&mut self, u: &Union) -> Result<()> {
        let Union { name, fields, properties, size, count: _, alignment } = u;
        writeln!(self.w, "{}// {}", self.indent, name.name)?;
        if properties.packed {
            writeln!(self.w, "{}#[repr(C, packed)]", self.indent)?;
        } else {
            writeln!(self.w, "{}#[repr(C)]", self.indent)?;
        }
        self.write_alignment(*alignment)?;
        writeln!(self.w, "{}#[derive(Clone, Copy)]", self.indent)?;
        writeln!(self.w, "{}pub union {} {{", self.indent, name.ident)?;
        self.current_type_name = Some(name.ident.clone());
        self.indent();
        self.union_number = 0;
        self.current_fields = Vec::new();
        for field in fields {
            self.write_class_field(field)?;
        }
        self.dedent();
        self.current_type_name = None;
        writeln!(self.w, "{}}} // size {:#05x}", self.indent, size)?;
        Ok(())
    }

    fn write_enum(&mut self, e: &Enum) -> Result<()> {
        let size = e.size(self.arena);
        let Enum { name, underlying, variants, properties, count: _, alignment } = e;
        writeln!(self.w, "{}// {}", self.indent, name.name)?;
        write!(self.w, "{}#[repr(", self.indent)?;
        self.write_field_primitive(underlying)?;
        if properties.packed {
            writeln!(self.w, ", packed)]")?;
        } else {
            writeln!(self.w, ")]")?;
        }
        self.write_alignment(*alignment)?;
        writeln!(self.w, "{}#[derive(Clone, Copy)]", self.indent)?;
        writeln!(self.w, "{}pub enum {} {{", self.indent, name.ident)?;
        self.current_type_name = Some(name.ident.clone());
        self.indent();
        self.union_number = 0;
        self.current_fields = Vec::new();
        for variant in variants {
            self.write_variant(variant)?;
        }
        self.dedent();
        self.current_type_name = None;
        writeln!(self.w, "{}}} // size {:#05x}", self.indent, size)?;
        Ok(())
    }

    fn write_class_member(&mut self, member: &ClassMember) -> Result<Vec<(String, Option<usize>)>> {
        Ok(match member {
            ClassMember::Vtable => self.write_vtable()?,
            ClassMember::BaseClass(base) => self.write_base_class(base)?,
            ClassMember::VirtualBaseClass(base) => self.write_virtual_base_class(base)?,
            ClassMember::Field(field) => self.write_class_field(field)?,
        })
    }

    fn write_vtable(&mut self) -> Result<Vec<(String, Option<usize>)>> {
        let name = self.current_base_class_name.as_ref()
            .or(self.current_type_name.as_ref()).unwrap();
        let name = format!("vtable_{}", name);
        writeln!(self.w, "{}{}: *const (),", self.indent, name)?;
        Ok(vec![(name, None)])
    }

    fn write_base_class(&mut self, base: &BaseClass) -> Result<Vec<(String, Option<usize>)>> {
        let BaseClass { attributes, offset, base_class } = base;
        let base_class = self.arena.get_largest_class_index(*base_class);
        let Class { name, kind, members, properties, size, alignment } = &self.arena[base_class];
        assert_eq!(*alignment, Alignment::None, "unimplemented: BaseClass Alignment");
        if attributes.any() {
            eprintln!("found nonrelevant base class: {}", name.name);
            return Ok(vec![]);
        }
        let old_base_class_name = mem::replace(&mut self.current_base_class_name, Some(name.ident.clone()));
        writeln!(self.w, "{}// START base class {}", self.indent, name.name)?;
        let mut names = Vec::new();
        for member in members {
            names.extend(self.write_class_member(member)?);
        }
        writeln!(self.w, "{}// END base class {} // size {:#05x}", self.indent, name.name, size)?;
        self.current_base_class_name = old_base_class_name;
        Ok(names)
    }

    fn write_virtual_base_class(&mut self, base: &VirtualBaseClass) -> Result<Vec<(String, Option<usize>)>> {
        let VirtualBaseClass { attributes, base_pointer_offset, base_class, .. } = base;
        let base_class = self.arena.get_largest_class_index(*base_class);
        let Class { name, kind, members, properties, size, alignment } = &self.arena[base_class];
        assert_eq!(*alignment, Alignment::None, "unimplemented: VirtualBaseClass Alignment");
        if attributes.any() {
            eprintln!("found nonrelevant base class: {}", name.name);
            return Ok(vec![]);
        }
        let old_base_class_name = mem::replace(&mut self.current_base_class_name, Some(name.ident.clone()));
        writeln!(self.w, "{}// START virtual base class {}", self.indent, name.name)?;
        let mut names = Vec::new();
        for member in members {
            names.extend(self.write_class_member(member)?);
        }
        writeln!(self.w, "{}// END virtual base class {} // size {:#05x}", self.indent, name.name, size)?;
        self.current_base_class_name = old_base_class_name;
        Ok(names)
    }

    fn write_class_field(&mut self, field: &ClassField) -> Result<Vec<(String, Option<usize>)>> {
        let ClassField { attributes, name, offset, kind, max_size } = field;
        if attributes.any() {
            eprintln!("found nonrelevant field: {}", name.name);
            return Ok(vec![]);
        }
        // let name = if let ClassFieldKind::Union(_) = kind {
        //     let name = format!("union{}", self.union_number);
        //     write!(self.w, "{}{}: ", self.indent, name)?;
        //     self.union_number += 1;
        //     name
        // } else {
            // fix multiple fields with the same name
            let ident: Cow<_> = if self.current_fields.contains(&name.ident) {
                let mut i = 0;
                loop {
                    i += 1;
                    let ident = format!("{}{}", name.ident, i);
                    if self.current_fields.contains(&ident) {
                        continue;
                    }
                    break ident.into();
                }
            } else {
                (&*name.ident).into()
            };
            write!(self.w, "{}pub {}: ", self.indent, ident)?;
            self.current_fields.push(ident.clone().into_owned());
            let name = ident.into_owned();
        // };
        self.write_class_field_kind(kind, *max_size)?;
        writeln!(self.w, ", // offset {:#05x}", offset)?;
        Ok(vec![(name, Some(*offset))])
    }

    fn write_class_field_kind(&mut self, kind: &ClassFieldKind, max_size: usize) -> Result<()> {
        match kind {
            ClassFieldKind::Primitive(prim) => self.write_field_primitive(prim)?,
            ClassFieldKind::Enum(e) => self.write_field_enum(*e)?,
            ClassFieldKind::Pointer(ptr) => self.write_field_pointer(ptr)?,
            ClassFieldKind::Class(class) => self.write_field_class(*class)?,
            ClassFieldKind::Bitfield(b) => self.write_field_bitfield(b)?,
            ClassFieldKind::Union(u) => self.write_field_union(*u)?,
            ClassFieldKind::Array(arr) => self.write_field_array(arr, max_size)?,
            ClassFieldKind::Modifier(m) => self.write_field_modifier(m)?,
            // ignore as they aren't fields
            ClassFieldKind::Procedure => self.write_field_function()?,
            ClassFieldKind::MemberFunction => self.write_field_function()?,
            ClassFieldKind::Method => self.write_field_function()?,
        }
        Ok(())
    }

    fn write_field_primitive(&mut self, prim: &PrimitiveKind) -> Result<()> {
        match prim {
            PrimitiveKind::Void => write!(self.w, "()")?,
            PrimitiveKind::Char => write!(self.w, "i8")?,
            PrimitiveKind::UChar => write!(self.w, "u8")?,
            PrimitiveKind::RChar => write!(self.w, "i8")?,
            PrimitiveKind::WChar => write!(self.w, "u32")?,
            PrimitiveKind::RChar16 => write!(self.w, "u16")?,
            PrimitiveKind::RChar32 => write!(self.w, "u32")?,
            PrimitiveKind::I8 => write!(self.w, "i8")?,
            PrimitiveKind::U8 => write!(self.w, "u8")?,
            PrimitiveKind::I16 => write!(self.w, "i16")?,
            PrimitiveKind::U16 => write!(self.w, "u16")?,
            PrimitiveKind::I32 => write!(self.w, "i32")?,
            PrimitiveKind::U32 => write!(self.w, "u32")?,
            PrimitiveKind::I64 => write!(self.w, "i64")?,
            PrimitiveKind::U64 => write!(self.w, "u64")?,
            PrimitiveKind::I128 => write!(self.w, "i128")?,
            PrimitiveKind::U128 => write!(self.w, "u128")?,
            PrimitiveKind::F16 => panic!("F16"),
            PrimitiveKind::F32 => write!(self.w, "f32")?,
            PrimitiveKind::F32PP => panic!("F32PP"),
            PrimitiveKind::F48 => panic!("F48"),
            PrimitiveKind::F64 => write!(self.w, "f64")?,
            PrimitiveKind::F80 => panic!("F80"),
            PrimitiveKind::F128 => panic!("F128"),
            PrimitiveKind::Complex32 => panic!("Complex32"),
            PrimitiveKind::Complex64 => panic!("Complex64"),
            PrimitiveKind::Complex80 => panic!("Complex80"),
            PrimitiveKind::Complex128 => panic!("Complex128"),
            PrimitiveKind::Bool8 => write!(self.w, "Bool8")?,
            PrimitiveKind::Bool16 => write!(self.w, "Bool16")?,
            PrimitiveKind::Bool32 => write!(self.w, "Bool32")?,
            PrimitiveKind::Bool64 => write!(self.w, "Bool64")?,
            t => unimplemented!("write_member_primitive: {:?}", t)
        }
        Ok(())
    }

    fn write_field_enum(&mut self, e: EnumIndex) -> Result<()> {
        let Enum { name, .. } = &self.arena[e];
        write!(self.w, "{}", name.ident)?;
        if self.is_pointer_field {
            self.add_stub(TypeIndex::Enum(e));
        } else {
            self.add_todo(TypeIndex::Enum(e));
        }
        Ok(())
    }

    fn write_field_pointer(&mut self, ptr: &Pointer) -> Result<()> {
        let Pointer { underlying, is_const, .. } = ptr;
        if *is_const {
            write!(self.w, "*const ")?;
        } else {
            write!(self.w, "*mut ")?;
        }
        self.is_pointer_field = true;
        self.write_class_field_kind(underlying, usize::MAX)?;
        self.is_pointer_field = false;
        Ok(())
    }

    fn write_field_class(&mut self, class: ClassIndex) -> Result<()> {
        let Class { name, .. } = &self.arena[class];
        write!(self.w, "{}", name.ident)?;
        if self.is_pointer_field {
            self.add_stub(TypeIndex::Class(class));
        } else {
            self.add_todo(TypeIndex::Class(class));
        }
        Ok(())
    }

    fn write_field_bitfield(&mut self, b: &Bitfield) -> Result<()> {
        let size = b.size(self.arena);
        let Bitfield { fields } = b;
        // TODO: write fields above?
        write!(self.w, "u{}", size * 8)?;
        Ok(())
    }

    fn write_field_union(&mut self, u: UnionIndex) -> Result<()> {
        let Union { name, .. } = &self.arena[u];
        write!(self.w, "{}", name.ident)?;
        if self.is_pointer_field {
            self.add_stub(TypeIndex::Union(u));
        } else {
            self.add_todo(TypeIndex::Union(u));
        }
        Ok(())
    }

    fn write_field_array(&mut self, arr: &Array, max_size: usize) -> Result<()> {
        let Array { element_type, dimensions, stride } = arr;
        for _ in dimensions {
            write!(self.w, "[")?;
        }
        let mut size = element_type.size(self.arena);
        dbg!(size, max_size);
        self.write_class_field_kind(element_type, usize::MAX)?;
        dbg!(element_type, stride, dimensions);
        let len = dimensions.len();
        for (i, &d) in dimensions.iter().enumerate() {
            let d = if i == len-1 && max_size < usize::MAX && max_size > d {
                // last element
                // The pdb sometimes reports wrong dimensions (number of elements instead of number
                // of bytes). Thus, for the last element we use max_size instead of the dimension.
                if max_size != d {
                    eprintln!("PDB reported invalid array dimension: {} instead of max {}", d, max_size);
                }
                max_size
            } else {
                if max_size == usize::MAX {
                    eprintln!("Unknown Array max_size");
                }
                d
            };
            let num_elements = d / size.max(1);
            write!(self.w, "; {}]", num_elements)?;
            size = d;
        }
        Ok(())
    }

    fn write_field_modifier(&mut self, m: &Modifier) -> Result<()> {
        let Modifier { underlying, constant, .. } = m;
//        if *constant {
//            write!(self.w, "const ")?;
//        }
        self.write_class_field_kind(underlying, usize::MAX)
    }

    fn write_field_function(&mut self) -> Result<()> {
        // TODO: actual function types
        write!(self.w, "fn()")?;
        Ok(())
    }

    fn write_variant(&mut self, variant: &Variant) -> Result<()> {
        let Variant { name, attributes, .. } = variant;
        if attributes.any() {
            unimplemented!("attributes for enum variant");
        }
        writeln!(self.w, "{}{},", self.indent, name.ident)?;
        Ok(())
    }

    fn write_bool_types(&mut self) -> Result<()> {
        for i in (0..4).map(|i| 2u8.pow(i + 3)) {
            writeln!(self.w, "{}", bool_fmt(i))?;
            writeln!(self.w)?;
        }
        Ok(())
    }
}

fn bool_fmt(size: u8) -> String {
    format!(r#"#[repr(transparent)]
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
