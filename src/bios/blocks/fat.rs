
use crate::bios::Error;
use crate::img::{Track,Sector};

/// Take a baseline track-sector list and produce an abstract one accounting for `heads`.
/// This assumes the mapping track = cyl*heads + head.
pub fn std_blocking(ts_list: Vec<[usize;2]>,heads: usize) -> Result<Vec<(Track,Sector)>,Error> {
    log::trace!("ts list {:?} (logical deblocked)",ts_list);
    if heads<1 {
        log::error!("FAT blocking was passed 0 heads");
        return Err(Error::SectorAccess);
    }
    let mut ans: Vec<[usize;3]> = Vec::new();
    for i in 0..ts_list.len() {
        let cyl = ts_list[i][0]/heads;
        let head = match heads { 1 => 0, _ => ts_list[i][0]%heads };
        let lsec = ts_list[i][1];
        ans.push([cyl,head,1+lsec]);
    }
    Ok(ans.iter().map(|[c,h,s]| (Track::CH((*c,*h)),Sector::Num(*s))).collect::<Vec<(Track,Sector)>>())
}
