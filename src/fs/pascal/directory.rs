//! ### Pascal directory structures
//! 
//! These are fixed length structures, with the DiskStruct trait.

// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
use a2kit_macro::{DiskStructError,DiskStruct};
use a2kit_macro_derive::DiskStruct;

use super::types::ENTRY_SIZE;

// Following are representations of disk directory structures
// these are mostly fixed length structures where the DiskStruct
// trait can be automatically derived.

#[derive(DiskStruct)]
pub struct VolDirHeader {
    pub begin_block: [u8;2],
    pub end_block: [u8;2],
    pub file_type: [u8;2], // 0
    pub name_len: u8, // & 0x07 (LS 3 bits = max 7)
    pub name: [u8;7],
    pub total_blocks: [u8;2],
    pub num_files: [u8;2],
    pub last_access_date: [u8;2],
    pub last_set_date: [u8;2],
    pub pad: [u8;4]
}

#[derive(DiskStruct,Copy,Clone)]
pub struct DirectoryEntry {
    pub begin_block: [u8;2],
    pub end_block: [u8;2],
    pub file_type: [u8;2],
    pub name_len: u8, // & 0x0f (LS 4 bits = max 15)
    pub name: [u8;15],
    pub bytes_remaining: [u8;2],
    pub mod_date: [u8;2]
}

// The directory is simply the header followed immediately by
// packed entries.  The entries are allowed to cross block boundaries.
pub struct Directory {
    pub header: VolDirHeader,
    pub entries: Vec<DirectoryEntry>
}

impl Directory {
    pub fn total_blocks(&self) -> usize {
        u16::from_le_bytes(self.header.total_blocks) as usize
    }
}

// The DiskStruct trait is not ideally suited to a scheme where structures
// are allowed to cross block-boundaries, but can be made to serve.
impl DiskStruct for Directory {
    fn new() -> Self {
        Self {
            header: VolDirHeader::new(),
            entries: Vec::new()
        }
    }
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.header.to_bytes());
        for i in 0..self.entries.len() {
            ans.append(&mut self.entries[i].to_bytes());
        }
        return ans;
    }
    fn update_from_bytes(&mut self,bytes: &[u8]) -> Result<(),DiskStructError> {
        // depending on equality of header and entry lengths
        let num_entries = bytes.len()/ENTRY_SIZE - 1;
        self.header.update_from_bytes(&bytes[0..ENTRY_SIZE])?;
        self.entries = Vec::new();
        for i in 0..num_entries {
            if (i+1)*ENTRY_SIZE > bytes.len() {
                return Err(DiskStructError::OutOfData);
            }
            let entry_buf = DirectoryEntry::from_bytes(&bytes[i*ENTRY_SIZE..(i+1)*ENTRY_SIZE])?;
            self.entries.push(entry_buf);
        }
        Ok(())
    }
    fn from_bytes(bytes: &[u8]) -> Result<Self,DiskStructError> {
        let mut ans = Self::new();
        ans.update_from_bytes(bytes)?;
        Ok(ans)
    }
    fn len(&self) -> usize {
        // depending on equality of header and entry lengths
        return self.header.len()*(1 + self.entries.len());
    }
}
