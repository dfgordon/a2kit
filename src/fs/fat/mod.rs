//! ## FAT file system module
//! 
//! The File Allocation Table (FAT) file system is named after the structure that
//! keeps track of allocated clusters.
//! The FAT itself is implemented in `crate::bios::fat`, this module makes use of the FAT
//! as part of managing the overall file system.
//! This is geared toward FAT volumes containing MS-DOS.
//! Assumes empty partition table.

// TODO: FAT32 could be huge, impose some limit

mod directory;
mod pack;
mod types;
mod display;

use std::collections::HashMap;
use a2kit_macro::DiskStruct;
use std::str::FromStr;
use std::fmt::Write;
use log::{trace,debug,error};
use types::*;
use directory::*;
use super::Block;
use crate::img;
use crate::bios::{bpb,fat};
use crate::commands::ItemType;
use crate::{DYNERR,STDRESULT};

/// The primary interface for disk operations.
pub struct Disk {
    img: Box<dyn img::DiskImage>,
    boot_sector: bpb::BootSector,
    maybe_fat: Option<Vec<u8>>,
    typ: usize
}

impl Disk {
    fn new_fimg(chunk_len: usize) -> super::FileImage {
        super::FileImage {
            fimg_version: super::FileImage::fimg_version(),
            file_system: String::from("fat"),
            fs_type: vec![0;3],
            aux: vec![],
            eof: vec![0;4],
            created: vec![0;4],
            modified: vec![0;4],
            access: vec![0;2], // only date
            version: vec![12],
            min_version: vec![12],
            chunk_len,
            chunks: HashMap::new()
        }
    }
    /// Create a disk file system using the given image as storage.
    /// The DiskFS takes ownership of the image.
    /// If maybe_boot is None, the FAT boot sector is buffered from the image.
    /// If maybe_boot is Some, the provided boot sector is buffered and written to the image.
    /// If disk layout matches a 160K or 180K disk, the BPB foundation is overwritten with hard coded values.
    pub fn from_img(mut img: Box<dyn img::DiskImage>,maybe_boot: Option<bpb::BootSector>) -> Self {
        let mut boot_sector = match maybe_boot {
            None => {
                let buf = img.read_sector(0, 0, 1).expect("failed to read boot sector");
                bpb::BootSector::from_bytes(&buf)
            },
            Some(b) => {
                img.write_sector(0, 0, 1, &b.to_bytes()).expect("failed to write boot sector");
                b
            }
        };
        match img.kind() {
            img::DiskKind::D525(img::names::IBM_SSDD_8) => {
                boot_sector.replace_foundation(bpb::SSDD_525_8);
            }
            img::DiskKind::D525(img::names::IBM_SSDD_9) => {
                boot_sector.replace_foundation(bpb::SSDD_525_9);
            }
            _ => {}
        }
        let typ = boot_sector.fat_type();
        Self {
            img,
            boot_sector,
            maybe_fat: None,
            typ
        }
    }
    /// Test an image for the FAT file system.
    pub fn test_img(img: &mut Box<dyn img::DiskImage>) -> bool {
        // test the boot sector to see if this is FAT
        if let Ok(boot) = img.read_sector(0,0,1) {
            return bpb::BootSector::verify(&boot);
        }
        debug!("FAT boot sector was not readable");
        return false;
    }
    fn lsec_to_chs(&self,lsec: usize) -> Result<[usize;3],DYNERR> {
        let psec = lsec % self.boot_sector.secs_per_track() as usize;
        let trk = lsec / self.boot_sector.secs_per_track() as usize;
        if trk >= self.img.track_count() {
            return Err(Box::new(Error::SectorNotFound));
        }
        let head = trk % self.boot_sector.heads() as usize;
        let cyl = trk / self.boot_sector.heads() as usize;
        Ok([cyl,head,1+psec])
    }
    fn export_clus(&self,block: usize) -> Block {
        Block::FAT((self.boot_sector.first_cluster_sec(block as u64),self.boot_sector.secs_per_clus()))
    }
    /// Open buffer if not already present.  Will usually be called indirectly.
    /// TODO: which FAT are we using?
    fn open_fat_buffer(&mut self) -> STDRESULT {
        if self.maybe_fat==None {
            let fat_secs = self.boot_sector.fat_secs();
            let mut ans = Vec::new();
            let sec1 = self.boot_sector.res_secs() as u64;
            for isec in sec1..sec1+fat_secs {
                let [cyl,head,sec] = self.lsec_to_chs(isec as usize)?;
                let mut buf = self.img.read_sector(cyl,head,sec)?;
                ans.append(&mut buf);
            }
            self.maybe_fat = Some(ans);
        }
        Ok(())
    }
    /// Get the type and buffer, if buffer doesn't exist it will be opened.
    fn get_fat_buffer(&mut self) -> Result<(usize,&mut Vec<u8>),DYNERR> {
        self.open_fat_buffer()?;
        if let Some(buf) = self.maybe_fat.as_mut() {
            return Ok((self.typ,buf));
        }
        panic!("FAT buffer failed to open");
    }
    /// Buffer needs to be written back when an external caller
    /// asks, directly or indirectly, for the underlying image.
    /// TODO: which FAT are we writing back?
    fn writeback_fat_buffer(&mut self) -> STDRESULT {
        let buf = match self.maybe_fat.as_ref() {
            Some(fat) => fat.clone(),
            None => return Ok(())
        };
        let fat_secs = self.boot_sector.fat_secs();
        let sec1 = self.boot_sector.res_secs() as u64;
        let mut offset = 0;
        for isec in sec1..sec1+fat_secs {
            let [cyl,head,sec] = self.lsec_to_chs(isec as usize)?;
            self.img.write_sector(cyl,head,sec,&buf[offset..])?;
            offset += self.boot_sector.sec_size() as usize;
        }
        Ok(())
    }
    fn is_block_free(&mut self,iblock: usize) -> Result<bool,DYNERR> {
        let (typ,buf) = self.get_fat_buffer()?;
        Ok(fat::is_free(iblock, typ, buf))
    }
    fn num_free_blocks(&mut self) -> Result<usize,DYNERR> {
        let mut free: usize = 0;
        for i in 0..self.boot_sector.cluster_count() as usize {
            if self.is_block_free(i)? {
                free += 1;
            }
        }
        Ok(free)
    }
    /// Free cluster `n`, return next cluster to be freed
    fn deallocate_block(&mut self,n: usize) -> Result<Option<usize>,DYNERR> {
        let (typ,buf) = self.get_fat_buffer()?;
        match fat::is_last(n, typ, buf) {
            true => {
                fat::deallocate(n, typ, buf);
                Ok(None)
            },
            false => {
                let next = fat::get_cluster(n, typ, buf);
                fat::deallocate(n, typ, buf);
                Ok(Some(next as usize))
            }
        }
    }
    /// Read a block (=cluster) and store in buffer starting at offset
    fn read_block(&mut self,data: &mut [u8], iblock: usize, offset: usize) -> STDRESULT {
        let bytes = self.boot_sector.block_size() as i32;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in read block"),
            x if x<=bytes => x,
            _ => bytes
        };
        let cluster = self.export_clus(iblock);
        match self.img.read_block(cluster) {
            Ok(buf) => {
                for i in 0..actual_len as usize {
                    data[offset + i] = buf[i];
                }
                Ok(())
            }
            Err(e) => Err(e)
        }
    }
    /// Write `curr` cluster's data, set EOC mark in FAT, and if `prev >= 2`, update the `prev`
    /// cluster's FAT entry to point to `curr`.
    fn write_block(&mut self,data: &[u8], prev: usize, curr: usize, offset: usize) -> STDRESULT {
        self.zap_block(data,curr,offset)?;
        let (typ,buf) = self.get_fat_buffer()?;
        if prev >= 2 {
            fat::set_cluster(prev, curr as u32, typ, buf);
        }
        fat::mark_last(curr, typ, buf);
        Ok(())
    }
    /// Writes a block of data from buffer `data`, starting at `offset` within the buffer.
    /// If `data` is shorter than the block, trailing bytes are unaffected.
    /// The FAT is not updated.
    fn zap_block(&mut self,data: &[u8], iblock: usize, offset: usize) -> STDRESULT {
        let bytes = self.boot_sector.block_size() as i32;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in write block"),
            x if x<=bytes => x as usize,
            _ => bytes as usize
        };
        let cluster = self.export_clus(iblock);
        self.img.write_block(cluster, &data[offset..offset+actual_len].to_vec())
    }
    fn get_available_block(&mut self) -> Result<Option<usize>,DYNERR> {
        for block in fat::FIRST_DATA_CLUSTER as usize..self.boot_sector.cluster_count() as usize {
            if self.is_block_free(block)? {
                return Ok(Some(block));
            }
        }
        return Ok(None);
    }
    /// Format a disk with the FAT file system, by this point the boot sector is presumed to be buffered.
    /// TODO: implement
    pub fn format(&mut self, vol_name: &str, time: Option<chrono::NaiveDateTime>) -> STDRESULT {
        if !pack::is_name_valid(vol_name) {
            error!("FAT volume name invalid");
            return Err(Box::new(Error::Syntax));
        }
        let sec_size = self.boot_sector.sec_size() as usize;
        let tracks = self.img.track_count();
        let heads = self.boot_sector.heads() as usize;
        let secs = self.boot_sector.secs_per_track() as usize;
        // make sure we start with all 0.
        // n.b. blocks can only access the data region.
        trace!("formatting: zero all");
        let zeroes: Vec<u8> = vec![0;sec_size];
        for cyl in 0..tracks/heads {
            for head in 0..heads {
                for sec in 1..secs+1 {
                    self.img.write_sector(cyl,head,sec,&zeroes)?;
                }
            }
        }
        // write the boot sector (perhaps rewriting)
        self.img.write_sector(0,0,1,&self.boot_sector.to_bytes())?;

        // FAT entry one is the media type
        let (typ,buf) = self.get_fat_buffer()?;
        fat::set_cluster(0, 0xf0, typ, buf);
        // FAT entry two is EOC
        fat::mark_last(1, typ, buf);

        // Create root directory appropriate for FAT type
        match typ {
            32 => {
                // TODO: implement
            },
            _ => {
                let mut dir = Directory::new();
                dir.expand(self.boot_sector.root_dir_entries() as usize);
                let mut label = Entry::create(vol_name,time);
                label.set_attr(directory::VOLUME_ID);
                let loc = EntryLocation {
                    cluster1: None,
                    entry: Ptr::Entry(0),
                    dir
                };
                self.writeback_directory_entry(&loc)?;
            }
        }

        self.writeback_fat_buffer()
    }
    /// given any cluster, return the next cluster, or None if this is the last one.
    /// panics if the current cluster is damaged or free.
    fn next_cluster(&mut self,curr: &Ptr) -> Result<Option<Ptr>,DYNERR> {
        let n = *match curr {
            Ptr::Cluster(n) => n,
            _ => panic!("wrong pointer type")
        };
        let (typ,fat_buf) = self.get_fat_buffer()?;
        if fat::is_damaged(n, typ, fat_buf) {
            panic!("cluster link at {} points to damaged cluster",n);
        }
        if fat::is_free(n, typ, fat_buf) {
            panic!("cluster link at {} points to free cluster",n);
        }
        match fat::is_last(n,typ,fat_buf) {
            true => Ok(None),
            false => Ok(Some(Ptr::Cluster(fat::get_cluster(n, typ, &fat_buf) as usize)))
        }
    }
    /// given any cluster, return the last cluster in its chain.
    /// panics if any damaged or free cluster is found in the chain.
    fn last_cluster(&mut self,initial: &Ptr) -> Result<Ptr,DYNERR> {
        let mut curr = *initial;
        let max_clusters = self.boot_sector.cluster_count() as usize;
        for _i in 0..max_clusters {
            curr = match self.next_cluster(&curr)? {
                None => return Ok(curr),
                Some(next) => next
            }
        }
        Err(Box::new(Error::BadFAT))
    }
    /// given an initial cluster, follow the chain to buffer the entire data set associated with
    /// all the clusters in the chain.
    fn get_cluster_chain_data(&mut self,initial: &Ptr) -> Result<Vec<u8>,DYNERR> {
        let mut ans: Vec<u8> = Vec::new();
        let mut curr = *initial;
        let max_clusters = self.boot_sector.cluster_count() as usize;
        for _i in 0..max_clusters {
            let mut data: Vec<u8> = vec![0;self.boot_sector.block_size() as usize];
            self.read_block(&mut data, curr.unwrap(), 0)?;
            ans.append(&mut data);
            curr = match self.next_cluster(&curr)? {
                None => return Ok(ans),
                Some(next) => next
            };
        }
        Err(Box::new(Error::BadFAT))
    }
    /// given an initial cluster, follow the chain to free fall clusters in the chain.
    fn deallocate_cluster_chain_data(&mut self,initial: &Ptr) -> STDRESULT {
        let mut curr = *initial;
        let max_clusters = self.boot_sector.cluster_count() as usize;
        for _i in 0..max_clusters {
            let maybe_next = self.deallocate_block(curr.unwrap())?;
            curr = match maybe_next {
                None => return Ok(()),
                Some(next) => Ptr::Cluster(next)
            };
        }
        Err(Box::new(Error::BadFAT))
    }
    /// Get the volume label from the boot sector, this is supposed to be the same
    /// as the label obtained from the root directory, if it exists.
    fn get_label(&self) -> Option<String> {
        match self.boot_sector.label() {
            Some(lab) => Some(pack::label_to_string(lab)),
            None => None
        }
    }
    fn is_root(&self,cluster1: &Option<Ptr>) -> bool {
        match (self.typ,cluster1) {
            (32,Some(c)) if c.unwrap() as u64 == self.boot_sector.root_dir_cluster1() => true,
            (32,None) => panic!("FAT32 directory with no first cluster"),
            (_,Some(_)) => false,
            (_,None) => true
        }
    }
    /// Return the (volume label , root directory), if there is no label it is set to "NO NAME" per MS docs
    fn get_root_dir(&mut self) -> Result<(String,Directory),DYNERR> {
       let root = match self.typ {
            32 => {
                let cluster = Ptr::Cluster(self.boot_sector.root_dir_cluster1() as usize);
                debug!("get FAT32 root at cluster {}",cluster.unwrap());
                let buf = self.get_cluster_chain_data(&cluster)?;
                Directory::from_bytes(&buf)
            },
            _ => {
                let mut buf = Vec::new();
                let sec_rng = self.boot_sector.root_dir_sec_rng();
                debug!("get FAT{} root at logical sector {}",self.typ,sec_rng[0]);
                for lsec in (sec_rng[0] as usize)..(sec_rng[1] as usize) {
                    let [cyl,head,sec] = self.lsec_to_chs(lsec)?;
                    buf.append(&mut self.img.read_sector(cyl, head, sec)?);
                }
                Directory::from_bytes(&buf)
            }
        };
        let vol_name = match root.find_label() {
            Some(entry) => entry.name(true),
            None => {
                match self.get_label() {
                    Some(lab) => lab,
                    None => "NO NAME".to_string()
                }
            }
        };
        Ok((vol_name,root))
    }
    /// Return the full directory that starts at cluster1.
    /// If cluster1.is_none(), get the FAT12/FAT16 root directory.
    fn get_directory(&mut self,cluster1: &Option<Ptr>) -> Result<Directory,DYNERR> {
        match cluster1 {
            Some(cluster) => {
                let dir_buf = self.get_cluster_chain_data(&cluster)?;
                Ok(Directory::from_bytes(&dir_buf))
            },
            None => {
                if self.typ==32 {
                    panic!("attempt to get FAT32 root without cluster 1");
                }
                Ok(self.get_root_dir()?.1)
            }
        }
    }
    /// Try to add another block to the directory which starts at `cluster1`.
    /// This is called when the directory runs out of entries.
    /// This only updates buffers, namely the FAT buffer and directory buffer.
    /// Unexpandable root directories do not have a cluster1, so there should be no confusion.
    fn expand_directory(&mut self, dir: &mut Directory,cluster1: &Ptr) -> STDRESULT {

        let new_cluster = match self.get_available_block() {
            Ok(maybe) => match maybe {
                Some(iblock) => iblock,
                None => return Err(Box::new(Error::DiskFull))
            },
            Err(e) => return Err(e)
        };
        let new_entries = self.boot_sector.block_size() as usize / directory::DIR_ENTRY_SIZE;
        let last = self.last_cluster(cluster1)?;
        dir.expand(new_entries);
        // defer writing back the directory, but do update FAT buffer to protect cluster
        // TODO: what if later we change our mind and want to get the cluster back?
        let (typ,fat_buf) = self.get_fat_buffer()?;
        fat::set_cluster(last.unwrap() , new_cluster as u32, typ, fat_buf);
        fat::mark_last(new_cluster, typ, fat_buf);
        Ok(())
    }
    /// Write a changed entry back to disk using the directory buffer.
    /// This zaps only the cluster/sector in which the entry resides, the FAT buffer is assumed already correct.
    /// FAT12 and FAT16 root directories are signaled by loc.cluster1==None.
    fn writeback_directory_entry(&mut self,loc: &EntryLocation) -> STDRESULT {
        let entries_per_cluster = self.boot_sector.block_size() as usize / directory::DIR_ENTRY_SIZE;
        let entries_per_sector = self.boot_sector.sec_size() as usize / directory::DIR_ENTRY_SIZE;
        match loc.cluster1 {
            Some(cluster1) => {
                let mut data: Vec<u8> = Vec::new();
                let cluster = cluster1.unwrap() + loc.entry.unwrap() / entries_per_cluster;
                let entry_beg = (cluster - cluster1.unwrap()) * entries_per_cluster;
                for i in entry_beg..entry_beg+entries_per_cluster {
                    data.append(&mut loc.dir.get_raw_entry(&Ptr::Entry(i)).to_vec());
                }
                self.zap_block(&data, cluster, 0)
            },
            None => {
                // this is a root directory in the reserved area (FAT12/16)
                let mut data: Vec<u8> = Vec::new();
                let [sec_beg,_sec_end] = self.boot_sector.root_dir_sec_rng();
                let lsec = sec_beg as usize + loc.entry.unwrap() / entries_per_sector;
                let entry_beg = (lsec - sec_beg as usize) * entries_per_sector;
                for i in entry_beg..entry_beg+entries_per_cluster {
                    data.append(&mut loc.dir.get_raw_entry(&Ptr::Entry(i)).to_vec());
                }
                let [cyl,head,sec] = self.lsec_to_chs(lsec)?;
                self.img.write_sector(cyl, head, sec, &data)
            }
        }
    }
    /// Get the next available entry location.
    /// Will try to expand the directory buffer when necessary if cluster 1 is provided.
    fn get_available_entry(&mut self, dir: &mut Directory, maybe_cluster1: &Option<Ptr>) -> Result<Ptr,DYNERR> {
        let num = dir.num_entries();
        for i in 0..num {
            match dir.get_type(&Ptr::Entry(i)) {
                EntryType::Free | EntryType::FreeAndNoMore => return Ok(Ptr::Entry(i)),
                _ => continue
            }
        }
        if let Some(cluster1) = maybe_cluster1 {
            self.expand_directory(dir, cluster1)?;
            return Ok(Ptr::Entry(num));
        }
        Err(Box::new(Error::DirectoryFull))
    }
    /// Put path as [volume,subdir,subdir,...,last] where last could be an empty string,
    /// which indicates this is a directory.  If last is not empty, it could be either directory or file.
    /// Also check that the path is not too long.
    fn normalize_path(&mut self,vol_name: &str,path: &str) -> Result<Vec<String>,DYNERR> {
        let mut path_nodes: Vec<String> = path.split("/").map(|s| s.to_string().to_uppercase()).collect();
        if &path[0..1]!="/" {
            path_nodes.insert(0,vol_name.to_string());
        } else {
            path_nodes = path_nodes[1..].to_vec();
        }
        // check path length
        let mut len = 0;
        for s in path_nodes.iter() {
            len += 1 + s.len();
        }
        if len>63 {
            error!("MS-DOS path too long {}",len);
            return Err(Box::new(Error::Syntax));
        }
        return Ok(path_nodes);
    }
    /// split the path into the last node (file or directory) and its parent path
    fn split_path(&mut self,path: &str) -> Result<[String;2],DYNERR> {
        let (vol_name,_root) = self.get_root_dir()?;
        let mut path_nodes = self.normalize_path(&vol_name,path)?;
        // if last node is empty, remove it (means we have a directory)
        if path_nodes[path_nodes.len()-1].len()==0 {
            path_nodes = path_nodes[0..path_nodes.len()-1].to_vec();
        }
        let name = path_nodes[path_nodes.len()-1].clone();
        if path_nodes.len()<2 {
            return Err(Box::new(Error::FileNotFound));
        } else {
            path_nodes = path_nodes[0..path_nodes.len()-1].to_vec();
        }
        let parent_path: String = path_nodes.iter().map(|s| "/".to_string() + s).collect::<Vec<String>>().concat();
        return Ok([parent_path,name]);
    }
    /// will return (parent directory,file/dir), or (None,file/dir), in case file/dir is root directory
    fn goto_path(&mut self,path: &str) -> Result<(Option<FileInfo>,FileInfo),DYNERR> {
        let (vol_name,root) = self.get_root_dir()?;
        let mut parent_info: Option<FileInfo> = None;
        let root_info: FileInfo = FileInfo::create_root(match self.typ {
            12 | 16 => 0,
            32 => self.boot_sector.root_dir_cluster1() as usize,
            _ => panic!("unexpected FAT type")
        });
        let path_nodes = self.normalize_path(&vol_name,path)?;
        // path_nodes = [volume,dir,dir,...,dir|file|empty]
        let n = path_nodes.len();
        // if this is the root directory, return special FileInfo
        if n<3 && path_nodes[n-1]=="" {
            return Ok((parent_info,root_info));
        }
        // walk the tree
        let mut files = root.build_files()?;
        parent_info = Some(root_info);
        for level in 1..n {
            let subdir = path_nodes[level].clone();
            let curr = match directory::get_file(&subdir, &files) {
                Some(finfo) => finfo.clone(),
                None => return Err(Box::new(Error::FileNotFound))
            };
            let terminus = level == n-1;
            let null_terminus = level == n-2 && path_nodes[n-1] == "";
            if terminus || null_terminus {
                return Ok((parent_info,curr));
            }
            let new_dir = self.get_directory(&curr.cluster1)?;
            files = new_dir.build_files()?;
            parent_info = Some(curr);
        }
        return Err(Box::new(Error::FileNotFound));
    }
    /// Read any file into a file image
    fn read_file(&mut self,parent: &FileInfo,finfo: &FileInfo) -> Result<super::FileImage,DYNERR> {
        let mut fimg = Disk::new_fimg(self.boot_sector.block_size() as usize);
        // TODO: eliminate redundancy, by this time the directory as already been read at least once
        let dir = self.get_directory(&parent.cluster1)?;
        let entry = dir.get_entry(&Ptr::Entry(finfo.idx));
        entry.metadata_to_fimg(&mut fimg);
        let all_data = self.get_cluster_chain_data(&finfo.cluster1.unwrap())?;
        fimg.desequence(&all_data);
        Ok(fimg)
    }
    /// If the new name does not already exist, return an EntryLocation with entry pointer set to existing file.
    /// If the new name does exist, return an error.
    fn ok_to_rename(&mut self,old_path: &str,new_name: &str) -> Result<EntryLocation,DYNERR> {
        if !pack::is_name_valid(new_name) {
            error!("invalid MS-DOS name {}",new_name);
            return Err(Box::new(Error::Syntax));
        }
        if let Ok((maybe_parent,file_info)) = self.goto_path(old_path) {
            if let Some(parent) = maybe_parent {
                let search_dir = self.get_directory(&parent.cluster1)?;
                let files = search_dir.build_files()?;
                if !files.contains_key(new_name) {
                    return Ok(EntryLocation { cluster1: parent.cluster1, entry: Ptr::Entry(file_info.idx), dir: search_dir })
                } else {
                    // TODO: this check did not work
                    return Err(Box::new(Error::DuplicateFile));
                }
            } else {
                error!("cannot rename root");
                return Err(Box::new(Error::General));
            }
        }
        return Err(Box::new(Error::FileNotFound));
    }
    /// Prepare a directory for a new file or subdirectory.  The directory buffer and FAT buffer will change
    /// if the directory needs to grow.
    fn prepare_to_write(&mut self,path: &str) -> Result<(String,EntryLocation),DYNERR> {
        let [parent_path,new_name] = self.split_path(path)?;
        if !pack::is_name_valid(&new_name) {
            error!("invalid MS-DOS name {}",&new_name);
            return Err(Box::new(Error::Syntax));
        }
        if let Ok((_maybe_grandparent,parent)) = self.goto_path(&parent_path) {
            let mut search_dir = self.get_directory(&parent.cluster1)?;
            let files = search_dir.build_files()?;
            if files.contains_key(&new_name) {
                return Err(Box::new(Error::DuplicateFile));
            } else {
                return match self.get_available_entry(&mut search_dir, &parent.cluster1) {
                    Ok(ptr) => Ok((new_name,EntryLocation { cluster1: parent.cluster1, entry: ptr, dir: search_dir})),
                    Err(e) => Err(e)
                } 
            }
        }
        return Err(Box::new(Error::FileNotFound));
    }
    /// Write any file.  Use `FileImage::desequence` to load sequential data into the file image.
    /// The entry must already exist and point to the next available block.
    /// Also writes back changes to the directory (but FAT remains in buffer).
    fn write_file(&mut self,loc: &mut EntryLocation,fimg: &super::FileImage) -> Result<usize,DYNERR> {
        let mut entry = loc.dir.get_entry(&loc.entry);
        entry.fimg_to_metadata(fimg)?;
        if self.num_free_blocks()? < fimg.end() {
            return Err(Box::new(Error::DiskFull));
        }
        let mut prev: usize = 0;
        for count in 0..fimg.end() {
            if let Some(data) = fimg.chunks.get(&count) {
                if let Some(curr) = self.get_available_block()? {
                    self.write_block(data, prev, curr, 0)?;
                    prev = curr;
                } else {
                    panic!("unexpectedly ran out of disk space");
                }
            } else {
                error!("FAT file image had a hole which is not allowed");
                return Err(Box::new(Error::WriteFault));
            }
        }
        entry.set_attr(directory::ARCHIVE);
        loc.dir.set_entry(&loc.entry, &entry);
        self.writeback_directory_entry(loc)?;
        return Ok(entry.eof());
    }
    /// modify a file entry, optionally change attributes, rename; attempt to rename read-only file will fail.
    fn modify(&mut self,loc: &mut EntryLocation,maybe_set: Option<u8>,maybe_clear: Option<u8>,maybe_new_name: Option<&str>) -> STDRESULT {  
        let mut entry = loc.dir.get_entry(&loc.entry);
        if entry.get_attr(directory::READ_ONLY) && maybe_new_name.is_some() {
            return Err(Box::new(Error::WriteProtect));
        }
        if let Some(mask) = maybe_set {
            entry.set_attr(mask);
        }
        if let Some(mask) = maybe_clear {
            entry.clear_attr(mask);
        }
        if let Some(new_name) = maybe_new_name {
            if pack::is_name_valid(new_name) {
                entry.rename(new_name);
            } else {
                return Err(Box::new(Error::Syntax));
            }
        }
        entry.set_attr(directory::ARCHIVE);
        loc.dir.set_entry(&loc.entry, &entry);
        self.writeback_directory_entry(&loc)
    }
}

impl super::DiskFS for Disk {
    fn new_fimg(&self,chunk_len: usize) -> super::FileImage {
        Disk::new_fimg(chunk_len)
    }
    fn catalog_to_stdout(&mut self, path: &str) -> STDRESULT {
        // TODO: resolve path into directory, wildcard spec, and /w option
        let (_maybe_parent,dir_info) = self.goto_path(path)?;
        if !dir_info.directory {
            error!("directory flag not set");
            return Err(Box::new(Error::ReadFault));
        }
        let dir = self.get_directory(&dir_info.cluster1)?;
        display::dir(&dir,&self.boot_sector,"")
    }
    fn create(&mut self,path: &str) -> STDRESULT {
        let (name,loc) = self.prepare_to_write(path)?;
        if let Some(new_cluster) = self.get_available_block()? {
            let parent_cluster = match loc.cluster1 {
                Some(c) => c.unwrap(),
                None => 0 // this holds even for FAT32
            };
            let (_entry, dir_data) = Entry::create_subdir(&name,parent_cluster,new_cluster,self.boot_sector.block_size() as usize,None);
            self.write_block(&dir_data, 0, new_cluster, 0)?;
            self.writeback_directory_entry(&loc)    
        } else {
            Err(Box::new(Error::DiskFull))
        }
    }
    fn delete(&mut self,path: &str) -> STDRESULT {
        let (maybe_parent,finfo) = self.goto_path(path)?;
        // files and directories can be delete the same way, except for a directory we
        // need some additional checks first:
        if finfo.directory {
            if self.is_root(&finfo.cluster1) {
                error!("cannot delete root directory");
                return Err(Box::new(Error::WriteProtect));
            }
            let dir = self.get_directory(&finfo.cluster1)?;
            let files = dir.build_files()?;
            if files.len() > 2 {
                error!("cannot delete directory with {} files",files.len()-2);
                return Err(Box::new(Error::DirectoryNotEmpty));
            }
        }
        match maybe_parent {
            Some(parent) => {
                let dir = self.get_directory(&parent.cluster1)?;
                let entry_ptr = Ptr::Entry(finfo.idx);
                let mut entry = dir.get_entry(&entry_ptr);
                entry.erase(false);
                self.writeback_directory_entry(&EntryLocation {
                    cluster1: parent.cluster1,
                    entry: entry_ptr,
                    dir
                })?;
                if let Some(cluster1) = finfo.cluster1 {
                    self.deallocate_cluster_chain_data(&cluster1)?;
                }
                Ok(())
            },
            None => panic!("file with no parent directory {}",path)
        }
    }
    fn protect(&mut self,_path: &str,_password: &str,_read: bool,_write: bool,_delete: bool) -> STDRESULT {
        error!("FAT does not support operation");
        Err(Box::new(Error::Syntax))
    }
    fn unprotect(&mut self,_path: &str) -> STDRESULT {
        error!("FAT does not support operation");
        Err(Box::new(Error::Syntax))
    }
    fn lock(&mut self,path: &str) -> STDRESULT {
        let (maybe_parent,finfo) = self.goto_path(path)?;
        match maybe_parent {
            Some(parent) => {
                let dir = self.get_directory(&parent.cluster1)?;
                let mut loc = EntryLocation {
                    cluster1: parent.cluster1,
                    entry: Ptr::Entry(finfo.idx),
                    dir
                };
                self.modify(&mut loc,Some(directory::READ_ONLY),None,None)
            },
            None => {
                error!("cannot lock root");
                Err(Box::new(Error::General))
            }
        }
    }
    fn unlock(&mut self,path: &str) -> STDRESULT {
        let (maybe_parent,finfo) = self.goto_path(path)?;
        match maybe_parent {
            Some(parent) => {
                let dir = self.get_directory(&parent.cluster1)?;
                let mut loc = EntryLocation {
                    cluster1: parent.cluster1,
                    entry: Ptr::Entry(finfo.idx),
                    dir
                };
                self.modify(&mut loc,None,Some(directory::READ_ONLY),None)
            },
            None => {
                error!("cannot unlock root");
                Err(Box::new(Error::General))
            }
        }
    }
    fn rename(&mut self,path: &str,name: &str) -> STDRESULT {
        self.ok_to_rename(path, name)?;
        let (maybe_parent,finfo) = self.goto_path(path)?;
        match maybe_parent {
            Some(parent) => {
                let dir = self.get_directory(&parent.cluster1)?;
                let mut loc = EntryLocation {
                    cluster1: parent.cluster1,
                    entry: Ptr::Entry(finfo.idx),
                    dir
                };
                self.modify(&mut loc,None,None,Some(name))
            },
            None => {
                error!("cannot rename root");
                Err(Box::new(Error::General))
            }
        }
    }
    fn retype(&mut self,path: &str,new_type: &str,_sub_type: &str) -> STDRESULT {
        let (maybe_parent,finfo) = self.goto_path(path)?;
        if finfo.directory {
            error!("cannot retype directory");
            return Err(Box::new(Error::General));
        }
        match maybe_parent {
            Some(parent) => {
                let dir = self.get_directory(&parent.cluster1)?;
                let mut loc = EntryLocation {
                    cluster1: parent.cluster1,
                    entry: Ptr::Entry(finfo.idx),
                    dir
                };
                match new_type {
                    "sys" => self.modify(&mut loc,Some(SYSTEM),None,None),
                    "reg" => self.modify(&mut loc,None,Some(SYSTEM),None),
                    "hid" => self.modify(&mut loc,Some(HIDDEN),None,None),
                    "vis" => self.modify(&mut loc,None,Some(HIDDEN),None),
                    _ => {
                        error!("valid types are sys, reg, hid, vis");
                        Err(Box::new(Error::General))
                    }
                }
            },
            None => {
                error!("cannot retype root");
                Err(Box::new(Error::General))
            }
        }
    }
    fn bload(&mut self,path: &str) -> Result<(u16,Vec<u8>),DYNERR> {
        self.read_raw(path,true)
    }
    fn bsave(&mut self,path: &str, dat: &[u8],_start_addr: u16,trailing: Option<&[u8]>) -> Result<usize,DYNERR> {
        let padded = match trailing {
            Some(v) => [dat.to_vec(),v.to_vec()].concat(),
            None => dat.to_vec()
        };
        let mut fimg = Disk::new_fimg(self.boot_sector.block_size() as usize);
        fimg.desequence(&padded);
        return self.write_any(path,&fimg);
    }
    fn load(&mut self,_path: &str) -> Result<(u16,Vec<u8>),DYNERR> {
        error!("MS-DOS implementation does not support operation");
        return Err(Box::new(Error::Syntax));
    }
    fn save(&mut self,_path: &str, _dat: &[u8], _typ: ItemType, _trailing: Option<&[u8]>) -> Result<usize,DYNERR> {
        error!("MS-DOS implementation does not support operation");
        return Err(Box::new(Error::Syntax));
    }
    fn read_raw(&mut self,path: &str,trunc: bool) -> Result<(u16,Vec<u8>),DYNERR> {
        let (maybe_parent,finfo) = self.goto_path(path)?;
        if let Some(parent) = maybe_parent {
            let fimg = self.read_file(&parent,&finfo)?;
            if trunc {
                let eof = super::FileImage::usize_from_truncated_le_bytes(&fimg.eof);
                return Ok((0,fimg.sequence_limited(eof)));
            } else {
                return Ok((0,fimg.sequence()));
            }
        }
        error!("cannot read root as a file");
        Err(Box::new(Error::ReadFault))
    }
    fn write_raw(&mut self,path: &str, dat: &[u8]) -> Result<usize,DYNERR> {
        let mut fimg = Disk::new_fimg(self.boot_sector.block_size() as usize);
        fimg.desequence(dat);
        return self.write_any(path,&fimg);
    }
    fn read_text(&mut self,path: &str) -> Result<(u16,Vec<u8>),DYNERR> {
        self.read_raw(path,true)
    }
    fn write_text(&mut self,path: &str, dat: &[u8]) -> Result<usize,DYNERR> {
        self.write_raw(path,dat)
    }
    fn read_records(&mut self,_path: &str,_record_length: usize) -> Result<super::Records,DYNERR> {
        error!("FAT does not support operation");
        return Err(Box::new(Error::Syntax));
    }
    fn write_records(&mut self,_path: &str, _records: &super::Records) -> Result<usize,DYNERR> {
        error!("FAT does not support operation");
        return Err(Box::new(Error::Syntax));
    }
    fn read_block(&mut self,num: &str) -> Result<(u16,Vec<u8>),DYNERR> {
        match usize::from_str(num) {
            Ok(block) => {
                let mut buf: Vec<u8> = vec![0;self.boot_sector.block_size() as usize];
                if block >= 2 + self.boot_sector.cluster_count() as usize {
                    return Err(Box::new(Error::SectorNotFound));
                }
                self.read_block(&mut buf,block,0)?;
                Ok((0,buf))
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn write_block(&mut self, num: &str, dat: &[u8]) -> Result<usize,DYNERR> {
        match usize::from_str(num) {
            Ok(block) => {
                if dat.len() > self.boot_sector.block_size() as usize || block >= 2 + self.boot_sector.cluster_count() as usize {
                    return Err(Box::new(Error::SectorNotFound));
                }
                self.zap_block(dat,block,0)?;
                Ok(dat.len())
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn read_any(&mut self,path: &str) -> Result<super::FileImage,DYNERR> {
        let (maybe_parent,finfo) = self.goto_path(path)?;
        if let Some(parent) = maybe_parent {
            return Ok(self.read_file(&parent,&finfo)?);
        }
        error!("cannot read root as a file");
        Err(Box::new(Error::ReadFault))
    }
    fn write_any(&mut self,path: &str,fimg: &super::FileImage) -> Result<usize,DYNERR> {
        if fimg.file_system!="fat" {
            error!("cannot write {} file image to FAT",fimg.file_system);
            return Err(Box::new(Error::WriteFault));
        }
        if fimg.chunk_len!=self.boot_sector.block_size() as usize {
            error!("chunk length {} is incompatible with this file image",fimg.chunk_len);
            return Err(Box::new(Error::IncorrectDOS));
        }
        match self.prepare_to_write(path) {
            Ok((name,mut loc)) => {
                // create the entry
                let mut entry = Entry::create(&name,None);
                entry.fimg_to_metadata(fimg)?;
                loc.dir.set_entry(&loc.entry, &entry);
                // write blocks
                match self.write_file(&mut loc,fimg) {
                    Ok(len) => Ok(len),
                    Err(e) => Err(e)
                }
            },
            Err(e) => Err(e)
        }
    }
    fn decode_text(&self,dat: &[u8]) -> Result<String,DYNERR> {
        let file = types::SequentialText::from_bytes(&dat.to_vec());
        Ok(file.to_string())
    }
    fn encode_text(&self,s: &str) -> Result<Vec<u8>,DYNERR> {
        let file = types::SequentialText::from_str(&s);
        match file {
            Ok(txt) => Ok(txt.to_bytes()),
            Err(_) => {
                error!("Cannot encode, perhaps use raw type");
                Err(Box::new(Error::Syntax))
            }
        }
    }
    fn standardize(&mut self,ref_con: u16) -> HashMap<Block,Vec<usize>> {
        let mut ans: HashMap<Block,Vec<usize>> = HashMap::new();
        return ans;
    }
    fn compare(&mut self,path: &std::path::Path,ignore: &HashMap<Block,Vec<usize>>) {
        self.writeback_fat_buffer().expect("disk error");
        let mut emulator_disk = crate::create_fs_from_file(&path.to_str().unwrap()).expect("read error");
        for block in 2..2+self.boot_sector.cluster_count() {
            let addr = self.export_clus(block as usize);
            let mut actual = self.img.read_block(addr).expect("bad sector access");
            let mut expected = emulator_disk.get_img().read_block(addr).expect("bad sector access");
            if let Some(ignorable) = ignore.get(&addr) {
                for offset in ignorable {
                    actual[*offset] = 0;
                    expected[*offset] = 0;
                }
            }
            for row in 0..16 {
                let mut fmt_actual = String::new();
                let mut fmt_expected = String::new();
                let offset = row*32;
                write!(&mut fmt_actual,"{:02X?}",&actual[offset..offset+32].to_vec()).expect("format error");
                write!(&mut fmt_expected,"{:02X?}",&expected[offset..offset+32].to_vec()).expect("format error");
                assert_eq!(fmt_actual,fmt_expected," at block {}, row {}",block,row)
            }
        }
    }
    fn get_img(&mut self) -> &mut Box<dyn img::DiskImage> {
        self.writeback_fat_buffer().expect("could not write back bitmap buffer");
        &mut self.img
    }
}

#[test]
fn test_path_normalize() {
    let img = Box::new(crate::img::imd::Imd::create(img::DiskKind::D525(img::names::IBM_SSDD_9)));
    let boot_sec = bpb::BootSector::create1216(bpb::SSDD_525_9);
    let mut disk = Disk::from_img(img,Some(boot_sec));
    disk.format(&String::from("DISK 1"),None).expect("disk error");
    match disk.normalize_path("DISK 1","DIR1") {
        Ok(res) => assert_eq!(res,["DISK 1","DIR1"]),
        Err(e) => panic!("{}",e)
    }
    match disk.normalize_path("DISK 1","dir1/") {
        Ok(res) => assert_eq!(res,["DISK 1","DIR1",""]),
        Err(e) => panic!("{}",e)
    }
    match disk.normalize_path("DISK 1","dir1/sub2") {
        Ok(res) => assert_eq!(res,["DISK 1","DIR1","SUB2"]),
        Err(e) => panic!("{}",e)
    }
    match disk.normalize_path("DISK 1","/disk 2/dir1/sub2") {
        Ok(res) => assert_eq!(res,["DISK 2","DIR1","SUB2"]),
        Err(e) => panic!("{}",e)
    }
    match disk.normalize_path("DISK 1","abcdefghijklmno/abcdefghijklmno/abcdefghijklmno/abcdefghijklmno/abcdefghijklmno/abcdefghijklmno/abcdefghijklmno/abcdefghijklmno") {
        Ok(_res) => panic!("normalize_path should have failed with path too long"),
        Err(e) => assert_eq!(e.to_string(),"syntax")
    }
}

