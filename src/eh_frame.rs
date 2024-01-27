use byteorder::ByteOrder;
use byteorder::ReadBytesExt;
use num_enum::TryFromPrimitive;
use num_enum::TryFromPrimitiveError;
use std::collections::HashMap;
use std::io;
use std::io::Cursor;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Seek;
use thiserror::Error;

#[allow(non_camel_case_types)]
#[derive(Debug, TryFromPrimitive, Clone, Copy)]
#[repr(u8)]
pub enum EhPointerFormat {
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
#[derive(Debug, TryFromPrimitive, Clone, Copy)]
#[repr(u8)]
pub enum EhPointerApplication {
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
pub enum EhFrameError {
    #[error("IO error: {0}")]
    Io(io::Error),
    #[error("LEB decode error: {0}")]
    Leb(leb128::read::Error),
    #[error("pointer format decode error: {0}")]
    PointerFormatDecode(TryFromPrimitiveError<EhPointerFormat>),
    #[error("pointer application decode error: {0}")]
    PointerApplicationDecode(TryFromPrimitiveError<EhPointerApplication>),
    #[error("invalid CIE {0} for parsing a FDE: {1}")]
    InvalidCie(u64, &'static str),
}

impl From<io::Error> for EhFrameError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<leb128::read::Error> for EhFrameError {
    fn from(value: leb128::read::Error) -> Self {
        Self::Leb(value)
    }
}

impl From<TryFromPrimitiveError<EhPointerFormat>> for EhFrameError {
    fn from(value: TryFromPrimitiveError<EhPointerFormat>) -> Self {
        Self::PointerFormatDecode(value)
    }
}

impl From<TryFromPrimitiveError<EhPointerApplication>> for EhFrameError {
    fn from(value: TryFromPrimitiveError<EhPointerApplication>) -> Self {
        Self::PointerApplicationDecode(value)
    }
}

#[derive(Debug)]
pub struct Cie {
    pub fde_pointer_format: Option<EhPointerFormat>,
    pub fde_pointer_application: Option<EhPointerApplication>,
}

#[derive(Debug)]
pub struct Fde {
    pub begin: u64,
    pub length: u64,
}

pub enum EhFrameEntry {
    Cie(u64, Cie),
    Fde(Fde),
}

fn read_encoded_no_application<Endian: ByteOrder, R: Read + Seek>(
    data: &mut R,
    format: EhPointerFormat,
    pointer_size: usize,
) -> Result<u64, EhFrameError> {
    Ok(match format {
        EhPointerFormat::DW_EH_PE_absptr => match pointer_size {
            4 => data.read_u32::<Endian>()?.into(),
            _ => todo!("unhandled pointer size: {}", pointer_size),
        },
        EhPointerFormat::DW_EH_PE_sdata4 => data.read_i32::<Endian>()? as u64,

        _ => todo!("unhandled format {:?}", format),
    })
}

fn read_encoded<Endian: ByteOrder, R: Read + Seek>(
    data: &mut R,
    format: EhPointerFormat,
    application: EhPointerApplication,
    pointer_size: usize,
    base_address: u64,
) -> Result<u64, EhFrameError> {
    let pcrel_offs = data.stream_position()?;
    let unapplied_value = read_encoded_no_application::<Endian, _>(data, format, pointer_size)?;
    let applied_value: u64 = match application {
        EhPointerApplication::DW_EH_PE_pcrel => base_address
            .wrapping_add(pcrel_offs)
            .wrapping_add(unapplied_value)
            .into(),
        _ => todo!("unhandled application {:?}", application),
    };

    Ok(applied_value)
}

impl Cie {
    fn parse<Endian: ByteOrder, R: Read + Seek>(
        data: &mut R,
        pointer_size: usize,
    ) -> Result<Self, EhFrameError> {
        // Version
        // Version assigned to the call frame information structure. This value shall be 1.
        let version = data.read_u8()?;
        assert_eq!(version, 1, "version mismatch: {version} != 1");

        // Augmentation String
        // This value is a NUL terminated string that identifies the augmentation to the CIE or to the
        // FDEs associated with this CIE. A zero length string indicates that no augmentation data is
        // present. The augmentation string is case sensitive.
        let mut augmentation_string = String::new();
        loop {
            let augmentation = data.read_u8()?;
            if augmentation == 0 {
                break;
            }

            augmentation_string.push(augmentation.into());
        }

        // EH Data
        // On 32 bit architectures, this is a 4 byte value that... On 64 bit architectures, this is a
        // 8 byte value that... This field is only present if the Augmentation String contains the
        // string "eh".
        let mut _eh: Option<u64> = None;
        if augmentation_string.contains("eh") {
            _eh = Some(match pointer_size {
                4 => data.read_u32::<Endian>()?.into(),
                _ => todo!("Unhandled pointer size: {}", pointer_size),
            });
        }

        // Code Alignment Factor
        // An unsigned LEB128 encoded value that is factored out of all advance location instructions that
        // are associated with this CIE or its FDEs. This value shall be multiplied by the delta argument
        // of an advance location instruction to obtain the new location value.
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
        let _return_address_register = data.read_u8()?;
        // let _return_address_register = leb128::read::unsigned(data);
        // Check LEB continuation bit, if this isn't zero, see if uncommenting the above line fixes
        // parsing issues
        assert_eq!(_return_address_register & (1 << 7), 0);

        // Augmentation Length
        // An unsigned LEB128 encoded value indicating the length in bytes of the Augmentation Data. This
        // field is only present if the Augmentation String contains the character 'z'.
        let mut augmentation_data_length = None;

        if augmentation_string.contains('z') {
            augmentation_data_length = Some(leb128::read::unsigned(data)?);
        }

        // Augmentation Data
        // A block of data whose contents are defined by the contents of the Augmentation String as
        // described below. This field is only present if the Augmentation String contains the character
        // 'z'.
        let mut augmentation_data: Option<Vec<u8>> = None;
        if let Some(augmentation_data_length) = augmentation_data_length {
            let mut buf = vec![0u8; augmentation_data_length.try_into().unwrap()];

            data.read_exact(&mut buf)?;

            augmentation_data = Some(buf)
        }

        let mut fde_pointer_format: Option<EhPointerFormat> = None;
        let mut fde_pointer_application: Option<EhPointerApplication> = None;
        if let Some(augmentation_data) = augmentation_data {
            let mut augmentation_data = Cursor::new(&augmentation_data);

            let mut augmentation_string_iter = augmentation_string.chars();
            while let Some(augmentation) = augmentation_string_iter.next() {
                match augmentation {
                    // A 'z' may be present as the first character of the string. If present, the
                    // Augmentation Data field shall be present. The contents of the Augmentation
                    // Data shall be interpreted according to other characters in the Augmentation
                    // String.
                    'z' => {}

                    // If the Augmentation string has the value "eh", then the EH Data field shall
                    // be present.
                    'e' => {
                        let next_char = augmentation_string_iter.next();
                        assert_eq!(
                            Some('h'),
                            next_char,
                            "saw '{next_char:?}' in augmentation, expected 'h' ('eh')"
                        );
                    }

                    // A 'L' may be present at any position after the first character of the
                    // string. This character may only be present if 'z' is the first character of
                    // the string. If present, it indicates the presence of one argument in the
                    // Augmentation Data of the CIE, and a corresponding argument in the
                    // Augmentation Data of the FDE. The argument in the Augmentation Data of the
                    // CIE is 1-byte and represents the pointer encoding used for the argument in
                    // the Augmentation Data of the FDE, which is the address of a
                    // language-specific data area (LSDA). The size of the LSDA pointer is
                    // specified by the pointer encoding used.
                    'L' => {
                        let _pointer_format = augmentation_data.read_u8()?;
                    }

                    // A 'P' may be present at any position after the first character of the string. This character may
                    // only be present if 'z' is the first character of the string. If present, it indicates the
                    // presence of two arguments in the Augmentation Data of the CIE. The first argument is 1-byte and
                    // represents the pointer encoding used for the second argument, which is the address of a
                    // personality routine handler. The personality routine is used to handle language and
                    // vendor-specific tasks. The system unwind library interface accesses the language-specific
                    // exception handling semantics via the pointer to the personality routine. The personality
                    // routine does not have an ABI-specific name. The size of the personality routine pointer is
                    // specified by the pointer encoding used.
                    'P' => {
                        let b = augmentation_data.read_u8()?;
                        let pointer_format = EhPointerFormat::try_from(b & 0x0F)?;

                        let _personality_routine = read_encoded_no_application::<Endian, _>(
                            &mut augmentation_data,
                            pointer_format,
                            pointer_size,
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

        Ok(Self {
            fde_pointer_format,
            fde_pointer_application,
        })
    }
}

impl Fde {
    fn parse<Endian: ByteOrder, R: Read + Seek>(
        data: &mut R,
        cie_pointer: u32,
        cies: &HashMap<u64, Cie>,
        pointer_size: usize,
        base_address: u64,
    ) -> Result<Self, EhFrameError> {
        let offs = data.stream_position()?;

        // - 4 because the stream is currently *after* the CIE id, we want directly before
        let absolute_cie_pointer = offs - u64::from(cie_pointer) - 4;
        let cie = cies
            .get(&absolute_cie_pointer)
            .ok_or(EhFrameError::InvalidCie(
                absolute_cie_pointer,
                "no such CIE",
            ))?;

        // PC Begin
        // An encoded value that indicates the address of the initial location associated with this
        // FDE. The encoding format is specified in the Augmentation Data.
        let pc_begin = read_encoded::<Endian, _>(
            data,
            cie.fde_pointer_format.ok_or(EhFrameError::InvalidCie(
                absolute_cie_pointer,
                "no pointer format in the CIE",
            ))?,
            cie.fde_pointer_application.ok_or(EhFrameError::InvalidCie(
                absolute_cie_pointer,
                "no pointer application in the CIE",
            ))?,
            pointer_size,
            base_address,
        )?;

        // PC Range
        // An absolute value that indicates the number of bytes of instructions associated with
        // this FDE.
        let pc_range: u64 = match pointer_size {
            4 => data.read_u32::<Endian>()?.into(),
            _ => todo!("unhandled pointer size: {}", pointer_size),
        };

        Ok(Self {
            begin: pc_begin,
            length: pc_range,
        })
    }
}

fn parse_eh_frame_entry<Endian: ByteOrder, R: Read + Seek>(
    data: &mut R,
    pointer_size: usize,
    cies: &HashMap<u64, Cie>,
    base_address: u64,
) -> Result<Option<EhFrameEntry>, EhFrameError> {
    let entry_offset = data.stream_position()?;

    // Length
    // A 4 byte unsigned value indicating the length in bytes of the CIE structure, not
    // including the Length field itself.
    let mut length: u64 = match data.read_u32::<Endian>() {
        Ok(l) => Ok(l.into()),
        // Some compilers don't put a terminator CIE in the section, and so we get an EOF.
        Err(e) => {
            if e.kind() == ErrorKind::UnexpectedEof {
                return Ok(None);
            } else {
                Err(e)
            }
        }
    }?;

    // If Length contains the value 0xffffffff, then the length is contained in the Extended
    // Length field.
    if length == 0xffff_ffff {
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
    // A 4 byte unsigned value that is used to distinguish CIE records from FDE records.
    let cie_id = data.read_u32::<Endian>()?;

    let entry = match cie_id {
        // For CIEs, This value shall always be 0, which indicates this record is a CIE.
        0 => EhFrameEntry::Cie(entry_offset, Cie::parse::<Endian, _>(data, pointer_size)?),
        // For FDEs, A 4 byte unsigned value that when subtracted from the offset of the CIE
        // Pointer in the current FDE yields the offset of the start of the associated CIE. This value
        // shall never be 0.
        _ => EhFrameEntry::Fde(Fde::parse::<Endian, _>(
            data,
            cie_id,
            cies,
            pointer_size,
            base_address,
        )?),
    };

    let n_bytes_read = data.stream_position()? - start_pos;
    assert!(
        n_bytes_read <= length,
        "number of bytes read overflowed CIE length: {n_bytes_read} > {length}"
    );

    // Skip over unread padding
    data.seek(io::SeekFrom::Current(
        (length - n_bytes_read).try_into().unwrap(),
    ))?;

    Ok(Some(entry))
}

pub fn get_fdes<Endian: ByteOrder, R: Read + Seek>(
    data: &mut R,
    pointer_size: usize,
    base_address: u64,
) -> Result<Vec<Fde>, EhFrameError> {
    let mut fdes: Vec<Fde> = vec![];
    let mut cies: HashMap<u64, Cie> = HashMap::new();

    while let Some(entry) =
        parse_eh_frame_entry::<Endian, _>(data, pointer_size, &cies, base_address)?
    {
        match entry {
            EhFrameEntry::Cie(offset, cie) => {
                cies.insert(offset, cie);
            }
            EhFrameEntry::Fde(fde) => fdes.push(fde),
        }
    }

    Ok(fdes)
}
