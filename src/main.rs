use std::env;
use std::fs;
use std::io;
use std::io::Cursor;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Seek;
use std::path::Path;

use byteorder::ByteOrder;
use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use leb128;

use num_enum::TryFromPrimitiveError;
use object::{Object, ObjectSection};

use thiserror::Error;
use num_enum::TryFromPrimitive;

#[allow(non_camel_case_types)]
#[derive(Debug, TryFromPrimitive)]
#[repr(u8)]
enum EhPointerFormat {
    // The Value is a literal pointer whose size is determined by the architecture.
    DW_EH_PE_absptr = 0x00,
    // Unsigned value is encoded using the Little Endian Base 128 (LEB128) as defined by DWARF Debugging Information Format, Revision 2.0.0.
    DW_EH_PE_uleb128 = 0x01,
    // A 2 bytes unsigned value.
    DW_EH_PE_udata2 = 0x02,
    // A 4 bytes unsigned value.
    DW_EH_PE_udata4 = 0x03,
    // An 8 bytes unsigned value.
    DW_EH_PE_udata8 = 0x04,
    // Signed value is encoded using the Little Endian Base 128 (LEB128) as defined by DWARF Debugging Information Format, Revision 2.0.0.
    DW_EH_PE_sleb128 = 0x09,
    // A 2 bytes signed value.
    DW_EH_PE_sdata2 = 0x0A,
    // A 4 bytes signed value.
    DW_EH_PE_sdata4 = 0x0B,
    // An 8 bytes signed value.
    DW_EH_PE_sdata8 = 0x0C,
}

#[allow(non_camel_case_types)]
#[derive(Debug, TryFromPrimitive)]
#[repr(u8)]
enum EhPointerApplication {
    // Value is relative to the current program counter.
    DW_EH_PE_pcrel = 0x10,
    // Value is relative to the beginning of the .text section.
    DW_EH_PE_textrel = 0x20,
    // Value is relative to the beginning of the .got or .eh_frame_hdr section.
    DW_EH_PE_datarel = 0x30,
    // Value is relative to the beginning of the function.
    DW_EH_PE_funcrel = 0x40,
    // Value is aligned to an address unit sized boundary.
    DW_EH_PE_aligned = 0x50,
}

#[derive(Debug, Error)]
enum EhFrameError {
    #[error("IO error: {0}")]
    IoError(io::Error),
    #[error("LEB decode error: {0}")]
    LebError(leb128::read::Error),
    #[error("pointer format decode error: {0}")]
    PointerFormatDecodeError(TryFromPrimitiveError<EhPointerFormat>),
    #[error("pointer application decode error: {0}")]
    PointerApplicationDecodeError(TryFromPrimitiveError<EhPointerApplication>),
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

impl From<TryFromPrimitiveError<EhPointerFormat>> for EhFrameError {
    fn from(value: TryFromPrimitiveError<EhPointerFormat>) -> Self {
        Self::PointerFormatDecodeError(value)
    }
}

impl From<TryFromPrimitiveError<EhPointerApplication>> for EhFrameError {
    fn from(value: TryFromPrimitiveError<EhPointerApplication>) -> Self {
        Self::PointerApplicationDecodeError(value)
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
            } else {
                print!(".");
            }
        }

        print!("\n");
    }
}

struct Cie {
    fde_pointer_format: Option<EhPointerFormat>,
    fde_pointer_application: Option<EhPointerApplication>,
}
struct Fde {}

enum EhFrameEntry {
    Cie(Cie),
    Fde(Fde),
}

impl Cie {
    fn parse<Endian: ByteOrder, R: Read + Seek>(
        data: &mut R,
        pointer_size: usize,
    ) -> Result<Cie, EhFrameError> {
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

        // return_address_register
        // An unsigned byte constant that indicates which column in the rule table represents the
        // return address of the function. Note that this column might not correspond to an actual
        // machine register.
        // NOTE: This field was a pain to figure out, as it doesn't seem to be properly documented.
        // It may be incorrect
        let _return_address_register = data.read_u8();
        // let _return_address_register = leb128::read::unsigned(data);

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

        let mut fde_pointer_format: Option<EhPointerFormat> = None;
        let mut fde_pointer_application: Option<EhPointerApplication> = None;
        if let Some(augmentation_data) = augmentation_data {
            // hexdump(&augmentation_data, 16);

            let mut augmentation_data = Cursor::new(&augmentation_data);

            let mut augmentation_string_iter = augmentation_string.chars();
            while let Some(augmentation) = augmentation_string_iter.next() {
                match augmentation {
                    // A 'z' may be present as the first character of the string. If present, the
                    // Augmentation Data field shall be present. The contents of the Augmentation
                    // Data shall be intepreted according to other characters in the Augmentation
                    // String.
                    'z' => {}

                    // If the Augmentation string has the value "eh", then the EH Data field shall
                    // be present.
                    'e' => {
                        let next_char = augmentation_string_iter.next();
                        assert_eq!(
                            Some('h'),
                            next_char,
                            "saw '{:?}' in augmentation, expected 'h' ('eh')",
                            next_char
                        );
                    }

                    // A 'R' may be present at any position after the first character of the
                    // string. This character may only be present if 'z' is the first character of
                    // the string. If present, The Augmentation Data shall include a 1 byte
                    // argument that represents the pointer encoding for the address pointers used
                    // in the FDE.
                    'R' => {
                        let b = augmentation_data.read_u8()?;
                        fde_pointer_format = Some(EhPointerFormat::try_from(b & 0x0F)?);
                        fde_pointer_application = Some(EhPointerApplication::try_from(b & 0xF0)?);
                    }

                    _ => todo!("unhandled augmentation: {}", augmentation),
                }
            }
        }

        // println!("version: {}", version);
        // println!("augmentation: {}", augmentation_string);
        // println!("augmentation_size: {:#?}", augmentation_data_length);
        // println!("eh: {:?}", _eh);
        // println!("pointer format: {:?}", fde_pointer_format);
        // println!("pointer application: {:?}", fde_pointer_application);

        Ok(Cie {
            fde_pointer_format,
            fde_pointer_application,
        })
    }
}

impl Fde {
    fn parse<Endian: ByteOrder, R: Read + Seek>(
        data: &mut R,
        cie_pointer: u32,
    ) -> Result<Fde, EhFrameError> {
        Ok(Fde {})
    }
}

fn parse_eh_frame_entry<Endian: ByteOrder, R: Read + Seek>(
    data: &mut R,
    pointer_size: usize,
) -> Result<Option<EhFrameEntry>, EhFrameError> {
    // Length
    // A 4 byte unsigned value indicating the length in bytes of the CIE structure, not
    // including the Length field itself.
    let mut length: u64 = match data.read_u32::<Endian>() {
        Ok(l) => Ok(l.into()),
        // Some compilers don't put a terminator CIE in the section, and so we get an EOF. In
        // this case we return 0, which will be handled below
        Err(e) => {
            if e.kind() == ErrorKind::UnexpectedEof {
                Ok(0)
            } else {
                Err(e)
            }
        }
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
        _ => EhFrameEntry::Fde(Fde::parse::<Endian, _>(data, cie_id)?),
    };

    let n_bytes_read = data.stream_position()? - start_pos;
    assert!(
        n_bytes_read <= length,
        "number of bytes read overflowed cie length: {} > {}",
        n_bytes_read,
        length
    );

    // Skip over unread padding
    data.seek(io::SeekFrom::Current((length - n_bytes_read) as i64))?;

    return Ok(Some(entry));
}

fn dump_eh_frame<Endian: ByteOrder, R: Read + Seek>(
    data: &mut R,
    pointer_size: usize,
) -> Result<(), EhFrameError> {
    // let cies = HashMap::new();

    while let Some(entry) = parse_eh_frame_entry::<Endian, _>(data, pointer_size)? {
        let end_pos = data.stream_position().unwrap();
        match entry {
            EhFrameEntry::Cie(_) => println!("cie: {:08x}", end_pos),
            EhFrameEntry::Fde(_) => println!("fde: {:08x}", end_pos),
        }
    }

    Ok(())
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
