use crate::ir::*;

pub trait Size {
    fn size(&self, arena: &Arena) -> usize;
}

impl Size for Class {
    fn size(&self, _: &Arena) -> usize {
        self.size
    }
}

impl Size for ClassMember {
    fn size(&self, arena: &Arena) -> usize {
        match self {
            // TODO: get actual pdb pointer size
            ClassMember::Vtable => 4,
            ClassMember::BaseClass(class) => class.size(arena),
            ClassMember::VirtualBaseClass(class) => class.size(arena),
            ClassMember::Field(field) => field.size(arena),
        }
    }
}

impl Size for BaseClass {
    fn size(&self, arena: &Arena) -> usize {
        arena[self.base_class].size
    }
}

impl Size for VirtualBaseClass {
    fn size(&self, arena: &Arena) -> usize {
        arena[self.base_class].size
    }
}

impl Size for ClassField {
    fn size(&self, arena: &Arena) -> usize {
        self.kind.size(arena)
    }
}

impl Size for ClassFieldKind {
    fn size(&self, arena: &Arena) -> usize {
        match *self {
            ClassFieldKind::Primitive(primitive) => primitive.size(arena),
            ClassFieldKind::Enum(e) => arena.get_largest_enum(e).size(arena),
            // TODO: get actual pdb pointer size
            ClassFieldKind::Pointer(_) => 4,
            ClassFieldKind::Class(c) => arena.get_largest_class(c).size(arena),
            ClassFieldKind::Bitfield(ref b) => b.size(arena),
            ClassFieldKind::Union(u) => arena.get_largest_union(u).size(arena),
            ClassFieldKind::Array(ref a) => a.size(arena),
            ClassFieldKind::Modifier(ref m) => m.size(arena),
            // ignore because those aren't actual fields
            ClassFieldKind::Procedure => 0,
            ClassFieldKind::MemberFunction => 0,
            ClassFieldKind::Method => 0,
        }
    }
}

impl Size for PrimitiveKind {
    fn size(&self, _: &Arena) -> usize {
        match *self {
            PrimitiveKind::Void => 0,
            PrimitiveKind::Char => 1,
            PrimitiveKind::UChar => 1,
            PrimitiveKind::RChar => 1,
            PrimitiveKind::RChar16 => 2,
            PrimitiveKind::RChar32 => 4,
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
            PrimitiveKind::HRESULT => 4,
            t => unimplemented!("size: primitive: {:?}", t)
        }
    }
}

impl Size for Enum {
    fn size(&self, arena: &Arena) -> usize {
        self.underlying.size(arena)
    }
}

impl Size for Bitfield {
    fn size(&self, arena: &Arena) -> usize {
        self.fields.iter().map(|f| f.size(arena)).max().unwrap()
    }
}

impl Size for BitfieldField {
    fn size(&self, arena: &Arena) -> usize {
        match self.underlying {
            BitfieldUnderlying::Primitive(primitive) => primitive.size(arena),
            BitfieldUnderlying::Enum(e) => arena[e].size(arena),
        }
    }
}

impl Size for Union {
    fn size(&self, _: &Arena) -> usize {
        self.size
    }
}

impl Size for Array {
    fn size(&self, arena: &Arena) -> usize {
        self.dimensions.iter().cloned().product::<usize>() * self.element_type.size(arena)
    }
}

impl Size for Modifier {
    fn size(&self, arena: &Arena) -> usize {
        self.underlying.size(arena)
    }
}
