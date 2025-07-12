use clap;
use crate::STDRESULT;

pub fn stat(cmd: &clap::ArgMatches) -> STDRESULT {
    let maybe_img_path = cmd.get_one::<String>("dimg");
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file_or_stdin_pro(maybe_img_path, fmt.as_ref())?;
    let stats = disk.stat()?;
    println!("{}",stats.to_json(cmd.get_one::<u16>("indent").copied()));
    return Ok(());
}

pub fn catalog(cmd: &clap::ArgMatches) -> STDRESULT {
    let default_path = "/".to_string();
    let path_in_img = cmd.get_one::<String>("file").unwrap_or(&default_path);
    let maybe_img_path = cmd.get_one::<String>("dimg");
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file_or_stdin_pro(maybe_img_path, fmt.as_ref())?;
    return if cmd.get_flag("generic") {
        let rows = disk.catalog_to_vec(&path_in_img)?;
        for row in rows {
            println!("{}",row);
        }
        Ok(())
    } else {
        disk.catalog_to_stdout(&path_in_img)
    }
}

pub fn tree(cmd: &clap::ArgMatches) -> STDRESULT {
    let maybe_img_path = cmd.get_one::<String>("dimg");
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file_or_stdin_pro(maybe_img_path, fmt.as_ref())?;
    println!("{}",disk.tree(cmd.get_flag("meta"), cmd.get_one::<u16>("indent").copied())?);
    return Ok(());
}

pub fn glob(cmd: &clap::ArgMatches) -> STDRESULT {
    let maybe_img_path = cmd.get_one::<String>("dimg");
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file_or_stdin_pro(maybe_img_path, fmt.as_ref())?;
    let v = disk.glob(cmd.get_one::<String>("file").unwrap(),false)?;
    let mut obj = json::array![];
    for m in v {
        obj.push(m)?;
    }
    let s = match cmd.get_one::<u16>("indent") {
        Some(spaces) => json::stringify_pretty(obj, *spaces),
        None => json::stringify(obj)
    };
    println!("{}",s);
    return Ok(());    
}

pub fn geometry(cmd: &clap::ArgMatches) -> STDRESULT {
    let maybe_img_path = cmd.get_one::<String>("dimg");
    let mut disk = crate::create_img_from_file_or_stdin(maybe_img_path)?;
    if let Some(fmt) = super::get_fmt(cmd)? {
        disk.change_format(fmt)?;
    }
    println!("{}",disk.export_geometry(cmd.get_one::<u16>("indent").copied())?);
    return Ok(());    
}