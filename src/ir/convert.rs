use std::collections::VecDeque;

use pdb::{self, FallibleIterator, TypeInformation, TypeFinder, Error as PdbError, TypeData};
use multimap::MultiMap;

use ir::{Arena, Result, Name, Class, TypeIndex, ClassIndex, EnumIndex, UnionIndex, Enum, Union};

pub struct Converter<'a, 't> {
    pub(in ir) finder: TypeFinder<'t>,
    types: VecDeque<pdb::TypeIndex>,
    pub(in ir) arena: &'a mut Arena,
}

impl<'a, 't, 's: 't> Converter<'a, 't> {
    pub fn new(info: &'t mut TypeInformation<'s>, arena: &'a mut Arena) -> Result<Converter<'a, 't>> {
        let mut finder = info.new_type_finder();
        let mut iter = info.iter();
        let mut types = VecDeque::new();
        while let Some(typ) = iter.next()? {
            finder.update(&iter);
            match typ.parse() {
                Ok(t) => {
                    if t.name().is_none() {
                        println!("ignore: {:?}", t);
                        continue;
                    }
                    let name = Name::from(t.name().unwrap());
                    if name.starts_with('<') {
                        // ignore anonymous types
                        println!("ignore: {:?}", t);
                        continue;
                    }
                    types.push_back(typ.type_index());
                }
                Err(PdbError::UnimplementedTypeKind(_)) => {},
                Err(e) => Err(e)?,
            }
        }
        Ok(Converter {
            finder,
            types,
            arena
        })
    }

    pub fn populate(&mut self) -> Result<()> {
        while let Some(idx) = self.types.pop_front() {
            self.convert(idx)?;
        }
        Ok(())
    }

    pub fn convert(&mut self, idx: pdb::TypeIndex) -> Result<TypeIndex> {
        if let Some(&index) = self.arena.index_map().get(&idx) {
            return Ok(index);
        }
        let typ = self.finder.find(idx)?.parse()?;
        Ok(match typ {
            TypeData::Class(class) => {
                let class = Class::from(self, class)?;
                TypeIndex::Class(self.arena.insert_class(class, idx))
            }
            TypeData::Enumeration(e) => {
                let e = Enum::from(self, e)?;
                TypeIndex::Enum(self.arena.insert_enum(e, idx))
            }
            TypeData::Union(u) => {
                let u = Union::from(self, u)?;
                TypeIndex::Union(self.arena.insert_union(u, idx))
            }
            _ => unimplemented!()
        })
    }

    pub fn convert_class(&mut self, idx: pdb::TypeIndex) -> Result<ClassIndex> {
        match self.convert(idx)? {
            TypeIndex::Class(index) => Ok(index),
            _ => unreachable!("Y u giv me no class?")
        }
    }
    pub fn convert_enum(&mut self, idx: pdb::TypeIndex) -> Result<EnumIndex> {
        match self.convert(idx)? {
            TypeIndex::Enum(index) => Ok(index),
            _ => unreachable!("Y u giv me no enum?")
        }
    }
    pub fn convert_union(&mut self, idx: pdb::TypeIndex) -> Result<UnionIndex> {
        match self.convert(idx)? {
            TypeIndex::Union(index) => Ok(index),
            _ => unreachable!("Y u giv me no union?")
        }
    }
}