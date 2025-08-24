use crate::bios::Error;
use crate::img::{Track,Sector};

/// Take a baseline track-sector list and produce an abstract one accounting for `sec_shift` and `heads`.
/// This assumes the mapping track = cyl*heads + head.
pub fn std_blocking(ts_list: Vec<[usize;2]>,sec_shift: u8,heads: usize) -> Result<Vec<(Track,Sector)>,Error> {
    log::trace!("ts list {:?} (logical deblocked)",ts_list);
    if (ts_list.len() % (1 << sec_shift) != 0) || ((ts_list[0][1]-1) % (1 << sec_shift) != 0) {
        log::info!("CP/M blocking was misaligned, start {}, length {}",ts_list[0][1],ts_list.len());
        return Err(Error::SectorAccess);
    }
    if heads<1 {
        log::error!("CP/M blocking was passed 0 heads");
        return Err(Error::SectorAccess);
    }
    let mut ans: Vec<[usize;3]> = Vec::new();
    let mut track = 0;
    for i in 0..ts_list.len() {
        let lsec = ts_list[i][1];
        if (lsec-1)%(1<<sec_shift) == 0 {
            track = ts_list[i][0];
        }
        if lsec%(1<<sec_shift) == 0 {
            let cyl = track/heads;
            let head = match heads { 1 => 0, _ => track%heads };
            ans.push([cyl,head,1+(lsec-1)/(1<<sec_shift)]);
        } else if ts_list[i][0]!=track {
            log::info!("CP/M blocking failed, sector crossed track {}",track);
            return Err(Error::SectorAccess);
        }
    }
    log::trace!("ts list {:?} (logical blocked)",ans);
    Ok(ans.iter().map(|[c,h,s]| (Track::CH((*c,*h)),Sector::Num(*s))).collect::<Vec<(Track,Sector)>>())
}
