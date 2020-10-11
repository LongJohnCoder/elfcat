use super::defs::*;
use super::elf32;
use super::elf64;

pub type InfoTuple = (&'static str, &'static str, String);

#[repr(u8)]
#[derive(Clone, PartialEq)]
pub enum RangeType {
    End,
    Ident,
    FileHeader,
    ProgramHeader,
    HeaderDetail(&'static str),
}

// Interval tree that allows querying point for all intervals that intersect it should be better.
// We can't beat O(n * m) but the average case should improve.
pub struct Ranges {
    pub data: Vec<Vec<RangeType>>,
}

pub struct ParsedIdent {
    pub magic: [u8; 4],
    pub class: u8,
    pub endianness: u8,
    pub version: u8,
    pub abi: u8,
    pub abi_ver: u8,
}

pub struct ParsedElf {
    pub filename: String,
    pub information: Vec<(&'static str, &'static str, String)>,
    pub contents: Vec<u8>,
    pub ranges: Ranges,
}

impl RangeType {
    fn id(&self) -> &str {
        match self {
            RangeType::Ident => "ident",
            RangeType::FileHeader => "ehdr",
            RangeType::ProgramHeader => "phdr",
            RangeType::HeaderDetail(class) => class,
            _ => "",
        }
    }

    fn always_highlight(&self) -> bool {
        match self {
            RangeType::ProgramHeader => true,
            RangeType::HeaderDetail(class) => match *class {
                "magic" => true,
                "ver" => true,
                "abi_ver" => true,
                "pad" => true,
                "e_version" => true,
                "e_flags" => true,
                "e_ehsize" => true,
                "e_shstrndx" => true,
                _ => false,
            },
            _ => false,
        }
    }

    fn needs_class(&self) -> bool {
        match self {
            RangeType::ProgramHeader => true,
            _ => false,
        }
    }

    fn class(&self) -> &str {
        match self {
            RangeType::ProgramHeader => "phdr",
            _ => "",
        }
    }

    pub fn span_attributes(&self) -> String {
        if self.needs_class() {
            // put id anyway for description
            format!(
                "id='{}' class='{}{}'",
                self.id(),
                self.class(),
                if self.always_highlight() {
                    " hover"
                } else {
                    ""
                }
            )
        } else {
            format!("id='{}'", self.id())
                + if self.always_highlight() {
                    " class='hover'"
                } else {
                    ""
                }
        }
    }
}

impl Ranges {
    fn new(capacity: usize) -> Ranges {
        Ranges {
            data: vec![vec![]; capacity],
        }
    }

    pub fn add_range(&mut self, start: usize, end: usize, range_type: RangeType) {
        self.data[start].push(range_type);
        self.data[start + end - 1].push(RangeType::End);
    }

    pub fn lookup_range_ends(&self, point: usize) -> usize {
        self.data[point]
            .iter()
            .filter(|&x| *x == RangeType::End)
            .count()
    }
}

impl ParsedIdent {
    fn from_bytes(buf: &Vec<u8>) -> ParsedIdent {
        ParsedIdent {
            magic: [buf[0], buf[1], buf[2], buf[3]],
            class: buf[ELF_EI_CLASS as usize],
            endianness: buf[ELF_EI_DATA as usize],
            version: buf[ELF_EI_VERSION as usize],
            abi: buf[ELF_EI_OSABI as usize],
            abi_ver: buf[ELF_EI_ABIVERSION as usize],
        }
    }
}

impl ParsedElf {
    pub fn from_bytes(filename: &String, buf: Vec<u8>) -> Result<ParsedElf, String> {
        if buf.len() < ELF_EI_NIDENT as usize {
            return Err(String::from("file is smaller than ELF header's e_ident"));
        }

        let ident = ParsedIdent::from_bytes(&buf);

        if ident.magic != [0x7f, 'E' as u8, 'L' as u8, 'F' as u8] {
            return Err(String::from("mismatched magic: not an ELF file"));
        }

        let mut ranges = Ranges::new(buf.len());

        let mut information = vec![];

        if ident.class == ELF_CLASS32 {
            elf32::parse(&buf, &ident, &mut information, &mut ranges)?;
        } else {
            elf64::parse(&buf, &ident, &mut information, &mut ranges)?;
        }

        ParsedElf::parse_ident(&ident, &mut information, &mut ranges)?;

        Ok(ParsedElf {
            filename: filename.clone(),
            information,
            contents: buf,
            ranges,
        })
    }

    fn parse_ident(
        ident: &ParsedIdent,
        information: &mut Vec<InfoTuple>,
        ranges: &mut Ranges,
    ) -> Result<(), String> {
        ParsedElf::push_ident_info(ident, information)?;

        ParsedElf::add_ident_ranges(ranges);

        Ok(())
    }

    fn push_ident_info(
        ident: &ParsedIdent,
        information: &mut Vec<InfoTuple>,
    ) -> Result<(), String> {
        information.push((
            "class",
            "Object class",
            match ident.class {
                ELF_CLASS32 => String::from("32-bit"),
                ELF_CLASS64 => String::from("64-bit"),
                x => return Err(format!("Unknown bitness: {}", x)),
            },
        ));

        information.push((
            "data",
            "Data encoding",
            match ident.endianness {
                ELF_DATA2LSB => String::from("Little endian"),
                ELF_DATA2MSB => String::from("Big endian"),
                x => return Err(format!("Unknown endianness: {}", x)),
            },
        ));

        if ident.version != ELF_EV_CURRENT {
            information.push(("ver", "Uncommon version(!)", format!("{}", ident.version)));
        }

        information.push((
            "abi",
            if ident.abi == ELF_OSABI_SYSV {
                "ABI"
            } else {
                "Uncommon ABI(!)"
            },
            abi_to_string(ident.abi),
        ));

        if !(ident.abi == ELF_OSABI_SYSV && ident.abi_ver == 0) {
            information.push((
                "abi_ver",
                if ident.abi == ELF_OSABI_SYSV && ident.abi_ver != 0 {
                    "Uncommon ABI version(!)"
                } else {
                    "ABI version"
                },
                format!("{}", ident.abi_ver),
            ));
        }

        Ok(())
    }

    fn add_ident_ranges(ranges: &mut Ranges) {
        ranges.add_range(0, ELF_EI_NIDENT as usize, RangeType::Ident);

        ranges.add_range(0, 4, RangeType::HeaderDetail("magic"));
        ranges.add_range(4, 1, RangeType::HeaderDetail("class"));
        ranges.add_range(5, 1, RangeType::HeaderDetail("data"));
        ranges.add_range(6, 1, RangeType::HeaderDetail("ver"));
        ranges.add_range(7, 1, RangeType::HeaderDetail("abi"));
        ranges.add_range(8, 1, RangeType::HeaderDetail("abi_ver"));
        ranges.add_range(9, 7, RangeType::HeaderDetail("pad"));
    }
}
