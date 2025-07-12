use crate::img;
use crate::DYNERR;
use std::str::FromStr;
use bit_vec::BitVec;
use super::{SectorMarker,ZoneFormat,DiskFormat};

// fn parse_isize(obj: &json::JsonValue,name: &str) -> Result<isize, DYNERR> {
//     if let Some(val) = obj.as_isize() {
//         return Ok(val);
//     }
//     log::error!("{} should be a number",name);
//     return Err(Box::new(img::Error::MetadataMismatch));
// }
fn parse_usize(obj: &json::JsonValue,name: &str) -> Result<usize, DYNERR> {
    if let Some(val) = obj.as_usize() {
        return Ok(val);
    }
    log::error!("{} should be a number",name);
    return Err(Box::new(img::Error::MetadataMismatch));
}
fn parse_str(obj: &json::JsonValue,name: &str) -> Result<String, DYNERR> {
    if let Some(s) = obj.as_str() {
        return Ok(s.to_string());
    }
    log::error!("{} should be a string",name);
    return Err(Box::new(img::Error::MetadataMismatch));
}
fn parse_str_list(obj: &json::JsonValue,name: &str) -> Result<Vec<String>,DYNERR> {
    if !obj.is_array() {
        log::error!("{} not an array or missing",name);
        return Err(Box::new(img::Error::MetadataMismatch));
    }
    let mut ans = Vec::new();
    for num in obj.members() {
        match num.as_str() {
            Some(s) => ans.push(s.to_string()),
            None => {
                log::error!("{} should contain strings",name);
                return Err(Box::new(img::Error::MetadataMismatch));
            }
        }
    }
    Ok(ans)
}
fn parse_bin(obj: &json::JsonValue,name: &str) -> Result<BitVec,DYNERR> {
    match obj.as_str() {
        Some(s) => {
            let mut ans = BitVec::new();
            for c in s.chars() {
                ans.push(match c {
                    '0' => false,
                    '1' => true,
                    _ => return Err(Box::new(img::Error::MetadataMismatch))
                });
            }
            Ok(ans)
        },
        None => {
            log::error!("{} should be a binary string",name);
            Err(Box::new(img::Error::MetadataMismatch))
        }
    }
}
fn parse_hex(obj: &json::JsonValue,name: &str) -> Result<Vec<u8>,DYNERR> {
    match obj.as_str() {
        Some(s) => Ok(hex::decode(&s)?),
        None => {
            log::error!("{} should be a hex string",name);
            return Err(Box::new(img::Error::MetadataMismatch));
        }
    }
}
fn parse_hex_list(obj: &json::JsonValue,name: &str) -> Result<Vec<Vec<u8>>,DYNERR> {
    if !obj.is_array() {
        log::error!("{} not an array or missing",name);
        return Err(Box::new(img::Error::MetadataMismatch));
    }
    let mut ans = Vec::new();
    for num in obj.members() {
        ans.push(parse_hex(num,&[name," list element"].concat())?);
    }
    Ok(ans)
}
/// expects list with each element either a number, or a sublist [number,repeat].
// fn parse_isize_list(obj: &json::JsonValue,name: &str) -> Result<Vec<isize>,DYNERR> {
//     if !obj.is_array() {
//         log::error!("{} not an array or missing",name);
//         return Err(Box::new(img::Error::MetadataMismatch));
//     }
//     let mut ans = Vec::new();
//     for run in obj.members() {
//         if run.is_array() {
//             if run.members().count() != 2 {
//                 log::error!("inner list should be [val,reps]");
//                 return Err(Box::new(img::Error::MetadataMismatch));
//             }
//             let mut iter = run.members();
//             let val = parse_isize(iter.next().unwrap(),&[name," first value"].concat())?;
//             let reps = parse_usize(iter.next().unwrap(),&[name," second value"].concat())?;
//             assert!(reps < 1000, "too many reps");
//             ans.append(&mut vec![val;reps]);
//         } else {
//             let val = parse_isize(run,&[name," value"].concat())?;
//             ans.push(val);
//         }
//     }
//     Ok(ans)
// }
/// expects list with each element either a number, or a sublist [number,repeat].
fn parse_usize_list(obj: &json::JsonValue,name: &str) -> Result<Vec<usize>,DYNERR> {
    if !obj.is_array() {
        log::error!("{} not an array or missing",name);
        return Err(Box::new(img::Error::MetadataMismatch));
    }
    let mut ans = Vec::new();
    for run in obj.members() {
        if run.is_array() {
            if run.members().count() != 2 {
                log::error!("inner list should be [val,reps]");
                return Err(Box::new(img::Error::MetadataMismatch));
            }
            let mut iter = run.members();
            let val = parse_usize(iter.next().unwrap(),&[name," first value"].concat())?;
            let reps = parse_usize(iter.next().unwrap(),&[name," second value"].concat())?;
            assert!(reps < 1000, "too many reps");
            ans.append(&mut vec![val;reps]);
        } else {
            let val = parse_usize(run,&[name," value"].concat())?;
            ans.push(val);
        }
    }
    Ok(ans)
}
/// expects list with each element either a binary string, or a sublist [binary,repeat].
fn parse_sync_gap(obj: &json::JsonValue,name: &str) -> Result<BitVec,DYNERR> {
    if !obj.is_array() {
        log::error!("{} not an array or missing",name);
        return Err(Box::new(img::Error::MetadataMismatch));
    }
    let mut ans = BitVec::new();
    for run in obj.members() {
        if run.is_array() {
            if run.members().count() != 2 {
                log::error!("inner list should be [val,reps]");
                return Err(Box::new(img::Error::MetadataMismatch));
            }
            let mut iter = run.members();
            let bits = parse_bin(iter.next().unwrap(),&[name," first value"].concat())?;
            let reps = parse_usize(iter.next().unwrap(),&[name," second value"].concat())?;
            assert!(reps < 1000, "too many reps");
            for _ in 0..reps {
                ans.append(&mut bits.clone()); // need to clone because arg is consumed
            }
        } else {
            let mut bits = parse_bin(run,&[name," element"].concat())?;
            ans.append(&mut bits);
        }
    }
    Ok(ans)
}
/// expects list with each element a two-elements list of hex strings
fn parse_swap_nibs(obj: &json::JsonValue,name: &str) -> Result<Vec<[u8;2]>,DYNERR> {
    if !obj.is_array() {
        log::error!("{} not an array or missing",name);
        return Err(Box::new(img::Error::MetadataMismatch));
    }
    let mut ans = Vec::new();
    for pair in obj.members() {
        if pair.is_array() {
            if pair.members().count() != 2 {
                log::error!("inner list should be [hex_str,hex_str]");
                return Err(Box::new(img::Error::MetadataMismatch));
            }
            let mut iter = pair.members();
            let special = parse_hex(iter.next().unwrap(),&[name," first value"].concat())?;
            let normal = parse_hex(iter.next().unwrap(),&[name," second value"].concat())?;
            ans.push([special[0],normal[0]]);
        } else {
            log::error!("{} should be pairs of hex strings",name);
            return Err(Box::new(img::Error::MetadataMismatch));
        }
    }
    Ok(ans)
}

impl ZoneFormat {
    pub fn from_json(obj: &json::JsonValue) -> Result<Self, DYNERR> {
        let flux_code = img::FluxCode::from_str(&parse_str(&obj["flux_code"],"flux_code")?)?;
        let addr_nibs = img::FieldCode::from_str(&parse_str(&obj["addr_nibs"],"addr_nibs")?)?;
        let data_nibs = img::FieldCode::from_str(&parse_str(&obj["data_nibs"],"data_nibs")?)?;
        let motor_range = parse_usize_list(&obj["motor_range"],"motor_range")?;
        let heads = parse_usize_list(&obj["heads"],"heads")?;
        let capacity = parse_usize_list(&obj["capacity"],"capacity")?;
        let addr_fmt_expr = parse_str_list(&obj["addr_fmt_expr"],"addr_fmt_expr")?;
        let addr_seek_expr = parse_str_list(&obj["addr_seek_expr"],"addr_seek_expr")?;
        let data_expr = parse_str_list(&obj["data_expr"],"data_expr")?;
        let chs_extract_expr = parse_str_list(&obj["chs_extract_expr"],"chs_extract_expr")?;
        let keys = parse_hex_list(&obj["markers"],"markers")?;
        let masks = parse_hex_list(&obj["marker_masks"],"marker_masks")?;
        let sync_trk_beg = parse_sync_gap(&obj["sync_trk_beg"], "sync_trk_beg")?;
        let sync_sec_end = parse_sync_gap(&obj["sync_sec_end"], "sync_sec_end")?;
        let sync_dat_end = parse_sync_gap(&obj["sync_dat_end"], "sync_dat_end")?;
        let swap_nibs = parse_swap_nibs(&obj["swap_nibs"], "swap_nibs")?;
        if motor_range.len() != 3 {
            log::error!("expected [beg,end,step] for range");
            return Err(Box::new(img::Error::MetadataMismatch));
        }
        if motor_range[0] >= motor_range[1] {
            log::error!("expected beg < end in range");
            return Err(Box::new(img::Error::MetadataMismatch));
        }
        if motor_range[2] < 1 {
            log::error!("expected step > 0 in range");
            return Err(Box::new(img::Error::MetadataMismatch));
        }
        if keys.len()!=4 || masks.len()!=4 {
            log::error!("expected 4 markers and 4 masks");
            return Err(Box::new(img::Error::MetadataMismatch));
        }
        if capacity.len() < 1 {
            log::error!("capacity must have at least one element");
            return Err(Box::new(img::Error::MetadataMismatch));
        }
        let mut markers = Vec::new();
        for m in 0..4 {
            markers.push(SectorMarker {key: keys[m].clone(),mask: masks[m].clone()})
        }
        return Ok(Self {
            flux_code,
            addr_nibs,
            data_nibs,
            motor_start: motor_range[0],
            motor_end: motor_range[1],
            motor_step: motor_range[2],
            heads,
            addr_fmt_expr,
            addr_seek_expr,
            data_expr,
            markers: std::array::from_fn(|i| markers[i].clone()),
            gaps: [sync_trk_beg,sync_sec_end,sync_dat_end],
            chs_extract_expr,
            capacity,
            swap_nibs
        });
    }
}

impl<'a> DiskFormat {
    pub fn from_json(json_str: &str) -> Result<Self, DYNERR> {
        let obj = json::parse(json_str)?;
        let typ = parse_str(&obj["a2kit_type"],"a2kit_type")?;
        if &typ != "format" {
            log::error!("file id had the wrong value ({})",typ);
            return Err(Box::new(img::Error::MetadataMismatch));
        }
        let vers = parse_str(&obj["version"],"version")?;
        if vers.starts_with("0.") {
            log::warn!("format file major version is behind reader version");
        }
        if !vers.starts_with("1.") {
            log::warn!("format file major version is beyond reader version");
        }
        let zone_list = &obj["zones"];
        let mut zones = Vec::new();
        if !zone_list.is_array() {
            log::error!("zones are not an array or missing");
            return Err(Box::new(img::Error::MetadataMismatch));
        }
        for obj in zone_list.members() {
            let zone = ZoneFormat::from_json(obj)?;
            zones.push(zone);
        }
        Ok(DiskFormat { zones })
    }
}
