use std::io::Write;
use std::collections::VecDeque;
use std::mem;
use std::borrow::Cow;

use crate::ir::*;
use crate::Result;

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
    pub fn new(w: W, arena: &'a Arena) -> Writer<'a, W> {
        Writer {
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
        }
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
            add_fn(self, self.arena[generic]);
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

    fn write_class(&mut self, class: &Class) -> Result<()> {
        let Class { name, kind, members, properties, size } = class;
        self.current_type_name = Some(name.ident.clone());
        assert_ne!(*kind, ClassKind::Interface);
        writeln!(self.w, "{}// {}", self.indent, name.name)?;
        if properties.packed {
            writeln!(self.w, "{}#[repr(C, packed)]", self.indent)?;
        } else {
            writeln!(self.w, "{}#[repr(C)]", self.indent)?;
        }
        writeln!(self.w, "{}pub struct {} {{", self.indent, name.ident)?;
        self.indent();
        self.union_number = 0;
        self.current_fields = Vec::new();
        for member in members {
            self.write_class_member(member)?;
        }
        self.dedent();
        self.current_type_name = None;
        writeln!(self.w, "{}}} // {:#05x}", self.indent, size)?;
        Ok(())
    }

    fn write_union(&mut self, u: &Union) -> Result<()> {
        let Union { name, fields, properties, size, .. } = u;
        writeln!(self.w, "{}// {}", self.indent, name.name)?;
        if properties.packed {
            writeln!(self.w, "{}#[repr(C, packed)]", self.indent)?;
        } else {
            writeln!(self.w, "{}#[repr(C)]", self.indent)?;
        }
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
        writeln!(self.w, "{}}} // {:#05x}", self.indent, size)?;
        Ok(())
    }

    fn write_enum(&mut self, e: &Enum) -> Result<()> {
        let size = e.size(self.arena);
        let Enum { name, underlying, variants, properties, .. } = e;
        writeln!(self.w, "{}// {}", self.indent, name.name)?;
        write!(self.w, "{}#[repr(", self.indent)?;
        self.write_field_primitive(underlying)?;
        if properties.packed {
            writeln!(self.w, ", packed)]")?;
        } else {
            writeln!(self.w, ")]")?;
        }
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
        writeln!(self.w, "{}}} // {:#05x}", self.indent, size)?;
        Ok(())
    }

    fn write_class_member(&mut self, member: &ClassMember) -> Result<()> {
        match member {
            ClassMember::Vtable => self.write_vtable()?,
            ClassMember::BaseClass(base) => self.write_base_class(base)?,
            ClassMember::VirtualBaseClass(base) => self.write_virtual_base_class(base)?,
            ClassMember::Field(field) => self.write_class_field(field)?,
        }
        Ok(())
    }

    fn write_vtable(&mut self) -> Result<()> {
        let name = self.current_base_class_name.as_ref()
            .or(self.current_type_name.as_ref()).unwrap();
        writeln!(self.w, "{}vtable_{}: *const (),", self.indent, name)?;
        Ok(())
    }

    fn write_base_class(&mut self, base: &BaseClass) -> Result<()> {
        let BaseClass { attributes, offset, base_class } = base;
        let base_class = self.arena.get_largest_class_index(*base_class);
        let Class { name, kind, members, properties, size } = &self.arena[base_class];
        if attributes.any() {
            eprintln!("found nonrelevant base class: {}", name.name);
            return Ok(());
        }
        let old_base_class_name = mem::replace(&mut self.current_base_class_name, Some(name.ident.clone()));
        writeln!(self.w, "{}// START base class {}", self.indent, name.name)?;
        for member in members {
            self.write_class_member(member)?;
        }
        writeln!(self.w, "{}// END base class {} // {:#05x}", self.indent, name.name, size)?;
        self.current_base_class_name = old_base_class_name;
        Ok(())
    }

    fn write_virtual_base_class(&mut self, base: &VirtualBaseClass) -> Result<()> {
        let VirtualBaseClass { attributes, base_pointer_offset, base_class, .. } = base;
        let base_class = self.arena.get_largest_class_index(*base_class);
        let Class { name, kind, members, properties, size } = &self.arena[base_class];
        if attributes.any() {
            eprintln!("found nonrelevant base class: {}", name.name);
            return Ok(());
        }
        let old_base_class_name = mem::replace(&mut self.current_base_class_name, Some(name.ident.clone()));
        writeln!(self.w, "{}// START virtual base class {}", self.indent, name.name)?;
        for member in members {
            self.write_class_member(member)?;
        }
        writeln!(self.w, "{}// END virtual base class {} // {:#05x}", self.indent, name.name, size)?;
        self.current_base_class_name = old_base_class_name;
        Ok(())
    }

    fn write_class_field(&mut self, field: &ClassField) -> Result<()> {
        let ClassField { attributes, name, offset, kind } = field;
        if attributes.any() {
            eprintln!("found nonrelevant field: {}", name.name);
            return Ok(());
        }
        if let ClassFieldKind::Union(_) = kind {
            write!(self.w, "{}union{}: ", self.indent, self.union_number)?;
            self.union_number += 1;
        } else {
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
            write!(self.w, "{}{}: ", self.indent, ident)?;
            self.current_fields.push(ident.into_owned());
        }
        self.write_class_field_kind(kind)?;
        writeln!(self.w, ", // {:#05x}", offset)?;
        Ok(())
    }

    fn write_class_field_kind(&mut self, kind: &ClassFieldKind) -> Result<()> {
        match kind {
            ClassFieldKind::Primitive(prim) => self.write_field_primitive(prim)?,
            ClassFieldKind::Enum(e) => self.write_field_enum(*e)?,
            ClassFieldKind::Pointer(ptr) => self.write_field_pointer(ptr)?,
            ClassFieldKind::Class(class) => self.write_field_class(*class)?,
            ClassFieldKind::Bitfield(b) => self.write_field_bitfield(b)?,
            ClassFieldKind::Union(u) => self.write_field_union(*u)?,
            ClassFieldKind::Array(arr) => self.write_field_array(arr)?,
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
        self.write_class_field_kind(underlying)?;
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

    fn write_field_array(&mut self, arr: &Array) -> Result<()> {
        let Array { element_type, dimensions, .. } = arr;
        for _ in dimensions {
            write!(self.w, "[")?;
        }
        self.write_class_field_kind(element_type)?;
        for d in dimensions {
            write!(self.w, "; {}]", d)?;
        }
        Ok(())
    }

    fn write_field_modifier(&mut self, m: &Modifier) -> Result<()> {
        let Modifier { underlying, constant, .. } = m;
//        if *constant {
//            write!(self.w, "const ")?;
//        }
        self.write_class_field_kind(underlying)
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

impl<'a, W: Write> Drop for Writer<'a, W> {
    fn drop(&mut self) {
        // TODO: if panicking
        self.write_todos().unwrap();
        self.write_stubs().unwrap();
        self.write_bool_types().unwrap();
    }
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
