use std::collections::HashMap;
use std::ops::{Index, IndexMut};

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
        self.insert_name(class.name.name.clone(), TypeIndex::Class(index), class.size, class.members.len());
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
        self.insert_name(e.name.name.clone(), TypeIndex::Enum(index), e.size(self), e.variants.len());
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
        self.insert_name(u.name.name.clone(), TypeIndex::Union(index), u.size, u.fields.len());
        self.unions.push(u);
        index
    }

    // For some reason some types are inside the pdb multiple times with
    // varying size and fields.
    // For a string-lookup, we usually only care about the largest one.
    fn insert_name(&mut self, name: String, index: TypeIndex, size: usize, fields_len: usize) {
        if let Some(old) = self.type_names.get(&name) {
            let (old_size, old_fields) = match *old {
                TypeIndex::Class(c) => (self[c].size(self), self[c].members.len()),
                TypeIndex::Enum(e) => (self[e].size(self), self[e].variants.len()),
                TypeIndex::Union(u) => (self[u].size(self), self[u].fields.len()),
            };
            if old_size >= size && old_fields >= fields_len {
                return;
            }
        }
        self.type_names.insert(name, index);
    }

    pub fn get_class(&self, index: ClassIndex) -> &Class {
        &self.classes[index.0]
    }
    pub fn get_class_mut(&mut self, index: ClassIndex) -> &mut Class {
        &mut self.classes[index.0]
    }
    pub fn get_enum(&self, index: EnumIndex) -> &Enum {
        &self.enums[index.0]
    }
    pub fn get_enum_mut(&mut self, index: EnumIndex) -> &mut Enum {
        &mut self.enums[index.0]
    }
    pub fn get_union(&self, index: UnionIndex) -> &Union {
        &self.unions[index.0]
    }
    pub fn get_union_mut(&mut self, index: UnionIndex) -> &mut Union {
        &mut self.unions[index.0]
    }

    pub fn get_largest_class_index(&self, index: ClassIndex) -> ClassIndex {
        let class = self.get_class(index);
        let new_index = match self[&class.name] {
            TypeIndex::Class(c) => c,
            _ => unreachable!()
        };
        let new_class = self.get_class(new_index);
        if class.size(self) >= new_class.size(self) && class.members.len() >= new_class.members.len() {
            index
        } else {
            new_index
        }
    }
    pub fn get_largest_class(&self, index: ClassIndex) -> &Class {
        self.get_class(self.get_largest_class_index(index))
    }
    pub fn get_largest_enum_index(&self, index: EnumIndex) -> EnumIndex {
        let e = self.get_enum(index);
        let new_index = match self[&e.name] {
            TypeIndex::Enum(e) => e,
            _ => unreachable!()
        };
        let new_e = self.get_enum(new_index);
        if e.size(self) >= new_e.size(self) && e.variants.len() >= new_e.variants.len() {
            index
        } else {
            new_index
        }
    }
    pub fn get_largest_enum(&self, index: EnumIndex) -> &Enum {
        self.get_enum(self.get_largest_enum_index(index))
    }
    pub fn get_largest_union_index(&self, index: UnionIndex) -> UnionIndex {
        let u = self.get_union(index);
        let new_index = match self[&u.name] {
            TypeIndex::Union(u) => u,
            _ => unreachable!()
        };
        let new_u = self.get_union(new_index);
        if u.size(self) >= new_u.size(self) && u.fields.len() >= new_u.fields.len() {
            index
        } else {
            new_index
        }
    }
    pub fn get_largest_union(&self, index: UnionIndex) -> &Union {
        self.get_union(self.get_largest_union_index(index))
    }
    pub fn get_largest_type_index(&self, index: TypeIndex) -> TypeIndex {
        match index {
            TypeIndex::Class(c) => TypeIndex::Class(self.get_largest_class_index(c)),
            TypeIndex::Enum(e) => TypeIndex::Enum(self.get_largest_enum_index(e)),
            TypeIndex::Union(u) => TypeIndex::Union(self.get_largest_union_index(u))
        }
    }
}

impl Index<ClassIndex> for Arena {
    type Output = Class;

    fn index(&self, index: ClassIndex) -> &Class {
        self.get_class(index)
    }
}
impl IndexMut<ClassIndex> for Arena {
    fn index_mut(&mut self, index: ClassIndex) -> &mut Class {
        self.get_class_mut(index)
    }
}

impl Index<EnumIndex> for Arena {
    type Output = Enum;

    fn index(&self, index: EnumIndex) -> &Enum {
        self.get_enum(index)
    }
}
impl IndexMut<EnumIndex> for Arena {
    fn index_mut(&mut self, index: EnumIndex) -> &mut Enum {
        self.get_enum_mut(index)
    }
}

impl Index<UnionIndex> for Arena {
    type Output = Union;

    fn index(&self, index: UnionIndex) -> &Union {
        self.get_union(index)
    }
}
impl IndexMut<UnionIndex> for Arena {
    fn index_mut(&mut self, index: UnionIndex) -> &mut Union {
        self.get_union_mut(index)
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
