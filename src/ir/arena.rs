use std::collections::HashMap;
use std::ops::Index;

use pdb;

use ir::{Class, Enum, Union, Name, Size};

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

#[derive(Debug)]
pub enum Type {
    Class(Class),
    Enum(Enum),
    Union(Union),
}

impl Type {
    pub fn name(&self) -> &Name {
        match self {
            Type::Class(c) => &c.name,
            Type::Enum(e) => &e.name,
            Type::Union(u) => &u.name,
        }
    }
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
        if idx == 6164 || idx == 174410 {
            println!("INSERT: {:?}", class);
        }
        let index = self.insert_custom_class(class);
        self.index_map.insert(idx, TypeIndex::Class(index));
        index
    }
    pub fn insert_custom_class(&mut self, class: Class) -> ClassIndex {
        let index = ClassIndex(self.classes.len());
        self.insert_name(class.name.name.clone(), TypeIndex::Class(index), class.size);
        self.classes.push(class);
        index
    }
    pub fn insert_enum(&mut self, e: Enum, idx: pdb::TypeIndex) -> EnumIndex {
        let index = self.insert_custom_enum(e);
        self.index_map.insert(idx, TypeIndex::Enum(index));
        index
    }
    pub fn insert_custom_enum(&mut self, e: Enum) -> EnumIndex {
        let index = EnumIndex(self.enums.len());
        self.insert_name(e.name.name.clone(), TypeIndex::Enum(index), e.size(self));
        self.enums.push(e);
        index
    }
    pub fn insert_union(&mut self, u: Union, idx: pdb::TypeIndex) -> UnionIndex {
        let index = self.insert_custom_union(u);
        self.index_map.insert(idx, TypeIndex::Union(index));
        index
    }
    pub fn insert_custom_union(&mut self, u: Union) -> UnionIndex {
        let index = UnionIndex(self.unions.len());
        self.insert_name(u.name.name.clone(), TypeIndex::Union(index), u.size);
        self.unions.push(u);
        index
    }

    // For some reason some types are inside the pdb multiple times with
    // varying size and fields.
    // For a string-lookup, we usually only care about the largest one.
    fn insert_name(&mut self, name: String, index: TypeIndex, size: usize) {
        if let Some(old) = self.type_names.get(&name) {
            let old_size = match *old {
                TypeIndex::Class(c) => self[c].size,
                TypeIndex::Enum(e) => self[e].size(self),
                TypeIndex::Union(u) => self[u].size,
            };
            if old_size >= size {
                return;
            }
        }
        self.type_names.insert(name, index);
    }

    pub fn get_largest_type(&self, index: TypeIndex) -> TypeIndex {
        let (current_name, current_size) = match index {
            TypeIndex::Class(c) => (&self[c].name, self[c].size(self)),
            TypeIndex::Enum(e) => (&self[e].name, self[e].size(self)),
            TypeIndex::Union(u) => (&self[u].name, self[u].size(self)),
        };
        let new_index = self[current_name];
        let new_size = match new_index {
            TypeIndex::Class(c) => self[c].size(self),
            TypeIndex::Enum(e) => self[e].size(self),
            TypeIndex::Union(u) => self[u].size(self),
        };
        if new_size > current_size {
            new_index
        } else {
            index
        }
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
