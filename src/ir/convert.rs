use std::collections::VecDeque;
use std::path::Path;
use std::fs::File;

use pdb::{self, PDB, FallibleIterator, TypeInformation, TypeFinder, Error as PdbError, TypeData};
use multimap::MultiMap;

use crate::ir::{Arena, Result, Name, Class, TypeIndex, ClassIndex, EnumIndex, UnionIndex, Enum, Union};

pub struct Converter<'a, 't> {
    pub(in crate::ir) finder: TypeFinder<'t>,
    types: VecDeque<pdb::TypeIndex>,
    pub(in crate::ir) arena: &'a mut Arena,
}

pub fn read<P: AsRef<Path>>(path: P) -> Result<Arena> {
    let mut arena = Arena::new();
    let file = File::open(path)?;
    let mut pdb = PDB::open(file)?;
    let mut info = pdb.type_information()?;
    let mut converter = Converter::new(&mut info, &mut arena)?;
    converter.populate()?;
    Ok(arena)
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
//                        println!("ignore: {:?}", t);
                        continue;
                    }
                    let name = Name::from(t.name().unwrap());
                    if name.starts_with('<') {
                        // ignore anonymous types
//                        println!("ignore: {:?}", t);
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