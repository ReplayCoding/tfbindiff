use std::io;
use std::io::Cursor;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Seek;
use std::path::Path;
use std::env;
use std::fs;

use byteorder::ByteOrder;
use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use object::{Object, ObjectSection};

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
            }
            else {
                print!(".");
            }
        }

        print!("\n");
    }
}

fn dump_eh_frame<Endian: ByteOrder, R: Read + Seek>(data: &mut R) -> io::Result<()> {
    loop {
        // Length
        // A 4 byte unsigned value indicating the length in bytes of the CIE structure, not
        // including the Length field itself.
        let mut length: u64 = match data.read_u32::<Endian>() {
            Ok(l) => Ok(l.into()),
            // Some compilers don't put a terminator CIE in the section, and so we get an EOF. In
            // this case we return 0, which will be handled below
            Err(e) => if e.kind() == ErrorKind::UnexpectedEof { Ok(0) } else { Err(e) },
        }?;

        // If Length contains the value 0xffffffff, then the length is contained in the Extended
        // Length field.
        if length == 0xffffffff {
            // Extended Length
            // A 8 byte unsigned value indicating the length in bytes of the CIE structure, not
            // including the Length and Extended Length fields.
            length = data.read_u64::<Endian>()?;
        }

        // If Length contains the value 0, then this CIE shall be considered a terminator and
        // processing shall end.
        if length == 0 {
            break;
        }

        // Used for keeping track of how many bytes we've read, so we can make sure it matches the
        // CIE length
        let start_pos = data.stream_position().unwrap();

        let _ = data.seek(io::SeekFrom::Current(length as i64));

        let n_bytes_read = data.stream_position().unwrap() - start_pos;
        assert_eq!(n_bytes_read, length, "number of bytes read did not match cie length: {} != {}", n_bytes_read, length);

        println!("cie: {:x} {:08x}", length, data.stream_position().unwrap());
    }

    Ok(())
}

fn main () {
    for (i, arg) in env::args().enumerate() {
        if i == 1 {
            let path = Path::new(arg.as_str());
            let buffer = fs::read(path).unwrap();
            let object = object::File::parse(&*buffer).unwrap();

            for section in object.sections() {
                println!("section: {} {:08x}", section.name().unwrap(), section.address());
            }
            let eh_frame = object.section_by_name(".eh_frame").unwrap().uncompressed_data().unwrap();
            // hexdump(&eh_frame, 16);

            dump_eh_frame::<LittleEndian, _>(&mut Cursor::new(eh_frame)).unwrap()
        }
    }
}
