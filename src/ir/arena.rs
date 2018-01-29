use std::collections::HashMap;
use std::ops::Index;

use pdb;

use ir::{Class, Enum, Union, Name};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct ClassIndex(pub usize);
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct EnumIndex(pub usize);
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct UnionIndex(pub usize);
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum TypeIndex {
    Class(ClassIndex),
    Enum(EnumIndex),
    Union(UnionIndex),
}

pub struct Arena {
    classes: Vec<Class>,
    enums: Vec<Enum>,
    unions: Vec<Union>,
    type_names: HashMap<String, TypeIndex>,
    index_map: HashMap<pdb::TypeIndex, TypeIndex>,
}

impl Arena {
    pub fn new() -> Arena {
        Arena {
            classes: Vec::new(),
            enums: Vec::new(),
            unions: Vec::new(),
            type_names: HashMap::new(),
            index_map: HashMap::new(),
        }
    }

    pub fn classes(&self) -> &Vec<Class> {
        &self.classes
    }

    pub fn enums(&self) -> &Vec<Enum> {
        &self.enums
    }

    pub fn unions(&self) -> &Vec<Union> {
        &self.unions
    }

    pub fn type_names(&self) -> &HashMap<String, TypeIndex> {
        &self.type_names
    }

    pub fn index_map(&self) -> &HashMap<pdb::TypeIndex, TypeIndex> {
        &self.index_map
    }

    pub fn insert_class(&mut self, class: Class, idx: pdb::TypeIndex) -> ClassIndex {
        let index = self.insert_custom_class(class);
        self.index_map.insert(idx, TypeIndex::Class(index));
        index
    }
    pub fn insert_custom_class(&mut self, class: Class) -> ClassIndex {
        let index = ClassIndex(self.classes.len());
        self.type_names.insert(class.name.name.clone(), TypeIndex::Class(index));
        self.classes.push(class);
        index
    }
    pub fn insert_enum(&mut self, e: Enum, idx: pdb::TypeIndex) -> EnumIndex {
        let index = self.insert_custom_enum(e);
        self.index_map.insert(idx, TypeIndex::Enum(index));
        index
    }
    pub fn insert_custom_enum(&mut self, e: Enum) -> EnumIndex {
        let index = EnumIndex(self.classes.len());
        self.type_names.insert(e.name.name.clone(), TypeIndex::Enum(index));
        self.enums.push(e);
        index
    }
    pub fn insert_union(&mut self, u: Union, idx: pdb::TypeIndex) -> UnionIndex {
        let index = self.insert_custom_union(u);
        self.index_map.insert(idx, TypeIndex::Union(index));
        index
    }
    pub fn insert_custom_union(&mut self, u: Union) -> UnionIndex {
        let index = UnionIndex(self.classes.len());
        self.type_names.insert(u.name.name.clone(), TypeIndex::Union(index));
        self.unions.push(u);
        index
    }
}

impl Index<ClassIndex> for Arena {
    type Output = Class;

    fn index(&self, index: ClassIndex) -> &Class {
        &self.classes[index.0]
    }
}

impl Index<EnumIndex> for Arena {
    type Output = Enum;

    fn index(&self, index: EnumIndex) -> &Enum {
        &self.enums[index.0]
    }
}

impl Index<UnionIndex> for Arena {
    type Output = Union;

    fn index(&self, index: UnionIndex) -> &Union {
        &self.unions[index.0]
    }
}

impl<T: AsRef<str>> Index<T> for Arena {
    type Output = TypeIndex;

    fn index(&self, index: T) -> &TypeIndex {
        &self.type_names[index.as_ref()]
    }
}

impl<'a> Index<&'a Name> for Arena {
    type Output = TypeIndex;

    fn index(&self, index: &Name) -> &TypeIndex {
        &self.type_names[&index.name]
    }
}
