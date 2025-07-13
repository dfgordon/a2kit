//! General disk image access optimized for language servers
//! 
//! This is primarily an interface, the heavy lifting is done in `img` and `fs` modules.

use crate::commands::{ItemType,CommandError};
use crate::fs::DiskFS;
use crate::{STDRESULT,DYNERR};

pub struct DiskServer {
    path_to_img: String,
    disk: Option<Box<dyn DiskFS>>
}

pub struct SimpleFileImage {
    pub file_system: String,
    pub fs_type: Vec<u8>,
    pub load_addr: usize,
    pub data: Vec<u8>
}

pub enum SelectionResult {
    Directory(Vec<String>),
    FileData(SimpleFileImage)
}

impl DiskServer {
    pub fn new() -> Self {
        Self {
            path_to_img: "".to_string(),
            disk: None
        }
    }
    /// Buffer a file system object including its underlying storage.
    /// Any previously mounted disk image is dropped.
    /// The white list can be used to restrict the file systems that are accepted.
    pub fn mount(&mut self,path_to_img: &str,maybe_white_list: &Option<Vec<String>>) -> STDRESULT {
        match crate::create_fs_from_file(path_to_img) {
            Ok(mut disk) => {
                let stat = disk.stat()?;
                match maybe_white_list {
                    Some(white_list) => {
                        if white_list.contains(&stat.fs_name) {
                            self.disk = Some(disk);
                            self.path_to_img = path_to_img.to_string();
                            Ok(())
                        } else {
                            Err(Box::new(CommandError::UnsupportedFormat))
                        }
                    },
                    None => {
                        self.disk = Some(disk);
                        self.path_to_img = path_to_img.to_string();
                        Ok(())
                    }
                }
            },
            Err(e) => Err(e)
        }
    }
    fn evaluate_selection(&mut self,path: &str,maybe_white_list: Option<Vec<String>>) -> Result<SelectionResult,DYNERR> {
        if let Some(disk) = self.disk.as_mut() {
            if let Ok(full_cat) = disk.catalog_to_vec(path) {
                if maybe_white_list.is_none() {
                    return Ok(SelectionResult::Directory(full_cat));
                }
                let mut filtered_cat = Vec::new();
                let mut white_list = maybe_white_list.to_owned().unwrap();
                if !white_list.contains(&"DIR".to_string()) {
                    white_list.push("DIR".to_string());
                }
                for row in full_cat {
                    for typ in &white_list {
                        if row.starts_with(&typ.to_uppercase()) {
                            filtered_cat.push(row);
                            break;
                        }
                    }
                }
                return Ok(SelectionResult::Directory(filtered_cat));
            }
            return match disk.get(path) {
                Ok(fimg) => Ok(SelectionResult::FileData(SimpleFileImage {
                    file_system: fimg.file_system.clone(),
                    fs_type: fimg.fs_type.clone(),
                    load_addr: fimg.get_load_address(),
                    data: match fimg.unpack() {
                        Ok(result) => match result {
                            crate::fs::UnpackedData::Binary(dat) => dat,
                            crate::fs::UnpackedData::Text(txt) => txt.as_bytes().to_vec(),
                            _ => return Err(Box::new(CommandError::UnsupportedFormat))
                        },
                        Err(e) => return Err(e)
                    }
                })),
                Err(e) => Err(e)
            }
        }
        return Err(Box::new(CommandError::InvalidCommand));
    }
    /// Extract path and white list from args and return an enumeration representing the outcome.
    /// If the selection is a directory, return its listing, if a file return a "simplified file image".
    /// Directory listings are generated using the `catalog_to_vec` trait method, so that the white list
    /// should use textual types consistent with that format.
    /// If white list is json falsey show everything, if white list is an empty array show only directories. 
    pub fn handle_selection(&mut self,args: &Vec<serde_json::Value>) -> Result<SelectionResult,DYNERR> {
        if args.len()!=2 {
            return Err(Box::new(CommandError::UnknownFormat));
        }
        let maybe_img_path = serde_json::from_value::<String>(args[0].clone());
        let maybe_white_list = serde_json::from_value::<Vec<String>>(args[1].clone());
        match (maybe_img_path,maybe_white_list) {
            (Ok(path),Ok(white_list)) => self.evaluate_selection(&path, Some(white_list)),
            (Ok(path),Err(_)) => self.evaluate_selection(&path,None),
            _ => Err(Box::new(CommandError::UnknownFormat))
        }
    }
    /// Write any sequential data (BASIC tokens, text, binary) and commit to real disk.
    /// N.b. the path that was used at mount time is assumed valid.
    pub fn write(&mut self,path: &str,dat: &[u8],typ: ItemType) -> STDRESULT {
        if let Some(disk) = self.disk.as_mut() {
            let mut fimg = disk.new_fimg(None, true, path)?;
            match typ {
                ItemType::IntegerTokens | ItemType::ApplesoftTokens => fimg.pack_tok(dat,typ,None)?,
                ItemType::MerlinTokens => fimg.pack_raw(dat)?,
                ItemType::Text => fimg.pack_raw(dat)?,
                _ => return Err(Box::new(CommandError::UnsupportedFormat))
            };
            disk.put(&fimg)?;
            crate::save_img(disk, &self.path_to_img)?;
            Ok(())
        } else {
            Err(Box::new(CommandError::InvalidCommand))
        }
    }
    /// Delete a file or directory.
    /// There is no overwriting in a2kit, but the client will often want to do so.
    /// So the workaround, as usual, is delete first.
    pub fn delete(&mut self,path: &str) -> STDRESULT {
        if let Some(disk) = self.disk.as_mut() {
            disk.delete(path)
        } else {
            Err(Box::new(CommandError::InvalidCommand))
        }
    }
}