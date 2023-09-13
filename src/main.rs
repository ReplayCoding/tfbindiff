use std::borrow::Cow;
use std::io::Read;
use std::path::Path;
use std::env;
use std::fs;

use gimli::BaseAddresses;
use gimli::UnwindSection;
use gimli::{self, Section};
use object::{Object, ObjectSection};

fn hexdump(data: &[u8], chunk_size: usize) {
    for chunk in data.chunks(chunk_size) {
        for b in chunk {
            print!("{:02x} ", b);
        }

        for _ in 0..chunk_size - chunk.len() {
            print!("   ");
        }

        for b in chunk {
            let c = *b as char;
            if c.is_alphanumeric() {
                print!("{}", c);
            }
            else {
                print!(".");
            }
        }
        print!("\n");
    }
}

fn dump_eh_frame(object: &object::File, endian: gimli::RunTimeEndian) {
    // Load a section and return as `Cow<[u8]>`.
    let load_section = |id: gimli::SectionId| -> Result<Cow<[u8]>, gimli::Error> {
        match object.section_by_name(id.name()) {
            Some(ref section) => Ok(section
                .uncompressed_data()
                .unwrap_or(Cow::Borrowed(&[][..]))),
            None => Ok(Cow::Borrowed(&[][..])),
        }
    };

    let eh_frame_data = load_section(gimli::SectionId::EhFrame).unwrap();
    hexdump(&eh_frame_data, 16);
    let eh_frame = gimli::EhFrame::new(&eh_frame_data, endian);

    let mut bases = gimli::BaseAddresses::default();
    if let Some(section) = object.section_by_name(".eh_frame_hdr") {
        bases = bases.set_eh_frame_hdr(section.address());
    }
    if let Some(section) = object.section_by_name(".eh_frame") {
        bases = bases.set_eh_frame(section.address());
    }
    if let Some(section) = object.section_by_name(".text") {
        bases = bases.set_text(section.address());
    }

    let mut entries = eh_frame.entries(&bases);
    loop {
        match entries.next().unwrap() {
            None => break,
            Some(entry) => println!("{:#?}", entry),
        }
    }
}

fn main () {
    for (i, arg) in env::args().enumerate() {
        if i == 1 {
            let path = Path::new(arg.as_str());
            let buffer = fs::read(path).unwrap();
            let object = object::File::parse(&*buffer).unwrap();
            let endian = if object.is_little_endian() {
                gimli::RunTimeEndian::Little
            } else {
                gimli::RunTimeEndian::Big
            };

            for section in object.sections() {
                println!("section: {}", section.name().unwrap());
            }
            dump_eh_frame(&object, endian);
        }
    }
}
