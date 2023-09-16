mod eh_frame;

use crate::eh_frame::dump_eh_frame;
use std::path::Path;
use std::env;
use std::fs;
use std::io::Cursor;
use byteorder::LittleEndian;
use object::{ObjectSection, Object};

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
            let eh_frame = object
                .section_by_name(".eh_frame")
                .unwrap()
                .uncompressed_data()
                .unwrap();
            // hexdump(&eh_frame, 16);
            //
            let pointer_size = if object.is_64() { 8 } else { 4 };

            dump_eh_frame::<LittleEndian, _>(&mut Cursor::new(eh_frame), pointer_size).unwrap()
        }
    }
}
