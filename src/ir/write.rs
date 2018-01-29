use std::io::Write;
use std::collections::VecDeque;
use std::mem;

use ir::*;

pub struct Writer<'a, W: Write> {
    w: W,
    arena: &'a Arena,
    todo: VecDeque<TypeIndex>,
    stubs: VecDeque<TypeIndex>,
    written: Vec<TypeIndex>,
    indent: String,
    current_type_name: Option<String>,
    current_base_class_name: Option<String>,
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
        }
    }

    fn indent(&mut self) {
        self.indent += "    ";
    }
    fn dedent(&mut self) {
        self.indent.truncate(self.indent.len() - 4);
    }

    fn add_todo(&mut self, index: TypeIndex) {
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
        match index {
            TypeIndex::Class(c) => self.write_class(&self.arena[c]),
            TypeIndex::Union(u) => self.write_union(&self.arena[u]),
            TypeIndex::Enum(e) => self.write_enum(&self.arena[e]),
        }
    }

    pub fn write_class(&mut self, class: &Class) -> Result<()> {
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
        for member in members {
            self.write_class_member(member)?;
        }
        self.dedent();
        self.current_type_name = None;
        writeln!(self.w, "{}}} // {:#05x}", self.indent, size)?;
        Ok(())
    }

    pub fn write_union(&mut self, u: &Union) -> Result<()> {
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
        for field in fields {
            self.write_class_field(field)?;
        }
        self.dedent();
        self.current_type_name = None;
        writeln!(self.w, "{}}} // {:#05x}", self.indent, size)?;
        Ok(())
    }

    pub fn write_enum(&mut self, e: &Enum) -> Result<()> {
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
        for variant in variants {
            self.write_variant(variant)?;
        }
        self.dedent();
        self.current_type_name = None;
        writeln!(self.w, "{}}} // {:#05x}", self.indent, size)?;
        Ok(())
    }

    pub fn write_class_member(&mut self, member: &ClassMember) -> Result<()> {
        match member {
            ClassMember::Vtable => self.write_vtable()?,
            ClassMember::BaseClass(base) => self.write_base_class(base)?,
            ClassMember::Field(field) => self.write_class_field(field)?,
        }
        Ok(())
    }

    pub fn write_vtable(&mut self) -> Result<()> {
        let name = self.current_base_class_name.as_ref()
            .or(self.current_type_name.as_ref()).unwrap();
        writeln!(self.w, "{}vtable_{}: *const (),", self.indent, name)?;
        Ok(())
    }

    pub fn write_base_class(&mut self, base: &BaseClass) -> Result<()> {
        let BaseClass { attributes, offset, base_class } = base;
        let Class { name, kind, members, properties, size } = &self.arena[*base_class];
        if attributes.any() {
            eprintln!("found nonrelevant base class: {}", name.name);
            return Ok(());
        }
        let old_base_class_name = mem::replace(&mut self.current_base_class_name, Some(name.ident.clone()));
        writeln!(self.w, "{}// START base class {}", self.indent, name.name)?;
        for member in members {
            self.write_class_member(member)?;
        }
        writeln!(self.w, "{}// END base class {}", self.indent, name.name)?;
        self.current_base_class_name = old_base_class_name;
        Ok(())
    }

    pub fn write_class_field(&mut self, field: &ClassField) -> Result<()> {
        let ClassField { attributes, name, offset, kind } = field;
        if attributes.any() {
            eprintln!("found nonrelevant field: {}", name.name);
            return Ok(());
        }
        write!(self.w, "{}{}: ", self.indent, name.ident)?;
        writeln!(self.w, ", // {:#05x}", offset)?;
        Ok(())
    }

    pub fn write_class_field_kind(&mut self, kind: &ClassFieldKind) -> Result<()> {
        match kind {
            ClassFieldKind::Primitive(prim) => self.write_field_primitive(prim)?,
            ClassFieldKind::Enum(e) => self.write_field_enum(*e)?,
            ClassFieldKind::Pointer(ptr) => self.write_field_pointer(ptr)?,
            ClassFieldKind::Class(class) => self.write_field_class(*class)?,
            ClassFieldKind::Bitfield(b) => self.write_field_bitfield(b)?,
            ClassFieldKind::Union(u) => self.write_field_union(*u)?,
            ClassFieldKind::Array(arr) => self.write_field_array(arr)?,
            ClassFieldKind::Modifier(m) => self.write_field_modifier(m)?,
        }
        Ok(())
    }

    pub fn write_field_primitive(&mut self, prim: &PrimitiveKind) -> Result<()> {
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

    pub fn write_field_enum(&mut self, e: EnumIndex) -> Result<()> {
        let Enum { name, .. } = &self.arena[e];
        write!(self.w, "{}", name.ident)?;
        self.add_todo(TypeIndex::Enum(e));
        Ok(())
    }

    pub fn write_field_pointer(&mut self, ptr: &Pointer) -> Result<()> {
        let Pointer { underlying, is_const, .. } = ptr;
        if *is_const {
            write!(self.w, "*const ")?;
        } else {
            write!(self.w, "*mut ")?;
        }
        self.write_class_field_kind(underlying)?;
        Ok(())
    }

    pub fn write_field_class(&mut self, class: ClassIndex) -> Result<()> {
        let Class { name, .. } = &self.arena[class];
        write!(self.w, "{}", name.ident)?;
        self.add_todo(TypeIndex::Class(class));
        Ok(())
    }

    pub fn write_field_bitfield(&mut self, b: &Bitfield) -> Result<()> {
        let size = b.size(self.arena);
        let Bitfield { fields } = b;
        // TODO: write fields above?
        write!(self.w, "u{}", size * 8)?;
        Ok(())
    }

    pub fn write_field_union(&mut self, u: UnionIndex) -> Result<()> {
        let Union { name, .. } = &self.arena[u];
        write!(self.w, "{}", name.ident)?;
        self.add_todo(TypeIndex::Union(u));
        Ok(())
    }

    pub fn write_field_array(&mut self, arr: &Array) -> Result<()> {
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

    pub fn write_field_modifier(&mut self, m: &Modifier) -> Result<()> {
        let Modifier { underlying, constant, .. } = m;
        if *constant {
            write!(self.w, "const ")?;
        }
        self.write_class_field_kind(underlying)
    }

    pub fn write_variant(&mut self, variant: &Variant) -> Result<()> {
        let Variant { name, attributes, .. } = variant;
        if attributes.any() {
            unimplemented!("attributes for enum variant");
        }
        writeln!(self.w, "{}{},", self.indent, name.ident)?;
        Ok(())
    }
}
