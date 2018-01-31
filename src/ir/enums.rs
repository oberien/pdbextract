use pdb::{EnumerateType, EnumerationType, TypeData};

use ir::{Name, PrimitiveKind, EnumValue, Properties, Attributes, Converter, Result};

#[derive(Debug)]
pub struct Enum {
    pub name: Name,
    pub underlying: PrimitiveKind,
    pub variants: Vec<Variant>,
    pub properties: Properties,
    pub count: usize,
}

impl Enum {
    pub fn from(converter: &mut Converter, e: EnumerationType) -> Result<Enum> {
        let EnumerationType { name, underlying_type, fields, properties, count, .. } = e;
        let underlying = converter.finder.find(underlying_type)?.parse()?;
        let underlying = match underlying {
            TypeData::Primitive(primitive) => primitive.kind,
            t => unreachable!("Enum with underlying {:?}", t)
        };
        let mut variants = Vec::new();
        // pdb contains empty versions of some enums
        if fields != 0 {
            match converter.finder.find(fields)?.parse()? {
                TypeData::FieldList(list) => {
                    for field in list.fields {
                        match field {
                            TypeData::Enumerate(variant) => variants.push(variant.into()),
                            t => unreachable!("not an Enumerate {:?}", t)
                        }
                    }
                },
                t => unreachable!("not a FieldList {:?}", t)
            }
        }

        Ok(Enum {
            name: name.into(),
            underlying,
            variants,
            properties: properties.into(),
            count: count as usize,
        })
    }
}

#[derive(Debug)]
pub struct Variant {
    pub name: Name,
    pub attributes: Attributes,
    pub value: EnumValue,
}

impl<'t> From<EnumerateType<'t>> for Variant {
    fn from(e: EnumerateType) -> Variant {
        let EnumerateType { name, attributes, value, .. } = e;
        Variant {
            name: name.into(),
            attributes: attributes.into(),
            value,
        }
    }
}

