use std::collections::VecDeque;

use pdb::{self, FallibleIterator, TypeInformation, Error as PdbError, TypeData, TypeFinder};

use crate::ir::{Arena, Name, Class, TypeIndex, ClassIndex, EnumIndex, UnionIndex, Enum, Union};
use crate::Result;

pub struct Converter<'a, 't> {
    finder: TypeFinder<'t>,
    pdb_type_indexes: VecDeque<pdb::TypeIndex>,
    pub(in crate::ir) arena: &'a mut Arena,
}

impl<'a, 't, 's: 't> Converter<'a, 't> {
    pub fn new(info: &'t TypeInformation<'s>, arena: &'a mut Arena) -> Result<Converter<'a, 't>> {
        let mut finder = info.new_type_finder();
        let mut iter = info.iter();
        finder.update(&iter);
        let mut pdb_type_indexes = VecDeque::new();
        while let Some(typ) = iter.next()? {
            finder.update(&iter);
            match typ.parse() {
                Ok(t) => {
                    if t.name().is_none() {
                        log::info!("ignore: {t:?}");
                        continue;
                    }
                    let name = Name::from(t.name().unwrap());
                    if name.starts_with('<') {
                        // ignore anonymous types
                        log::info!("ignore: {t:?}");
                        continue;
                    }
                    pdb_type_indexes.push_back(typ.type_index());
                }
                Err(PdbError::UnimplementedTypeKind(_)) => {},
                Err(e) => Err(e)?,
            }
        }
        Ok(Converter {
            finder,
            pdb_type_indexes,
            arena,
        })
    }

    pub fn populate(&mut self) -> Result<()> {
        while let Some(idx) = self.pdb_type_indexes.pop_front() {
            self.convert(idx)?;
        }
        Ok(())
    }

    pub(in crate::ir) fn pdb_type(&self, idx: pdb::TypeIndex) -> TypeData<'t> {
        self.finder.find(idx).unwrap().parse().unwrap()
    }

    fn convert(&mut self, idx: pdb::TypeIndex) -> Result<TypeIndex> {
        if let Some(&index) = self.arena.index_map().get(&idx) {
            return Ok(index);
        }
        let typ = self.pdb_type(idx);
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