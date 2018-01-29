use pdb::{UnionType, TypeData};

use ir::{Name, ClassField, Properties, Converter, Result};

#[derive(Debug)]
pub struct Union {
    pub name: Name,
    pub fields: Vec<ClassField>,
    pub properties: Properties,
    pub size: usize,
    pub count: u16,
}

impl Union {
    pub fn from(converter: &mut Converter, u: UnionType) -> Result<Union> {
        let UnionType { name, fields, properties, size, count } = u;
        let mut members = Vec::new();
        match converter.finder.find(fields)?.parse()? {
            TypeData::FieldList(list) => {
                for field in list.fields {
                    match field {
                        TypeData::Member(member) =>
                            members.push(ClassField::from(converter, member)?),
                        t => unreachable!("not a member {:?}", t)
                    }
                }
            }
            t => unreachable!("Not a FieldList {:?}", t)
        }
        Ok(Union {
            name: name.into(),
            fields: members,
            properties: properties.into(),
            size: size as usize,
            count,
        })
    }
}