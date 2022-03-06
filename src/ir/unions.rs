use std::collections::VecDeque;
use pdb::{UnionType, TypeData, ClassKind};
use crate::ir::{Name, ClassField, Properties, Converter, Size, Class, ClassMember, ClassFieldKind};
use crate::{Alignment, Arena, Result};

#[derive(Debug)]
pub struct Union {
    pub name: Name,
    pub fields: Vec<ClassField>,
    pub properties: Properties,
    pub size: usize,
    pub count: u16,
    pub alignment: Alignment,
}

impl Union {
    pub fn from(converter: &mut Converter, u: UnionType) -> Result<Union> {
        let UnionType { name, fields, properties, size, count } = u;
        let mut members = VecDeque::new();
        // pdb contains empty versions of some unions
        if fields != 0 {
            match converter.pdb_type(fields) {
                TypeData::FieldList(list) => {
                    let mut peekable = list.fields.into_iter().peekable();
                    let mut last_offset = 0;
                    while let Some(field) = peekable.next() {
                        let max_size = peekable.peek().map(|t| match t {
                            TypeData::Member(field) => field.offset as usize - last_offset,
                            _ => usize::MAX,
                        }).unwrap_or(size as usize - last_offset);
                        match field {
                            TypeData::Member(member) => {
                                last_offset = member.offset as usize;
                                members.push_back(ClassField::from(converter, member, max_size)?);
                            }
                            TypeData::Nested(_) => {},
                            TypeData::Method(_) => {},
                            t => unreachable!("not a member {:?}", t)
                        }
                    }
                }
                t => unreachable!("Not a FieldList {:?}", t)
            }
        }
        let name = name.into();
        let members = Self::transform_inline_structs(&mut converter.arena, &name, members);
        Ok(Union {
            name: name.into(),
            fields: members,
            properties: properties.into(),
            size: size as usize,
            count,
            alignment: Alignment::None,
        })
    }

    /// Converts inline-lying structs into actual structs
    // Assume the following C++ union with anonymous structs.
    // union U {
    //   struct {
    //     float a;
    //     float b;
    //   };
    //   struct {
    //     int c;
    //     int d;
    //   };
    // };
    //
    // This will be described in the pdb like this:
    // a: offset 0
    // b: offset 4
    // c: offset 0
    // d: offset 4
    //
    // To generate rust types, we need to detect these inner structs and create new types for them.
    // For simplification, for each substruct (even if its just a single field), we create a new struct.
    fn transform_inline_structs(arena: &mut Arena, name: &Name, mut fields: VecDeque<ClassField>) -> Vec<ClassField> {
        let mut res = Vec::with_capacity(fields.len());
        let mut struct_number = 0;

        while let Some(field) = fields.pop_front() {
            assert_eq!(0, field.offset);
            if fields.front().is_none() || fields.front().unwrap().offset == 0 {
                res.push(field);
                continue;
            }

            // we have an inline struct
            let mut inner_members = vec![ClassMember::Field(field)];
            while fields.front().is_some() && fields.front().unwrap().offset != 0 {
                inner_members.push(ClassMember::Field(fields.pop_front().unwrap()));
            }
            let last = inner_members.last().unwrap();
            let size = last.offset() + last.size(arena);
            let inner_struct_index = arena.insert_custom_class(Class {
                name: format!("{}_Struct{}", name.ident, struct_number).into(),
                kind: ClassKind::Struct,
                members: inner_members,
                properties: Default::default(),
                size,
                alignment: Alignment::None,
            });
            res.push(ClassField {
                attributes: Default::default(),
                name: format!("struct{}", struct_number).into(),
                offset: 0,
                kind: ClassFieldKind::Class(inner_struct_index),
                max_size: size,
            });
            struct_number += 1;
        }

        res
    }
}