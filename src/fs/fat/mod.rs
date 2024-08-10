//! ## FAT file system module
//! 
//! The File Allocation Table (FAT) file system is named after the structure that
//! keeps track of allocated clusters.
//! The FAT itself is implemented in `crate::bios::fat`, this module makes use of the FAT
//! as part of managing the overall file system.
//! This is geared toward FAT volumes containing MS-DOS.
//! Assumes empty partition table.

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
use crate::{DYNERR,STDRESULT};

pub const FS_NAME: &str = "fat";

pub fn new_fimg(chunk_len: usize,set_time: bool,path: &str) -> Result<super::FileImage,DYNERR> {
    if !pack::is_path_valid(path) {
        return Err(Box::new(Error::Syntax))
    }
    let created = match set_time {
        true => [
            vec![pack::pack_tenths(None)],
            pack::pack_time(None).to_vec(),
            pack::pack_date(None).to_vec()
        ].concat(),
        false => vec![0;5]
    };
    Ok(super::FileImage {
        fimg_version: super::FileImage::fimg_version(),
        file_system: String::from(FS_NAME),
        fs_type: vec![0;3],
        aux: vec![],
        eof: vec![0;4],
        accessed: created.clone(),
        created: created.clone(),
        modified: vec![created[1],created[2],created[3],created[4]],
        access: vec![0], // attributes, do not confuse with access time
        version: vec![12],
        min_version: vec![12],
        chunk_len,
        full_path: path.to_string(),
        chunks: HashMap::new()
    })
}

pub struct Packer {
}

/// The primary interface for disk operations.
pub struct Disk {
    img: Box<dyn img::DiskImage>,
    boot_sector: bpb::BootSector,
    maybe_fat: Option<Vec<u8>>,
    /// only valid during glob search
    curr_path: Vec<String>,
    typ: usize
}

impl Disk {
    /// Create a disk file system using the given image as storage.
    /// The DiskFS takes ownership of the image.
    /// If maybe_boot is None, the FAT boot sector is buffered from the image.
    /// If maybe_boot is Some, the provided boot sector is buffered and written to the image.
    /// If disk layout matches a 160K or 180K disk, the BPB foundation is overwritten with hard coded values,
    /// but only in the buffer, i.e., without affecting the boot sector on the image.
    pub fn from_img(mut img: Box<dyn img::DiskImage>,maybe_boot: Option<bpb::BootSector>) -> Result<Self,DYNERR> {
        let mut boot_sector = match maybe_boot {
            None => {
                let buf = img.read_sector(0, 0, 1)?;
                bpb::BootSector::from_bytes(&buf)?
            },
            Some(b) => {
                img.write_sector(0, 0, 1, &b.to_bytes())?;
                b
            }
        };
        match img.kind() {
            img::DiskKind::D525(img::names::IBM_SSDD_8) => {
                boot_sector.replace_foundation(&img.kind())?;
            }
            img::DiskKind::D525(img::names::IBM_SSDD_9) => {
                boot_sector.replace_foundation(&img.kind())?;
            }
            _ => {}
        }
        let typ = boot_sector.fat_type();
        Ok(Self {
            img,
            boot_sector,
            maybe_fat: None,
            curr_path: Vec::new(),
            typ
        })
    }
    /// Create an MS-DOS 1.0 file system using the given image as storage.
    /// The DiskFS takes ownership of the image.
    pub fn from_img_dos1x(img: Box<dyn img::DiskImage>) -> Result<Self,DYNERR> {
        let boot_sector = bpb::BootSector::create(&img.kind())?;
        let typ = boot_sector.fat_type();
        Ok(Self {
            img,
            boot_sector,
            maybe_fat: None,
            curr_path: Vec::new(),
            typ
        })
    }
    /// Test an image for the FAT file system.
    pub fn test_img(img: &mut Box<dyn img::DiskImage>) -> bool {
        // test the boot sector to see if this is FAT
        if let Ok(boot) = img.read_sector(0,0,1) {
            return bpb::BootSector::verify(&boot);
        }
        debug!("boot sector was not readable");
        return false;
    }
    /// Test an image for DOS 1.x (no signature, no BPB)
    pub fn test_img_dos1x(img: &mut Box<dyn img::DiskImage>) -> bool {
        // We are going to look in the first FAT.
        // We look in the boot sector only for logging purposes.
        // By this time layout or size has already been used to create the `DiskImage`.
        let mut ans = true;
        // first look at boot sector, but only for logging
        if let Ok(boot) = img.read_sector(0,0,1) {
            if boot[0]!=0xeb {
                debug!("JMP mismatch {}",boot[0]);
            }
            let boot_str = String::from_utf8_lossy(&boot);
            let s1 = "Microsoft";
            let s2 = "com";
            let s3 = "dos";
            if !boot_str.contains(s1) && !boot_str.contains(s2) && !boot_str.contains(s3) {
                debug!("no string matches");
            }
        } else {
            debug!("boot sector was not readable");
            ans = false;
        }
        // buffer a temporary boot sector for this disk kind.
        // we only use this to analyze the FAT.
        if let Ok(boot) = bpb::BootSector::create(&img.kind()) {
            let mut buf = Vec::new();
            let sec1 = boot.res_secs() as u64;
            for isec in sec1..sec1+boot.fat_secs() {
                debug!("load FAT at sec {}",isec);
                let mut sec_buf = match img.read_sector(0, 0, 1 + isec as usize) {
                    Ok(b) => b,
                    Err(_) => {
                        debug!("could not read FAT");
                        return false;
                    }
                };
                buf.append(&mut sec_buf);
            }
            if buf[0]!=boot.media_byte() {
                debug!("wrong media type {} != {}",buf[0],boot.media_byte());
                ans = false;
            }
            let beg = fat::FIRST_DATA_CLUSTER as usize;
            let end = beg + boot.cluster_count_usable() as usize;
            for n in beg..end {
                if fat::is_damaged(n, 12, &buf) || fat::is_free(n, 12, &buf) || fat::is_last(n, 12, &buf) {
                    continue;
                }
                let val = fat::get_cluster(n, 12, &buf) as usize;
                trace!("cluster link {} -> {}",n,val);
                if val < beg || val >= end {
                    debug!("wrong cluster link {} -> {}",n,val);
                    ans = false;
                    break;
                }
            }
        } else {
            debug!("could not guess FAT parameters");
            ans = false;
        }
        ans
    }
    fn get_chs(&self,ptr: &Ptr) -> Result<[usize;3],DYNERR> {
        let lsec = match ptr {
            Ptr::LogicalSector(s) => *s,
            _ => panic!("wrong pointer type")
        };
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
    fn clus_in_rng(&self,block: usize) -> bool {
        block >= fat::FIRST_DATA_CLUSTER as usize && block < fat::FIRST_DATA_CLUSTER as usize + self.boot_sector.cluster_count_usable() as usize
    }
    /// Open buffer if not already present.  Will usually be called indirectly.
    /// The backup FAT will be written when the buffer is written back.
    fn open_fat_buffer(&mut self) -> STDRESULT {
        if self.maybe_fat==None {
            trace!("buffering first FAT");
            let fat_secs = self.boot_sector.fat_secs();
            let mut ans = Vec::new();
            let sec1 = self.boot_sector.res_secs() as u64;
            for isec in sec1..sec1+fat_secs {
                let [cyl,head,sec] = self.get_chs(&Ptr::LogicalSector(isec as usize))?;
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
    fn writeback_fat_buffer(&mut self) -> STDRESULT {
        let buf = match self.maybe_fat.as_ref() {
            Some(fat) => fat.clone(),
            None => return Ok(())
        };
        let num_fats = self.boot_sector.num_fats();
        let fat_secs = self.boot_sector.fat_secs();
        let mut sec1 = self.boot_sector.res_secs() as u64;
        let mut offset = 0;
        for _fat in 0..num_fats {
            for isec in sec1..sec1+fat_secs {
                let [cyl,head,sec] = self.get_chs(&Ptr::LogicalSector(isec as usize))?;
                self.img.write_sector(cyl,head,sec,&buf[offset..])?;
                offset += self.boot_sector.sec_size() as usize;
            }
            offset = 0;
            sec1 += fat_secs;
        }
        Ok(())
    }
    fn is_block_free(&mut self,iblock: usize) -> Result<bool,DYNERR> {
        let (typ,buf) = self.get_fat_buffer()?;
        Ok(fat::is_free(iblock, typ, buf))
    }
    fn num_free_blocks(&mut self) -> Result<usize,DYNERR> {
        let mut free: usize = 0;
        let beg = fat::FIRST_DATA_CLUSTER as usize;
        for i in beg..beg+self.boot_sector.cluster_count_usable() as usize {
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
        self.img.write_block(cluster, &data[offset..offset+actual_len])
    }
    fn get_available_block(&mut self) -> Result<Option<usize>,DYNERR> {
        for block in fat::FIRST_DATA_CLUSTER as usize..self.boot_sector.cluster_count_usable() as usize {
            if self.is_block_free(block)? {
                return Ok(Some(block));
            }
        }
        return Ok(None);
    }
    /// Format a disk with the FAT file system, by this point the boot sector is presumed to be buffered,
    /// and must at least contain a valid BPB foundation.  If there is a BPB tail it is overwritten.
    pub fn format(&mut self, vol_name: &str, time: Option<chrono::NaiveDateTime>) -> STDRESULT {
        if !pack::is_label_valid(vol_name) && vol_name.len()>0 {
            error!("FAT volume name invalid");
            return Err(Box::new(Error::Syntax));
        }
        let block_size = self.boot_sector.block_size() as usize;
        let sec_size = self.boot_sector.sec_size() as usize;
        let media = self.boot_sector.media_byte() as u32;
        // Start with 0 in reserved/FAT/root, f6 in data region
        // n.b. blocks can only access the data region.
        trace!("fill sectors with 0x00 and 0xf6");
        trace!("disk has {} tracks {} sectors",self.img.track_count(),self.boot_sector.secs_per_track());
        let zeroes: Vec<u8> = vec![0;sec_size];
        let f6: Vec<u8> = vec![0xf6;sec_size];
        for lsec in 0..self.boot_sector.tot_sec() as usize {
            let [c,h,s] = self.get_chs(&Ptr::LogicalSector(lsec))?;
            if lsec < self.boot_sector.first_data_sec() as usize {
                self.img.write_sector(c,h,s,&zeroes)?;
            } else {
                self.img.write_sector(c,h,s,&f6)?;
            }
        }
        // Create a BPB tail
        let boot_label: [u8;11] = match vol_name.len()>0 {
            true => {
                let (nm,x) = pack::string_to_label_name(vol_name);
                [nm.to_vec(),x.to_vec()].concat().try_into().expect("label mismatch")
            }
            false => *b"NO NAME    "
        };
        // Using nanos gives us about 30 bits of resolution.
        // This avoids issues with the FAT datestamp after the year 2107.
        let id = u32::to_le_bytes(chrono::Local::now().naive_local().and_utc().timestamp_subsec_nanos());
        self.boot_sector.create_tail(0x00, id, boot_label);
        // write the boot sector (perhaps rewriting).
        // may be written again if FAT32.
        trace!("write boot sector");
        self.img.write_sector(0,0,1,&self.boot_sector.to_bytes())?;

        // FAT entry one is the media type
        trace!("setup the first FAT");
        let (typ,buf) = self.get_fat_buffer()?;
        fat::set_cluster(0, media + 0xf00, typ, buf);
        // FAT entry two is EOC
        fat::mark_last(1, typ, buf);

        // Create root directory appropriate for FAT type.
        // For FAT32 we expect the first root directory cluster to be in the BPB already.
        match typ {
            32 => {
                trace!("create FAT32 root directory");
                let root_cluster = self.boot_sector.root_dir_cluster1() as usize;
                let mut dir = Directory::new();
                dir.expand(block_size/directory::DIR_ENTRY_SIZE);
                self.write_block(&vec![0;block_size], 0, root_cluster,0)?;
                if vol_name.len()>0 {
                    let mut label = Entry::create_label(vol_name,time);
                    label.set_attr(directory::VOLUME_ID | directory::ARCHIVE);
                    let mut loc = EntryLocation {
                        cluster1: Some(Ptr::Cluster(root_cluster)),
                        entry: Ptr::Entry(0),
                        dir
                    };
                    self.writeback_directory_entry(&mut loc,&label)?;
                }
            },
            _ => {
                trace!("create FAT{} root directory",typ);
                // unless there is a label, there is actually nothing to do
                if vol_name.len()>0 {
                    let mut dir = Directory::new();
                    dir.expand(self.boot_sector.root_dir_entries() as usize);
                    let mut label = Entry::create_label(vol_name,time);
                    label.set_attr(directory::VOLUME_ID | directory::ARCHIVE);
                    let mut loc = EntryLocation {
                        cluster1: None,
                        entry: Ptr::Entry(0),
                        dir
                    };
                    self.writeback_directory_entry(&mut loc,&label)?;
                }
            }
        }
        trace!("write FAT buffer to all FAT sectors");
        self.writeback_fat_buffer()
    }
    /// given any cluster, return the next cluster, or None if this is the last one.
    /// return error if the current cluster is damaged or out of range.
    fn next_cluster(&mut self,curr: &Ptr) -> Result<Option<Ptr>,DYNERR> {
        let n = *match curr {
            Ptr::Cluster(n) => n,
            _ => panic!("wrong pointer type")
        };
        if !self.clus_in_rng(n) {
            error!("cluster {} out of range",n);
            return Err(Box::new(Error::BadFAT));
        }
        let (typ,fat_buf) = self.get_fat_buffer()?;
        if fat::is_damaged(n, typ, fat_buf) {
            error!("damaged cluster in chain");
            return Err(Box::new(Error::BadFAT))
        }
        match fat::is_last(n,typ,fat_buf) {
            true => Ok(None),
            false => Ok(Some(Ptr::Cluster(fat::get_cluster(n, typ, &fat_buf) as usize)))
        }
    }
    /// given any cluster, return the last cluster in its chain.
    fn last_cluster(&mut self,initial: &Ptr) -> Result<Ptr,DYNERR> {
        let mut curr = *initial;
        let max_clusters = self.boot_sector.cluster_count_usable() as usize;
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
        match initial.unwrap() {
            0 => return Ok(ans), // empty chain
            c => {
                if !self.clus_in_rng(c) {
                    return Err(Box::new(Error::FirstClusterInvalid));
                }
            }
        }
        let mut curr = *initial;
        let max_clusters = self.boot_sector.cluster_count_usable() as usize;
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
    /// given an initial cluster, follow the chain to the end and return number of clusters.
    fn get_cluster_chain_length(&mut self,initial: &Ptr) -> Result<usize,DYNERR> {
        let mut ans: usize = 0;
        match initial.unwrap() as u32 {
            0 => return Ok(ans), // empty chain
            c => {
                if !self.clus_in_rng(c as usize) {
                    return Err(Box::new(Error::FirstClusterInvalid));
                }
            }
        }
        let mut curr = *initial;
        let max_clusters = self.boot_sector.cluster_count_usable() as usize;
        for _i in 0..max_clusters {
            ans += 1;
            curr = match self.next_cluster(&curr)? {
                None => return Ok(ans),
                Some(next) => next
            };
        }
        Err(Box::new(Error::BadFAT))
    }
    /// given an initial cluster, follow the chain to free all clusters in the chain.
    fn deallocate_cluster_chain_data(&mut self,initial: &Ptr) -> STDRESULT {
        match initial.unwrap() {
            0 => return Ok(()),
            c => {
                if !self.clus_in_rng(c) {
                    return Err(Box::new(Error::FirstClusterInvalid));
                }
            }
        }
        let mut curr = *initial;
        let max_clusters = self.boot_sector.cluster_count_usable() as usize;
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
                Directory::from_bytes(&buf)?
            },
            _ => {
                let mut buf = Vec::new();
                let sec_rng = self.boot_sector.root_dir_sec_rng();
                debug!("get FAT{} root at logical sector {}",self.typ,sec_rng[0]);
                for lsec in (sec_rng[0] as usize)..(sec_rng[1] as usize) {
                    let [cyl,head,sec] = self.get_chs(&Ptr::LogicalSector(lsec))?;
                    buf.append(&mut self.img.read_sector(cyl, head, sec)?);
                }
                Directory::from_bytes(&buf)?
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
                Ok(Directory::from_bytes(&dir_buf)?)
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
    fn writeback_directory_entry(&mut self,loc: &mut EntryLocation,entry: &Entry) -> STDRESULT {
        let entries_per_cluster = self.boot_sector.block_size() as usize / directory::DIR_ENTRY_SIZE;
        let entries_per_sector = self.boot_sector.sec_size() as usize / directory::DIR_ENTRY_SIZE;
        loc.dir.set_entry(&loc.entry, entry);
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
                let [cyl,head,sec] = self.get_chs(&Ptr::LogicalSector(lsec))?;
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
    /// Put path as [subdir,subdir,...,last] where last could be an empty string,
    /// which indicates this is a directory (if path is "/" we get [""]).
    /// If last is not empty, it could be either directory or file.
    /// Path is always absolute, starting slash is optional.
    /// Also check that the path is not too long.
    fn normalize_path(&mut self,path: &str) -> Result<Vec<String>,DYNERR> {
        let mut normalized = path.to_string();
        if normalized.len()==0 {
            normalized = "/".to_string();
        }
        if &normalized[0..1]!="/" {
            normalized.insert(0,'/');
        }
        if normalized.len()>63 {
            error!("MS-DOS path too long {}",normalized.len());
            return Err(Box::new(Error::Syntax));
        }
        let path_nodes: Vec<String> = normalized.split("/").map(|s| s.to_string().to_uppercase()).collect();

        // check empty nodes
        for i in 1..path_nodes.len() {
            if path_nodes[i].len()==0 && i!=path_nodes.len()-1 {
                error!("empty path node not allowed");
                return Err(Box::new(Error::Syntax));
            }
        }
        return Ok(path_nodes[1..].to_vec());
    }
    /// split the path into the last node (file or directory) and its parent path
    fn split_path(&mut self,path: &str) -> Result<[String;2],DYNERR> {
        let mut path_nodes = self.normalize_path(path)?;
        if path_nodes.len()==0 {
            error!("cannot resolve path {}",path);
            return Err(Box::new(Error::FileNotFound));
        }
        // if last node is empty, remove it (means we have a directory)
        if path_nodes[path_nodes.len()-1].len()==0 {
            path_nodes = path_nodes[0..path_nodes.len()-1].to_vec();
        }
        let name = path_nodes[path_nodes.len()-1].clone();
        path_nodes = path_nodes[0..path_nodes.len()-1].to_vec();
        let parent_path: String = path_nodes.iter().map(|s| "/".to_string() + s).collect::<Vec<String>>().concat();
        return Ok([parent_path,name]);
    }
    /// Goto the specified path and return tuple (maybe_parent,file).
    /// * if file is the root directory, maybe_parent==None
    /// * if the path contains a wildcard in the last node, the returned file contains the pattern, not the matches
    fn goto_path(&mut self,path: &str) -> Result<(Option<FileInfo>,FileInfo),DYNERR> {
        let (_vol_name,root) = self.get_root_dir()?;
        let mut parent_info: Option<FileInfo> = None;
        let root_info: FileInfo = FileInfo::create_root(match self.typ {
            12 | 16 => 0,
            32 => self.boot_sector.root_dir_cluster1() as usize,
            _ => panic!("unexpected FAT type")
        });
        let path_nodes = self.normalize_path(path)?;
        debug!("search path {:?}",path_nodes);
        // path_nodes = [dir,dir,...,dir|file|empty]
        let n = path_nodes.len();
        // if this is the root directory, return special FileInfo
        if n==1 && path_nodes[0]=="" {
            debug!("goto root directory");
            return Ok((parent_info,root_info));
        }
        // walk the tree
        let mut files = root.build_files()?;
        parent_info = Some(root_info);
        for level in 0..n {
            debug!("searching level {}: {}",level,parent_info.clone().unwrap().name);
            let subdir = path_nodes[level].clone();
            let terminus = level+1 == n;
            let null_terminus = level+2 == n && path_nodes[n-1] == "";
            let wildcard_terminus = level+1 == n && (subdir.contains("*") || subdir.contains("?"));
            if wildcard_terminus {
                return Ok((parent_info,FileInfo::create_wildcard(&subdir)));
            }
            let curr = match directory::get_file(&subdir, &files) {
                Some(finfo) => finfo.clone(),
                None => return Err(Box::new(Error::FileNotFound))
            };
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
        let mut fimg = new_fimg(self.boot_sector.block_size() as usize,false,"temp")?;
        // TODO: eliminate redundancy, by this time the directory has already been read at least once
        let dir = self.get_directory(&parent.cluster1)?;
        let entry = dir.get_entry(&Ptr::Entry(finfo.idx));
        let all_data = self.get_cluster_chain_data(&finfo.cluster1.unwrap())?;
        fimg.desequence(&all_data);
        entry.metadata_to_fimg(&mut fimg); // must come after desequence or eof is spoiled
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
                return match directory::get_file(new_name, &files) {
                    Some(_) => Err(Box::new(Error::DuplicateFile)),
                    None => Ok(EntryLocation { cluster1: parent.cluster1, entry: Ptr::Entry(file_info.idx), dir: search_dir })
                };
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
        debug!("write {} to {}",new_name,parent_path);
        if let Ok((_maybe_grandparent,parent)) = self.goto_path(&parent_path) {
            let mut search_dir = self.get_directory(&parent.cluster1)?;
            let files = search_dir.build_files()?;
            return match directory::get_file(&new_name, &files) {
                Some(_) => Err(Box::new(Error::DuplicateFile)),
                None => match self.get_available_entry(&mut search_dir, &parent.cluster1) {
                    Ok(ptr) => Ok((new_name,EntryLocation { cluster1: parent.cluster1, entry: ptr, dir: search_dir})),
                    Err(e) => Err(e)
                } 
            }
        }
        return Err(Box::new(Error::FileNotFound));
    }
    /// Write any file.  Use `FileImage::desequence` to load sequential data into the file image.
    /// The entry must already exist, but cluster1 pointer will be set herein.
    /// Also writes back changes to the directory (but FAT remains in buffer).
    fn write_file(&mut self,loc: &mut EntryLocation,fimg: &super::FileImage) -> Result<usize,DYNERR> {
        let mut entry = loc.dir.get_entry(&loc.entry);
        if self.num_free_blocks()? < fimg.end() {
            return Err(Box::new(Error::DiskFull));
        }
        let mut prev: usize = 0;
        for count in 0..fimg.end() {
            if let Some(data) = fimg.chunks.get(&count) {
                if let Some(curr) = self.get_available_block()? {
                    self.write_block(data, prev, curr, 0)?;
                    if count==0 {
                        entry.set_cluster(curr);
                    }
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
        self.writeback_directory_entry(loc,&entry)?;
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
        self.writeback_directory_entry(loc,&entry)
    }
    /// Output FAT directory as a vector of paths that match a glob, calls itself recursively
    fn glob_node(&mut self,pattern: &str,dir: &directory::Directory,case_sensitive: bool) -> Result<Vec<String>,DYNERR> {
        // this blindly searches everywhere, we could be more efficient by truncating based on the pattern
        let mut files = Vec::new();
        let glob = match case_sensitive {
            true => globset::GlobBuilder::new(&pattern).literal_separator(true).build()?.compile_matcher(),
            false => globset::GlobBuilder::new(&pattern.to_uppercase()).literal_separator(true).build()?.compile_matcher()
        };
        if let Ok(sorted) = dir.build_files() {
            for finfo in sorted.values() {
                if finfo.volume_id {
                    continue;
                }
                if finfo.directory && (finfo.name=="." || finfo.name=="..") {
                    continue;
                }
                let key = match finfo.typ.len() {
                    0 => finfo.name.clone(),
                    _ => [finfo.name.clone(),".".to_string(),finfo.typ.clone()].concat()
                };
                let test = match case_sensitive {
                    true => [self.curr_path.concat(),key.clone()].concat(),
                    false => [self.curr_path.concat(),key.clone()].concat().to_uppercase(),
                };
                if !finfo.directory && glob.is_match(&test) {
                    let mut full_path = self.curr_path.concat();
                    full_path += &key;
                    files.push(full_path);
                }
                if finfo.directory && finfo.name!="." && finfo.name!=".." {
                    if let Some(ptr) = finfo.cluster1 {
                        trace!("descend into directory {}",key);
                        let subdir = self.get_directory(&Some(ptr))?;
                        self.curr_path.push(key + "/");
                        files.append(&mut self.glob_node(pattern,&subdir,case_sensitive)?);
                    }
                }
            }
        }
        self.curr_path.pop();
        Ok(files)
    }
    /// Output FAT directory as a JSON object, calls itself recursively
    fn tree_node(&mut self,dir: &directory::Directory,include_meta: bool) -> Result<json::JsonValue,DYNERR> {
        const DATE_FMT: &str = "%Y/%m/%d";
        const TIME_FMT: &str = "%H:%M";
        let mut files = json::JsonValue::new_object();
        if let Ok(sorted) = dir.build_files() {
            for finfo in sorted.values() {
                if finfo.volume_id {
                    continue;
                }
                if finfo.directory && (finfo.name=="." || finfo.name=="..") {
                    continue;
                }
                let key = match finfo.typ.len() {
                    0 => finfo.name.clone(),
                    _ => [finfo.name.clone(),".".to_string(),finfo.typ.clone()].concat()
                };
                files[&key] = json::JsonValue::new_object();
                if finfo.directory && finfo.name!="." && finfo.name!=".." {
                    if let Some(ptr) = finfo.cluster1 {
                        trace!("descend into directory {}",key);
                        let subdir = self.get_directory(&Some(ptr))?;
                        files[&key]["files"] = self.tree_node(&subdir,include_meta)?;
                    }
                }
                if include_meta {
                    files[&key]["meta"] = json::JsonValue::new_object();
                    let meta = &mut files[&key]["meta"];
                    if finfo.directory {
                        meta["type"] = json::JsonValue::String("DIR".to_string());
                    } else {
                        meta["type"] = json::JsonValue::String(finfo.typ.clone());
                    }
                    meta["eof"] = json::JsonValue::Number(finfo.eof.into());
                    let created = match (finfo.create_date,finfo.create_time) {
                        (Some(d),Some(t)) => [
                            d.format(DATE_FMT).to_string(),
                            " ".to_string(),
                            t.format(TIME_FMT).to_string()
                        ].concat(),
                        _ => "".to_string()
                    };
                    let accessed = match finfo.access_date {
                        Some(d) => d.format(DATE_FMT).to_string(),
                        None => "".to_string()
                    };
                    let modified = match (finfo.write_date,finfo.write_time) {
                        (Some(d),Some(t)) => [
                            d.format(DATE_FMT).to_string(),
                            " ".to_string(),
                            t.format(TIME_FMT).to_string()
                        ].concat(),
                        _ => "".to_string()
                    };
                    if created.len()>0 {
                        meta["time_created"] = json::JsonValue::String(created);
                    }
                    if accessed.len()>0 {
                        meta["time_accessed"] = json::JsonValue::String(accessed);
                    }
                    if modified.len()>0 {
                        meta["time_modified"] = json::JsonValue::String(modified);
                    }
                    meta["read_only"] = json::JsonValue::Boolean(finfo.read_only);
                    meta["system"] = json::JsonValue::Boolean(finfo.system);
                    meta["hidden"] = json::JsonValue::Boolean(finfo.system);
                    meta["archived"] = json::JsonValue::Boolean(finfo.archived);
                    if let Some(cluster1) = finfo.cluster1 {
                        let blocks = self.get_cluster_chain_length(&cluster1)?;
                        meta["blocks"] = json::JsonValue::Number(blocks.into());
                    }
                }
            }
        }
        Ok(files)
    }
}

impl super::DiskFS for Disk {
    fn new_fimg(&self, chunk_len: Option<usize>,set_time: bool,path: &str) -> Result<super::FileImage,DYNERR> {
        match chunk_len {
            Some(l) => new_fimg(l,set_time,path),
            None => new_fimg(self.boot_sector.block_size() as usize,set_time,path)
        }
    }
    fn stat(&mut self) -> Result<super::Stat,DYNERR> {
        let (vol_lab,_) = self.get_root_dir()?;
        Ok(super::Stat {
            fs_name: FS_NAME.to_string(),
            label: vol_lab,
            users: Vec::new(),
            block_size: self.boot_sector.block_size() as usize,
            block_beg: 2,
            block_end: 2 + self.boot_sector.cluster_count_usable() as usize,
            free_blocks: self.num_free_blocks()?,
            raw: self.boot_sector.to_json(None)
        })
    }
    fn catalog_to_stdout(&mut self, path_and_options: &str) -> STDRESULT {
        let items: Vec<&str> = path_and_options.split_whitespace().collect();
        let (path,opt) = match items.len() {
            1 => (items[0],""),
            2 => (items[0],items[1]),
            _ => return Err(Box::new(Error::Syntax))
        };
        let (dir_info,pattern) = match self.goto_path(path)? {
            (Some(parent),dir) => {
                match (dir.wildcard.len(),dir.directory) {
                    (0,true) => (dir,"".to_string()),
                    (0,false) => (parent,[dir.name,".".to_string(),dir.typ].concat()),
                    _ => (parent,dir.wildcard)
                }
            },
            (None,dir) => {
                match (dir.wildcard.len(),dir.directory) {
                    (0,true) => (dir,"".to_string()),
                    _ => return Err(Box::new(Error::Syntax)),
                }
            }
        };
        let dir = self.get_directory(&dir_info.cluster1)?;
        let (vol_lab,_) = self.get_root_dir()?;
        let free = self.num_free_blocks()? as u64 * self.boot_sector.sec_size() * self.boot_sector.secs_per_clus() as u64;
        match opt.to_lowercase().as_str() {
            "" => display::dir(&path,&vol_lab,&dir,&pattern,false,free),
            "/w" => display::dir(&path,&vol_lab,&dir,&pattern,true,free),
            _ => Err(Box::new(Error::InvalidSwitch))
        }
    }
    fn catalog_to_vec(&mut self, path: &str) -> Result<Vec<String>,DYNERR> {
        match self.goto_path(path) {
            Ok((_,dir_info)) => {
                if dir_info.directory {
                    let dir = self.get_directory(&dir_info.cluster1)?;
                    let mut ans = Vec::new();
                    for i in 0..dir.num_entries() {
                        let entry_type = dir.get_type(&Ptr::Entry(i));
                        if entry_type==EntryType::FreeAndNoMore {
                            break;
                        }
                        if entry_type==EntryType::VolumeLabel || entry_type==EntryType::Free {
                            continue;
                        }
                        let entry = dir.get_entry(&Ptr::Entry(i));
                        let name_and_ext = entry.name(false);
                        let mut split = name_and_ext.split(".").collect::<Vec<&str>>();
                        if split.len()<2 {
                            split.push("");
                        }
                        let (typ,name) = match (entry_type,split[1].len()) {
                            (EntryType::Directory,0) => ("DIR",split[0].to_string()),
                            (EntryType::Directory,_) => ("DIR",name_and_ext),
                            _ => (split[1],split[0].to_string())
                        };
                        let blocks = 1 + (entry.eof() as i64 - 1) / self.boot_sector.block_size() as i64;
                        ans.push(super::universal_row(typ,blocks as usize,&name));
                    }
                    return Ok(ans);
                } else {
                    return Err(Box::new(Error::FileNotFound));
                }
            },
            Err(e) => Err(e)
        }
    }
    fn glob(&mut self,pattern: &str,case_sensitive: bool) -> Result<Vec<String>,DYNERR> {
        let (_,dir) = self.get_root_dir()?;
        self.curr_path = vec!["/".to_string()];
        self.glob_node(pattern, &dir,case_sensitive)
    }
    fn tree(&mut self,include_meta: bool,indent: Option<u16>) -> Result<String,DYNERR> {
        let (vol,dir) = self.get_root_dir()?;
        let mut tree = json::JsonValue::new_object();
        tree["file_system"] = json::JsonValue::String(FS_NAME.to_string());
        tree["files"] = self.tree_node(&dir,include_meta)?;
        tree["label"] = json::JsonValue::new_object();
        tree["label"]["name"] = json::JsonValue::String(vol);
        if let Some(spaces) = indent {
            Ok(json::stringify_pretty(tree,spaces))
        } else {
            Ok(json::stringify(tree))
        }
    }
    fn create(&mut self,path: &str) -> STDRESULT {
        let (name,mut loc) = self.prepare_to_write(path)?;
        if let Some(new_cluster) = self.get_available_block()? {
            let parent_cluster = match loc.cluster1 {
                Some(c) => c.unwrap(),
                None => 0 // this holds even for FAT32
            };
            let (entry, dir_data) = Entry::create_subdir(&name,parent_cluster,new_cluster,self.boot_sector.block_size() as usize,None);
            self.write_block(&dir_data, 0, new_cluster, 0)?;
            self.writeback_directory_entry(&mut loc,&entry)    
        } else {
            Err(Box::new(Error::DiskFull))
        }
    }
    fn delete(&mut self,path: &str) -> STDRESULT {
        let (maybe_parent,finfo) = self.goto_path(path)?;
        if finfo.wildcard.len()>0 {
            error!("wildcards are not allowed here");
            return Err(Box::new(Error::Syntax));
        }
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
                self.writeback_directory_entry(&mut EntryLocation {
                    cluster1: parent.cluster1,
                    entry: entry_ptr,
                    dir
                }, &entry)?;
                debug!("erased entry {:?}",entry.to_bytes());
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
    fn read_block(&mut self,num: &str) -> Result<Vec<u8>,DYNERR> {
        match usize::from_str(num) {
            Ok(block) => {
                let mut buf: Vec<u8> = vec![0;self.boot_sector.block_size() as usize];
                if block < 2 || block >= 2 + self.boot_sector.cluster_count_usable() as usize {
                    return Err(Box::new(Error::SectorNotFound));
                }
                self.read_block(&mut buf,block,0)?;
                Ok(buf)
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn write_block(&mut self, num: &str, dat: &[u8]) -> Result<usize,DYNERR> {
        match usize::from_str(num) {
            Ok(block) => {
                if block < 2 || block >= 2 + self.boot_sector.cluster_count_usable() as usize {
                    return Err(Box::new(Error::SectorNotFound));
                }
                if dat.len() > self.boot_sector.block_size() as usize {
                    return Err(Box::new(Error::WriteFault));
                }
                self.zap_block(dat,block,0)?;
                Ok(dat.len())
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn get(&mut self,path: &str) -> Result<super::FileImage,DYNERR> {
        let (maybe_parent,finfo) = self.goto_path(path)?;
        if let Some(parent) = maybe_parent {
            let mut fimg = self.read_file(&parent,&finfo)?;
            fimg.full_path = path.to_string();
            return Ok(fimg);
        }
        error!("cannot read root as a file");
        Err(Box::new(Error::ReadFault))
    }
    fn put(&mut self,fimg: &super::FileImage) -> Result<usize,DYNERR> {
        if fimg.file_system!=FS_NAME {
            error!("cannot write {} file image to FAT",fimg.file_system);
            return Err(Box::new(Error::WriteFault));
        }
        if fimg.chunk_len!=self.boot_sector.block_size() as usize {
            error!("chunk length {} is incompatible with this file image",fimg.chunk_len);
            return Err(Box::new(Error::IncorrectDOS));
        }
        match self.prepare_to_write(&fimg.full_path) {
            Ok((name,mut loc)) => {
                // create the entry
                let mut entry = Entry::create(&name,None);
                entry.fimg_to_metadata(fimg,true)?;
                debug!("create entry {:?}",entry.to_bytes());
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
    fn standardize(&mut self,ref_con: u16) -> HashMap<Block,Vec<usize>> {
        // We have to ignore timestamps and the unused bytes in the last cluster of any chain.
        // Initial ref_con should be 0.  Not meant for FAT32.
        let mut ans: HashMap<Block,Vec<usize>> = HashMap::new();
        let cluster1 = match ref_con {
            0 => None,
            _ => Some(Ptr::Cluster(ref_con as usize))
        };
        let dir = self.get_directory(&cluster1).expect("disk error");
        let files = dir.build_files().expect("could not build files");
        for finfo in files.values() {
            // recursion into subdirectory
            if finfo.directory && finfo.name!="." && finfo.name!=".." {
                let sub_map = self.standardize(finfo.cluster1.unwrap().unwrap() as u16);
                super::combine_ignorable_offsets(&mut ans, sub_map);
            }
            // ignore timestamps
            if ref_con>0 {
                let links = finfo.idx*32/self.boot_sector.block_size() as usize;
                let offset = finfo.idx*32%self.boot_sector.block_size() as usize;
                let mut curr = Ptr::Cluster(cluster1.unwrap().unwrap());
                for _i in 0..links {
                    curr = self.next_cluster(&curr).expect("could not link cluster").unwrap();
                }
                let offsets = Entry::standardize(offset);
                super::add_ignorable_offsets(&mut ans,self.export_clus(curr.unwrap()),offsets);
            }
            // ignore trailing bytes in the last cluster
            if let Some(c1) = finfo.cluster1 {
                if !finfo.directory && !finfo.volume_id {
                    let block_size = self.boot_sector.block_size() as usize;
                    let last = self.last_cluster(&c1).expect("could not link cluster");
                    let ignore1 = finfo.eof % block_size;
                    let offsets = (ignore1..block_size).collect();
                    super::add_ignorable_offsets(&mut ans, self.export_clus(last.unwrap()), offsets);
                }
            }
        }
        return ans;
    }
    fn compare(&mut self,path: &std::path::Path,ignore: &HashMap<Block,Vec<usize>>) {
        // Not meant for FAT32
        self.writeback_fat_buffer().expect("disk error");
        let mut emulator_disk = crate::create_fs_from_file(&path.to_str().unwrap()).expect("read error");
        // compare the FATs
        for lsec in self.boot_sector.res_secs() as usize..self.boot_sector.root_dir_sec_rng()[0] as usize {
            let [c,h,s] = self.get_chs(&Ptr::LogicalSector(lsec)).expect("bad sector access");
            let actual = self.img.read_sector(c,h,s).expect("bad sector access");
            let expected = emulator_disk.get_img().read_sector(c, h, s).expect("bad sector access");
            assert_eq!(actual,expected," at sector {}",lsec)
        }
        // compare root directory
        let [beg,end] = self.boot_sector.root_dir_sec_rng();
        for lsec in beg..end {
            let [c,h,s] = self.get_chs(&Ptr::LogicalSector(lsec as usize)).expect("bad sector access");
            let mut actual = self.img.read_sector(c,h,s).expect("bad sector access");
            let mut expected = emulator_disk.get_img().read_sector(c, h, s).expect("bad sector access");
            let offsets = Entry::standardize(0);
            for i in 0..self.boot_sector.sec_size() as usize {
                let rel_offset = i%32;
                if offsets.contains(&rel_offset) {
                    actual[i] = 0;
                    expected[i] = 0;
                }
            }
            assert_eq!(actual,expected," at sector {}",lsec)
        }
        // compare clusters in the data region
        for block in 2..2+self.boot_sector.cluster_count_usable() {
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
        self.writeback_fat_buffer().expect("could not write back FAT buffer");
        &mut self.img
    }
}

#[test]
fn test_path_normalize() {
    let kind = img::DiskKind::D525(img::names::IBM_SSDD_9);
    let img = Box::new(crate::img::imd::Imd::create(kind));
    let boot_sec = bpb::BootSector::create(&kind).expect("can't create boot sector");
    let mut disk = Disk::from_img(img,Some(boot_sec)).expect("bad disk");
    disk.format(&String::from("DISK 1"),None).expect("disk error");
    match disk.normalize_path("/") {
        Ok(res) => assert_eq!(res,[""]),
        Err(e) => panic!("{}",e)
    }
    match disk.normalize_path("dir1/") {
        Ok(res) => assert_eq!(res,["DIR1",""]),
        Err(e) => panic!("{}",e)
    }
    match disk.normalize_path("dir1/sub2") {
        Ok(res) => assert_eq!(res,["DIR1","SUB2"]),
        Err(e) => panic!("{}",e)
    }
    match disk.normalize_path("/dir1/sub2/sub3") {
        Ok(res) => assert_eq!(res,["DIR1","SUB2","SUB3"]),
        Err(e) => panic!("{}",e)
    }
    match disk.normalize_path("abcdefghijklmno/abcdefghijklmno/abcdefghijklmno/abcdefghijklmno/abcdefghijklmno/abcdefghijklmno/abcdefghijklmno/abcdefghijklmno") {
        Ok(_res) => panic!("normalize_path should have failed with path too long"),
        Err(e) => assert_eq!(e.to_string(),"syntax")
    }
}

