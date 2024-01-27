use byteorder::LittleEndian;
use object::{Object, ObjectSection};
use std::fs;
use std::io::Cursor;
use std::path::Path;
use tfbindiff::eh_frame::get_fdes;

fn load_file(filename: &str) -> memmap2::Mmap {
    let file = fs::File::open(Path::new(filename)).unwrap();
    unsafe { memmap2::Mmap::map(&file).unwrap() }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();

    if args.len() != 2 {
        println!("Usage: {} <program>", args[0]);
        return;
    }

    let data = load_file(&args[1]);
    let object = object::File::parse(data.as_ref()).unwrap();

    let pointer_size = if object.is_64() { 8 } else { 4 };

    let eh_frame = object.section_by_name(".eh_frame").unwrap();
    let eh_frame_data = eh_frame.uncompressed_data().unwrap();

    // FIXME: not that it actually matters, but this shouldn't be hardcoded
    let fdes = get_fdes::<LittleEndian, _>(
        &mut Cursor::new(eh_frame_data),
        pointer_size,
        eh_frame.address(),
    )
    .unwrap();

    for fde in fdes {
        println!("{:08X} len {:04x}", fde.begin, fde.length);
    }
}
