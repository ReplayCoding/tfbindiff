mod eh_frame;

use crate::eh_frame::get_fdes;
use byteorder::LittleEndian;
use object::{Object, ObjectSection};
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::Path;

fn hexdump(data: &[u8], chunk_size: usize) {
    for chunk in data.chunks(chunk_size) {
        for b in chunk {
            print!("{:02x} ", b);
        }

        // padding to align the ascii display
        for _ in 0..chunk_size - chunk.len() {
            print!("   ");
        }

        for b in chunk {
            let c = *b as char;
            if c.is_alphanumeric() {
                print!("{}", c);
            } else {
                print!(".");
            }
        }

        print!("\n");
    }
}

fn main() {
    for (i, arg) in env::args().enumerate() {
        if i == 1 {
            let path = Path::new(arg.as_str());
            let buffer = fs::read(path).unwrap();
            let object = object::File::parse(&*buffer).unwrap();

            for section in object.sections() {
                println!(
                    "section: {} {:08x}",
                    section.name().unwrap(),
                    section.address()
                );
            }
            let eh_frame = object.section_by_name(".eh_frame").unwrap();
            let eh_frame_data = eh_frame.uncompressed_data().unwrap();

            let pointer_size = if object.is_64() { 8 } else { 4 };

            let fdes = get_fdes::<LittleEndian, _>(
                &mut Cursor::new(eh_frame_data),
                pointer_size,
                eh_frame.address(),
            )
            .unwrap();
            let symbol_map = object.symbol_map();
            for fde in fdes {
                if let Some(symbol) = symbol_map.get(fde.begin) {
                    println!("function {} has length {}", symbol.name(), fde.length);
                }
            }
        }
    }
}
