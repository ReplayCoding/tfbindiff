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
use leb128;
use thiserror::Error;

#[derive(Debug, Error)]
enum EhFrameError {
    #[error("IO error: {0}")]
    IoError(io::Error),
    #[error("LEB decode error: {0}")]
    LebError(leb128::read::Error),
}

impl From<io::Error> for EhFrameError {
    fn from(value: io::Error) -> Self {
        Self::IoError(value)
    }
}

impl From<leb128::read::Error> for EhFrameError {
    fn from(value: leb128::read::Error) -> Self {
        Self::LebError(value)
    }
}

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

struct Cie {
    augmentation_string: String
}
struct Fde {}

enum EhFrameEntry {
    Cie(Cie),
    Fde(Fde)
}

impl Cie {
    fn parse<Endian: ByteOrder, R: Read + Seek>(data: &mut R, pointer_size: usize) -> Result<Cie, EhFrameError> {
        // Version
        // Version assigned to the call frame information structure. This value shall be 1.
        let version = data.read_u8()?;
        assert_eq!(version, 1, "version mismatch: {} != 1", version);

        let mut augmentation_string = String::new();

        // Augmentation String
        // This value is a NUL terminated string that identifies the augmentation to the CIE or to the
        // FDEs associated with this CIE. A zero length string indicates that no augmentation data is
        // present. The augmentation string is case sensitive.
        let mut augmentation = data.read_u8()?;
        while augmentation != 0 {
            augmentation_string.push(augmentation.into());

            augmentation = data.read_u8()?;
        }

        // EH Data
        // On 32 bit architectures, this is a 4 byte value that... On 64 bit architectures, this is a
        // 8 byte value that... This field is only present if the Augmentation String contains the
        // string "eh".
        let mut _eh: Option<u64> = None;
        if augmentation_string.contains("eh") {
            _eh = Some(match pointer_size {
                4 => data.read_u32::<Endian>()?.into(),
                8 => data.read_u64::<Endian>()?,
                _ => todo!("Unhandled pointer size: {}", pointer_size),
            });
        }

        // Code Alignment Factor
        // An unsigned LEB128 encoded value that is factored out of all advance location instructions that
        // are associated with this CIE or its FDEs. This value shall be multiplied by the delta argument
        // of an adavance location instruction to obtain the new location value.
        let _code_alignment_factor = leb128::read::unsigned(data);

        // Data Alignment Factor
        // A signed LEB128 encoded value that is factored out of all offset instructions that are
        // associated with this CIE or its FDEs. This value shall be multiplied by the register offset
        // argument of an offset instruction to obtain the new offset value.
        let _data_alignment_factor = leb128::read::signed(data);

        // Augmentation Length
        // An unsigned LEB128 encoded value indicating the length in bytes of the Augmentation Data. This
        // field is only present if the Augmentation String contains the character 'z'.
        let mut augmentation_data_length = None;

        if augmentation_string.contains("z") {
            augmentation_data_length = Some(leb128::read::unsigned(data)?);
        }

        // Augmentation Data
        // A block of data whose contents are defined by the contents of the Augmentation String as
        // described below. This field is only present if the Augmentation String contains the character
        // 'z'.
        let mut augmentation_data: Option<Vec<u8>> = None;
        if let Some(augmentation_data_length) = augmentation_data_length {
            let mut buf = vec![0u8; augmentation_data_length as usize];
            
            data.read_exact(&mut buf)?;

            augmentation_data = Some(buf)
        }

        // Initial Instructions
        // Initial set of Call Frame Instructions.
        // Unimplemented...

        println!("version: {}", version);
        println!("augmentation: {}", augmentation_string);
        println!("eh: {:?}", _eh);
        if let Some(d) = augmentation_data {
            hexdump(&d, 16);
        }

        Ok(Cie {
            augmentation_string
        })
    }
}

impl Fde {
    fn parse<Endian: ByteOrder, R: Read + Seek>(data: &mut R, cie_pointer: u32) -> Result<Fde, EhFrameError> {
        Ok(Fde {  })
    }
}

fn parse_eh_frame_entry<Endian: ByteOrder, R: Read + Seek>(data: &mut R, pointer_size: usize) -> Result<Option<EhFrameEntry>, EhFrameError> {
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
            return Ok(None);
        }

        // Used for keeping track of how many bytes we've read, so we can make sure it matches the
        // CIE length
        let start_pos = data.stream_position()?;

        // CIE ID
        // A 4 byte unsigned value that is used to distinguish CIE records from FDE records. This value
        // shall always be 0, which indicates this record is a CIE.
        let cie_id = data.read_u32::<Endian>()?;

        let entry = match cie_id {
            0 => EhFrameEntry::Cie(Cie::parse::<Endian, _>(data, pointer_size)?),
            _ => EhFrameEntry::Fde(Fde::parse::<Endian, _>(data, cie_id)?)
        };

        let n_bytes_read = data.stream_position()? - start_pos;
        assert!(n_bytes_read <= length, "number of bytes read overflowed cie length: {} > {}", n_bytes_read, length);

        // Skip over unread padding
        data.seek(io::SeekFrom::Current((length - n_bytes_read) as i64))?;

        return Ok(Some(entry));
}

fn dump_eh_frame<Endian: ByteOrder, R: Read + Seek>(data: &mut R, pointer_size: usize) -> Result<(), EhFrameError> {
    // let cies = HashMap::new();

    while let Some(entry) = parse_eh_frame_entry::<Endian, _>(data, pointer_size)? {
        println!("cie/fde: {:08x}", data.stream_position().unwrap());
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
            //
            let pointer_size = if object.is_64() { 8 } else { 4 };

            dump_eh_frame::<LittleEndian, _>(&mut Cursor::new(eh_frame), pointer_size).unwrap()
        }
    }
}
