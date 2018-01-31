use std::collections::VecDeque;
use std::cmp;
use std::io::Write;

use pdb::{self, FieldAttributes, TypeProperties, ClassType, TypeData, BaseClassType, MemberType,
          PointerType, BitfieldType, ArrayType, ModifierType, VirtualBaseClassType};

use ir::{ClassIndex, Name, ClassKind, PrimitiveKind, EnumIndex, UnionIndex, Converter, Result, Size,
         Union, Arena};

#[derive(Debug)]
pub struct Class {
    pub name: Name,
    pub kind: ClassKind,
    pub members: Vec<ClassMember>,
    pub properties: Properties,
    //pub derived_from: Option<ClassIndex>,
    // TODO: vtable_shape
    pub size: usize,
}

impl Class {
    pub fn from(converter: &mut Converter, class: ClassType) -> Result<Class> {
        let ClassType { name, kind, fields, properties, derived_from, size, ..} = class.clone();
        assert_eq!(derived_from, None);
        assert_ne!(kind, ClassKind::Interface);
        let mut members = VecDeque::new();
        if let Some(fields) = fields {
            match converter.finder.find(fields)?.parse()? {
                TypeData::FieldList(list) => {
                    for field in list.fields {
                        if let Ok(Some(member)) = ClassMember::from(converter, field) {
                            members.push_back(member);
                        }
                    }
                }
                t => unreachable!("Not a FieldList {:?}", t)
            }
        }
        let name: Name = name.into();
        if name.name == "UObjectBase" {
            println!();
            println!("{:?}", class);
            println!("{:?}", members);
            println!();
        }
        let members = Class::transform_unions(converter.arena, &name, members);
        let members = Class::transform_bitfields(&name, members);
        Ok(Class {
            name,
            kind,
            members,
            properties: properties.into(),
            size: size as usize,
        })
    }

    /// Converts inline-lying unions into actual unions
    // Assume the following C++ struct with anonymous unions.
    // struct S {
    //   union {
    //     int a;
    //     struct {
    //       int ba;
    //       int bb;
    //     } b;
    //     struct {
    //       int ca;
    //       int cb;
    //       int cc;
    //     } c;
    //   } u;
    // };
    //
    // This will be described in the pdb like this:
    // a : offset 0
    // ba: offset 0
    // bb: offset 4
    // ca: offset 0
    // cb: offset 4
    // cc: offset 8
    //
    // To generate rust types, we need to detect these unions and create new types for them.
    // For simplification, for each union field, we create a new struct.
    fn transform_unions(arena: &mut Arena, name: &Name, mut members: VecDeque<ClassMember>) -> VecDeque<ClassMember> {
        let mut res = VecDeque::with_capacity(members.len());
        let mut union_number = 0;

        while let Some(member) = members.pop_front() {
            let offset = member.offset();
            // if we have a union
            if let Some(position) = members.iter().position(|m| m.offset() == offset) {
                members.push_front(member);
                let mut union_fields = Vec::new();
                let mut max_size = 0;

                // while the union has more fields
                while let Some(position) = members.iter().skip(1).position(|m| m.offset() == offset) {
                    // we consume all fields of the anonymous struct of this union field
                    let mut union_struct: Vec<_> = members.drain(..position+1).collect();
                    let last = &union_struct[union_struct.len()-1];
                    let size = last.offset() - offset + last.size(arena);
                    max_size = cmp::max(max_size, size);

                    // create union-field-struct
                    union_fields.push(ClassField {
                        attributes: Attributes::default(),
                        name: format!("struct{}", union_fields.len()).into(),
                        offset: 0,
                        kind: ClassFieldKind::Class(arena.insert_custom_class(Class {
                            name: format!("{}_Union{}_Struct{}", name.ident, union_number, union_fields.len()).into(),
                            kind: ClassKind::Struct,
                            members: union_struct,
                            properties: Properties::default(),
                            size,
                        })),
                    });
                }

                // The first field after the last union member must have a higher offset
                // than the current offset + size of the union.
                // The last member of the union might actually be larger than previous ones,
                // but we can not get that information from the debug type information.
                // If it is in fact larger, the additional fields will be represented as regular
                // members of the struct.
                let union_struct = if max_size == 0 {
                    eprintln!("I have no idea how large a union of {:?} is.", name);
                    vec![members.pop_front().unwrap()]
                } else {
                    let position = members.iter().position(|m| m.offset() >= offset + max_size);
                    if let Some(end) = position {
                        members.drain(..end).collect()
                    } else {
                        members.drain(..).collect()
                    }
                };
                union_fields.push(ClassField {
                    attributes: Attributes::default(),
                    name: format!("struct{}", union_fields.len()).into(),
                    offset: 0,
                    kind: ClassFieldKind::Class(arena.insert_custom_class(Class {
                        name: format!("{}_Union{}_Struct{}", name.ident, union_number, union_fields.len()).into(),
                        kind: ClassKind::Struct,
                        members: union_struct,
                        properties: Properties::default(),
                        size: max_size,
                    })),
                });
                // We have created all union-field-structs. Now we create the actual union
                // and set it as field of the class we're currently analyzing.
                union_number += 1;
                let count = union_fields.len() as u16;
                res.push_back(ClassMember::Field(ClassField {
                    attributes: Attributes::default(),
                    name: format!("union{}", union_number).into(),
                    offset,
                    kind: ClassFieldKind::Union(arena.insert_custom_union(Union {
                        name: format!("{}_Union{}", name.ident, union_number).into(),
                        fields: union_fields,
                        properties: Properties::default(),
                        size: max_size,
                        count,
                    })),
                }));
            } else {
                res.push_back(member);
            }
        }
        res
    }

    fn transform_bitfields(name: &Name, mut members: VecDeque<ClassMember>) -> Vec<ClassMember> {
        let mut res = Vec::with_capacity(members.len());
        let mut bitfield_number = 0;
        let mut pos = usize::max_value();
        let mut offset = 0;
        let mut fields = Vec::new();
        while let Some(member) = members.pop_front() {
            if let ClassMember::Field(ClassField { offset: offs, kind: ClassFieldKind::Bitfield(mut b), .. }) = member {
                assert_eq!(b.fields.len(), 1);
                let field = b.fields.pop().unwrap();
                if field.position < pos && !fields.is_empty() {
                    // new bitfield after bitfield, need to finish old one
                    res.push(ClassMember::Field(ClassField {
                        attributes: Attributes::default(),
                        name: format!("bitfield{}", bitfield_number).into(),
                        offset,
                        kind: ClassFieldKind::Bitfield(Bitfield {
                            fields,
                        })
                    }));
                    bitfield_number += 1;
                    offset = offs;
                    pos = field.position;
                    fields = vec![field];
                } else {
                    offset = offs;
                    fields.push(field);
                }
            } else {
                // if we had a bitfield before, we need to finish it
                if !fields.is_empty() {
                    res.push(ClassMember::Field(ClassField {
                        attributes: Attributes::default(),
                        name: format!("bitfield{}", bitfield_number).into(),
                        offset,
                        kind: ClassFieldKind::Bitfield(Bitfield {
                            fields,
                        })
                    }));
                    pos = usize::max_value();
                    bitfield_number += 1;
                    fields = Vec::new();
                }
                res.push(member);
            }
        }
        res
    }
}

#[derive(Debug)]
pub enum ClassMember {
    Vtable,
    BaseClass(BaseClass),
    VirtualBaseClass(VirtualBaseClass),
    Field(ClassField),
}

impl ClassMember {
    pub fn from(converter: &mut Converter, typ: TypeData) -> Result<Option<ClassMember>> {
        Ok(match typ {
            TypeData::BaseClass(class) => Some(ClassMember::BaseClass(BaseClass::from(converter, class)?)),
            TypeData::Member(field) => Some(ClassMember::Field(ClassField::from(converter, field)?)),
            TypeData::VirtualBaseClass(class) => Some(ClassMember::VirtualBaseClass(VirtualBaseClass::from(converter, class)?)),
            TypeData::VirtualFunctionTablePointer(typ) => Some(ClassMember::Vtable),
            TypeData::MemberFunction(_) => None,
            TypeData::OverloadedMethod(_) => None,
            TypeData::Method(_) => None,
            TypeData::Nested(_) => None,
            TypeData::StaticMember(_) => None,
            t => unimplemented!("ClassMember: {:?}", t)
        })
    }

    pub fn offset(&self) -> usize {
        match self {
            // please be a nice compiler
            ClassMember::Vtable => 0,
            ClassMember::BaseClass(base) => base.offset,
            ClassMember::VirtualBaseClass(base) => base.base_pointer_offset,
            ClassMember::Field(field) => field.offset,
        }
    }
}

#[derive(Debug)]
pub struct BaseClass {
    pub attributes: Attributes,
    pub offset: usize,
    pub base_class: ClassIndex,
}

impl BaseClass {
    pub fn from(converter: &mut Converter, class: BaseClassType) -> Result<BaseClass> {
        let BaseClassType { attributes, kind, offset, base_class, .. } = class;
        let base_class = converter.convert_class(base_class)?;
//        assert_eq!(converter.arena[base_class].kind, kind, "{:?}\n\n{:?}", converter.arena[base_class], class);

        Ok(BaseClass {
            attributes: attributes.into(),
            offset: offset as usize,
            base_class,
        })
    }
}

#[derive(Debug)]
pub struct VirtualBaseClass {
    pub direct: bool,
    pub attributes: Attributes,
    pub base_class: ClassIndex,
    // TODO: base_pointer
    pub base_pointer_offset: usize,
    pub virtual_base_offset: usize,
}

impl VirtualBaseClass {
    pub fn from(converter: &mut Converter, class: VirtualBaseClassType) -> Result<VirtualBaseClass> {
        let VirtualBaseClassType { direct, attributes, base_class, base_pointer,
            base_pointer_offset, virtual_base_offset }  = class;
        let base_class = converter.convert_class(base_class)?;
        Ok(VirtualBaseClass {
            direct,
            attributes: attributes.into(),
            base_class,
            base_pointer_offset: base_pointer_offset as usize,
            virtual_base_offset: virtual_base_offset as usize,
        })
    }
}

#[derive(Debug)]
pub struct ClassField {
    pub attributes: Attributes,
    pub name: Name,
    pub offset: usize,
    pub kind: ClassFieldKind,
}

impl ClassField {
    pub fn from(converter: &mut Converter, field: MemberType) -> Result<ClassField> {
        let MemberType { attributes, name, offset, field_type, .. } = field;
        let kind = ClassFieldKind::from(converter, field_type)?;
        Ok(ClassField {
            attributes: attributes.into(),
            name: name.into(),
            offset: offset as usize,
            kind,
        })
    }
}

#[derive(Debug)]
pub enum ClassFieldKind {
    // TODO: Do we need PrimitiveType::indirection?
    Primitive(PrimitiveKind),
    Enum(EnumIndex),
    Pointer(Box<Pointer>),
    Class(ClassIndex),
    Bitfield(Bitfield),
    Union(UnionIndex),
    Array(Box<Array>),
    Modifier(Box<Modifier>),
    // TODO: Arguments
    Procedure,
    // TODO: Arguments
    MemberFunction,
    // TODO: Arguments
    Method,
}

impl ClassFieldKind {
    pub fn from(converter: &mut Converter, idx: pdb::TypeIndex) -> Result<ClassFieldKind> {
        let typ = converter.finder.find(idx)?.parse()?;
        Ok(match typ {
            TypeData::Primitive(kind) => ClassFieldKind::Primitive(kind.kind),
            TypeData::Enumeration(_) => ClassFieldKind::Enum(converter.convert_enum(idx)?),
            TypeData::Pointer(ptr) => ClassFieldKind::Pointer(Box::new(Pointer::from(converter, ptr)?)),
            TypeData::Class(_) => ClassFieldKind::Class(converter.convert_class(idx)?),
            TypeData::Bitfield(bitfield) => ClassFieldKind::Bitfield(Bitfield::from(converter, bitfield)?),
            TypeData::Union(_) => ClassFieldKind::Union(converter.convert_union(idx)?),
            TypeData::Array(array) => ClassFieldKind::Array(Box::new(Array::from(converter, array)?)),
            TypeData::Modifier(modifier) => ClassFieldKind::Modifier(Box::new(Modifier::from(converter, modifier)?)),
            TypeData::Procedure(_) => ClassFieldKind::Procedure,
            TypeData::MemberFunction(_) => ClassFieldKind::MemberFunction,
            TypeData::Method(_) => ClassFieldKind::Method,
            t => unimplemented!("ClassFieldKind: {:?}", t)
        })
    }
}

#[derive(Debug)]
pub struct Pointer {
    pub underlying: ClassFieldKind,
    pub typ: u8,
    pub is_const: bool,
    pub is_reference: bool,
    pub size: usize,
}

impl Pointer {
    pub fn from(converter: &mut Converter, ptr: PointerType) -> Result<Pointer> {
        let PointerType { attributes, underlying_type } = ptr;
        let underlying = ClassFieldKind::from(converter, underlying_type)?;
        Ok(Pointer {
            underlying,
            typ: attributes.pointer_type(),
            is_const: attributes.is_const(),
            is_reference: attributes.is_reference(),
            size: attributes.size() as usize,
        })
    }
}

#[derive(Default, Debug)]
pub struct Attributes {
    pub is_static: bool,
    pub is_virtual: bool,
    pub is_pure_virtual: bool,
    pub is_intro_virtual: bool,
}

impl From<FieldAttributes> for Attributes {
    fn from(attrs: FieldAttributes) -> Attributes {
        Attributes {
            is_static: attrs.is_static(),
            is_virtual: attrs.is_virtual(),
            is_pure_virtual: attrs.is_pure_virtual(),
            is_intro_virtual: attrs.is_intro_virtual(),
        }
    }
}

impl Attributes {
    pub fn any(&self) -> bool {
        self.is_static || self.is_virtual || self.is_pure_virtual || self.is_intro_virtual
    }
}

#[derive(Default, Debug)]
pub struct Properties {
    pub packed: bool,
    pub constructors: bool,
    pub overloaded_operators: bool,
    pub nested_type: bool,
    pub contains_nested_types: bool,
    pub overloaded_assignment: bool,
    pub overloaded_casting: bool,
    pub forward_reference: bool,
    pub scoped_definitions: bool,
    pub has_unique_name: bool,
    pub sealed: bool,
    pub hfa: u8,
    pub intrinsic_type: bool,
    pub mocom: u8,
}

impl From<TypeProperties> for Properties {
    fn from(props: TypeProperties) -> Properties {
        Properties {
            packed: props.packed(),
            constructors: props.constructors(),
            overloaded_operators: props.overloaded_operators(),
            nested_type: props.is_nested_type(),
            contains_nested_types: props.contains_nested_types(),
            overloaded_assignment: props.overloaded_assignment(),
            overloaded_casting: props.overloaded_casting(),
            forward_reference: props.forward_reference(),
            scoped_definitions: props.scoped_definition(),
            has_unique_name: props.has_unique_name(),
            sealed: props.sealed(),
            hfa: props.hfa(),
            intrinsic_type: props.intrinsic_type(),
            mocom: props.mocom(),
        }
    }
}

#[derive(Debug)]
pub struct Bitfield {
    pub fields: Vec<BitfieldField>,
}

impl Bitfield {
    pub fn from(converter: &mut Converter, bitfield: BitfieldType) -> Result<Bitfield> {
        // We can only get a single bitfield here. Later, we'll make a second pass to
        // collect adjacent bitfield fields into a single large bitfield.
        Ok(Bitfield {
            fields: vec![BitfieldField::from(converter, bitfield)?],
        })
    }
}

#[derive(Debug)]
pub enum BitfieldUnderlying {
    Primitive(PrimitiveKind),
    Enum(EnumIndex),
}

#[derive(Debug)]
pub struct BitfieldField {
    pub underlying: BitfieldUnderlying,
    pub length: usize,
    pub position: usize,
}

impl BitfieldField {
    pub fn from(converter: &mut Converter, bitfield: BitfieldType) -> Result<BitfieldField> {
        let BitfieldType { length, position, underlying_type } = bitfield;
        let underlying = BitfieldField::underlying(converter, underlying_type)?;

        Ok(BitfieldField {
            underlying,
            length: length as usize,
            position: position as usize,
        })
    }

    fn underlying(converter: &mut Converter, underlying_type: pdb::TypeIndex) -> Result<BitfieldUnderlying> {
        let typ = converter.finder.find(underlying_type)?.parse()?;

        Ok(match typ {
            TypeData::Primitive(primitive) => BitfieldUnderlying::Primitive(primitive.kind),
            TypeData::Enumeration(typ) =>
                BitfieldUnderlying::Enum(converter.convert_enum(underlying_type)?),
            TypeData::Modifier(m) => BitfieldField::underlying(converter, m.underlying_type)?,
            t => unimplemented!("Bitfield is {:?}", t),
        })
    }
}

#[derive(Debug)]
pub struct Array {
    pub element_type: ClassFieldKind,
    // TODO: indexing_type
    pub stride: Option<u32>,
    pub dimensions: Vec<usize>,
}

impl Array {
    pub fn from(converter: &mut Converter, array: ArrayType) -> Result<Array> {
        let ArrayType { element_type, dimensions, stride, .. } = array;
        let element_type = ClassFieldKind::from(converter, element_type)?;
        let mut size_so_far = element_type.size(converter.arena);
        let dimensions = if size_so_far == 0 {
            // For some reason it's ok for types with an actual size to be zero-sized in the pdb.
            dimensions.into_iter().map(|i| i as usize).collect()
        } else {
            dimensions.into_iter().map(|i| {
                let dim = i as usize / size_so_far;
                size_so_far = i as usize;
                dim
            }).collect()
        };
        Ok(Array {
            element_type,
            stride,
            dimensions,
        })
    }
}

#[derive(Debug)]
pub struct Modifier {
    pub underlying: ClassFieldKind,
    pub constant: bool,
    pub volatile: bool,
    pub unaligned: bool,
}

impl Modifier {
    pub fn from(converter: &mut Converter, modifier: ModifierType) -> Result<Modifier> {
        let ModifierType { underlying_type, constant, volatile, unaligned } = modifier;
        let underlying = ClassFieldKind::from(converter, underlying_type)?;
        Ok(Modifier {
            underlying,
            constant,
            volatile,
            unaligned,
        })
    }
}
