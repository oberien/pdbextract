use std::env;
use std::io;

use pdbextract::ir::*;

enum State {
    Struct,
    Ignore,
    Replace,
}

fn main() {
    let mut args = env::args().skip(1);
    let file = args.next().unwrap();
    let mut structs = Vec::new();
    let mut ignore = Vec::new();
    let mut replace = Vec::new();
    let mut state = State::Struct;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--struct" => {
                state = State::Struct;
                continue
            }
            "--ignore" => {
                state = State::Ignore;
                continue;
            }
            "--replace" => {
                state = State::Replace;
                continue;
            }
            _ => {
                match state {
                    State::Struct => structs.push(arg),
                    State::Ignore => ignore.push(arg),
                    State::Replace => replace.push((arg, args.next().unwrap())),
                };
            }
        }
    }

    let mut arena = read(&file).unwrap();
    println!("parsed");
    //let character = get_class(&arena, "AMyCharacter");
    //character.check_offsets(&arena);
    let mut writer = Writer::new(io::stdout(), &arena);
    for s in structs {
        writer.write_type(arena[&s]);
    }
    //writer.write_type(arena["AMyCharacter"]);
    //writer.write_type(arena["USceneComponent"]);
    //writer.write_type(arena["UCharacterMovementComponent"]);
    //writer.write_type(arena["AController"]);

    ::std::process::exit(0);

    let amycharacter = get_class_mut(&mut arena, "AActor");
    replace_with_padding(&mut amycharacter.members, Some("ControllingMatineeActors"),
                         Some("InstanceComponents"), 0, 0x150);
    let last = find_field(&amycharacter.members, "InstanceComponents");
    let len = amycharacter.members.len();
    delete_between(&mut amycharacter.members, last + 1, len);
    insert_after(&mut amycharacter.members, "InstanceComponents",
                 padding(1, 0xb0, 0));

    let apawn = get_class_mut(&mut arena, "APawn");
    insert_padding_before(&mut apawn.members, "bitfield0", 2, 8);

    let uactorcomponent = get_class_mut(&mut arena, "UActorComponent");
    let from = get_start(&mut uactorcomponent.members, Some("UCSModifiedProperties"));
    let to = get_end(&mut uactorcomponent.members, Some("WorldPrivate"));
    delete_between(&mut uactorcomponent.members, from, to);

    let mut writer = Writer::new(io::stdout(), &arena);
    for name in structs {
        writer.write_type(arena[&name]).unwrap();
    }
}

fn get_class<'a>(arena: &'a Arena, name: &str) -> &'a Class {
    if let TypeIndex::Class(index) = arena[name] {
        &arena[index]
    } else {
        unreachable!()
    }
}

fn get_class_mut<'a>(arena: &'a mut Arena, name: &str) -> &'a mut Class {
    if let TypeIndex::Class(index) = arena[name] {
        &mut arena[index]
    } else {
        unreachable!()
    }
}

fn find_field(members: &[ClassMember], to_find: &str) -> usize {
    members.iter().position(|m| match m {
        ClassMember::Field(ClassField { name, .. }) if name.name == to_find => true,
        ClassMember::Field(ClassField { name, .. }) => {
            false
        },
        _ => false
    }).unwrap()
}

fn get_start(members: &[ClassMember], name: Option<&str>) -> usize {
    match name {
        Some(name) => find_field(&members, name),
        None => 0
    }
}
fn get_end(members: &[ClassMember], name: Option<&str>) -> usize {
    match name {
        Some(name) => find_field(&members, name),
        None => members.len()
    }
}

fn delete_between(members: &mut Vec<ClassMember>, from: usize, to: usize) {
    members.drain(from..to);
}

fn replace_between(members: &mut Vec<ClassMember>, from: usize, to: usize, with: ClassMember) {
    delete_between(members, from, to);
    members.insert(from, with);
}

fn replace_with_padding(members: &mut Vec<ClassMember>, from: Option<&str>, to: Option<&str>,
                        pad_num: usize, size: usize) {
    let from = get_start(members, from);
    let to = get_end(members, to);
    let offset = match &members[from] {
        ClassMember::Field(ClassField { offset, .. }) => *offset,
        _ => unreachable!()
    };
    replace_between(members, from, to, padding(pad_num, size, offset));
}

fn insert_padding_after(members: &mut Vec<ClassMember>, after: &str, pad_num: usize, size: usize) {
    let from = find_field(members, after) + 1;
    let offset = match &members[from - 1] {
        ClassMember::Field(ClassField { offset, .. }) => *offset,
        _ => unreachable!()
    };
    insert_after(members, after, padding(pad_num, size, offset));
}

fn insert_padding_before(members: &mut Vec<ClassMember>, before: &str, pad_num: usize, size: usize) {
    let from = find_field(members, before);
    let offset = match &members[from] {
        ClassMember::Field(ClassField { offset, .. }) => *offset,
        _ => unreachable!()
    };
    insert_before(members, before, padding(pad_num, size, offset));
}

fn insert_before(members: &mut Vec<ClassMember>, before: &str, element: ClassMember) {
    let before = find_field(members, before);
    members.insert(before, element);
}

fn insert_after(members: &mut Vec<ClassMember>, after: &str, element: ClassMember) {
    let after = find_field(members, after);
    members.insert(after + 1, element);
}

fn padding(pad_num: usize, size: usize, offset: usize) -> ClassMember {
    ClassMember::Field(ClassField {
        attributes: Attributes::default(),
        name: format!("_pad{}", pad_num).into(),
        offset,
        kind: ClassFieldKind::Array(Box::new(Array {
            element_type: ClassFieldKind::Primitive(PrimitiveKind::U8),
            stride: None,
            dimensions: vec![size],
        }))
    })
}
