use std::io;
use clap::Parser;
use pdbextract::Alignment;
use pdbextract::ir::*;

#[derive(Parser)]
struct Args {
    file: String,
    #[clap(long)]
    structs: Vec<String>,
    #[clap(long)]
    ignore: Vec<String>,
    #[clap(long)]
    replace: Vec<String>,
    #[clap(long)]
    recursive: bool,
}

fn main() {
    env_logger::init();
    let args = Args::parse();
    let mut arena = pdbextract::parse(&args.file).unwrap();
    eprintln!("parsed");
    // let character = get_class(&arena, "TTypeCompatibleBytes<unsigned int>");
    // panic!("{}, {}", character.size, character.size(&arena));
    // let character = get_class(&arena, "AMyCharacter");
    // character.check_offsets(&arena);
    //writer.write_type(arena["AMyCharacter"]);
    //writer.write_type(arena["USceneComponent"]);
    //writer.write_type(arena["UCharacterMovementComponent"]);
    //writer.write_type(arena["AController"]);

    weird_ue_fixes(&mut arena);


    let mut writer = Writer::new(io::stdout(), &arena).unwrap();
    // for class_index in arena.class_indices() {
    //     if arena[class_index].name.name == "FTransform" {
    //         writer.write_exact_type(TypeIndex::Class(class_index));
    //     }
    // }
    for name in &["TTraceThreadData<FTraceDatum>"] {
    // for name in &args.structs {
        writer.write_type(arena[&name]).unwrap();
    }

    // if args.recursive {
    //     writer.write_rest().unwrap();
    // }
    eprintln!("{:#?}", get_class(&arena, "TTraceThreadData<FTraceDatum>"));
    eprintln!("{:#?}", arena.get_class(ClassIndex(1866)));
}

#[allow(unused)]
fn weird_ue_fixes(arena: &mut Arena) {
    get_class_mut(arena, "FQuat").alignment = Alignment::Both(16);
    get_class_mut(arena, "FVector4").alignment = Alignment::Both(16);
    get_union_mut(arena, "__m128").alignment = Alignment::Both(16);
    for class in arena.classes_mut() {
        if let Some(rest) = class.name.name.strip_prefix("TAlignedBytes<") {
            if let Some(rest) = rest.strip_suffix(">::TPadding") {
                let mut nums = rest.split(",");
                let _size: usize = nums.next().unwrap().parse().unwrap();
                let align: usize = nums.next().unwrap().parse().unwrap();
                class.alignment = Alignment::Both(align);
            }
        }
    }
    // let amycharacter = get_class_mut(arena, "AActor");
    // replace_with_padding(&mut amycharacter.members, Some("ControllingMatineeActors"),
    //                      Some("InstanceComponents"), 0, 0x150);
    // let last = find_field(&amycharacter.members, "InstanceComponents");
    // let len = amycharacter.members.len();
    // delete_between(&mut amycharacter.members, last + 1, len);
    // insert_after(&mut amycharacter.members, "InstanceComponents",
    //              padding(1, 0xb0, 0));
    //
    // let apawn = get_class_mut(arena, "APawn");
    // insert_padding_before(&mut apawn.members, "bitfield0", 2, 8);

    // let uactorcomponent = get_class_mut(arena, "UActorComponent");
    // let from = get_start(&mut uactorcomponent.members, Some("UCSModifiedProperties"));
    // let to = get_end(&mut uactorcomponent.members, Some("WorldPrivate"));
    // delete_between(&mut uactorcomponent.members, from, to);
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
fn get_union_mut<'a>(arena: &'a mut Arena, name: &str) -> &'a mut Union {
    if let TypeIndex::Union(index) = arena[name] {
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
        })),
        max_size: size,
    })
}
