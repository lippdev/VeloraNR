use std::{
    collections::BTreeMap,
    ffi::c_void,
    fs,
    io::{BufRead, BufReader, Seek, SeekFrom, Write},
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

type Frame = (u32, u64, u64);
unsafe extern "C" {
    fn portal_measure(
        path: *const u16,
        sample: extern "C" fn(*mut c_void, u32, u64, u64),
        context: *mut c_void,
        lost: *mut u32,
    ) -> u32;
}
extern "C" fn collect(context: *mut c_void, pid: u32, chain: u64, time: u64) {
    // ProcessTrace invokes this synchronously, only on the worker that owns the Vec.
    unsafe {
        (*context.cast::<Vec<Frame>>()).push((pid, chain, time));
    }
}

fn write_report(dir: &Path, frames: &[Frame]) -> std::io::Result<()> {
    let mut prev = BTreeMap::new();
    let mut groups: BTreeMap<_, Vec<f64>> = BTreeMap::new();
    let mut rows = std::io::BufWriter::new(fs::File::create(dir.join("apresentacoes.csv"))?);
    writeln!(rows, "pid,window_hwnd,utc_unix_us,ms_between_presents")?;
    for &(pid, chain, stamp) in frames {
        let key = (pid, chain);
        let Some(last) = prev.insert(key, stamp) else {
            continue;
        };
        if stamp <= last {
            continue;
        }
        let ms = (stamp - last) as f64 / 10000.;
        let micros = stamp.saturating_sub(116444736000000000) / 10;
        writeln!(rows, "{pid},{chain:#x},{micros},{ms:.4}")?;
        groups.entry(key).or_default().push(ms);
    }
    rows.flush()?;
    let mut summary = fs::File::create(dir.join("resumo.csv"))?;
    writeln!(summary, "pid,window_hwnd,samples,p50_ms,p95_ms,max_ms")?;
    for ((pid, chain), mut v) in groups {
        v.sort_by(f64::total_cmp);
        let rank = |percent: usize| v[(v.len() * percent).div_ceil(100) - 1];
        writeln!(
            summary,
            "{pid},{chain:#x},{},{:.3},{:.3},{:.3}",
            v.len(),
            rank(50),
            rank(95),
            v.last().unwrap()
        )?;
    }
    Ok(())
}

pub fn capture(root: &Path, sidecar: Option<u32>) -> Result<(PathBuf, u32), String> {
    let mut stream = fs::File::open(root.join("stream.log")).map_err(|e| e.to_string())?;
    stream.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    let pid = std::process::id();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dir = root.join("diagnosticos").join(format!("{stamp}-{pid}"));
    let trace = root.join(format!("cadence-{stamp}-{pid}.etl"));
    let wide = trace
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut frames = Vec::new();
    let mut lost = 0;
    let code =
        unsafe { portal_measure(wide.as_ptr(), collect, (&raw mut frames).cast(), &mut lost) };
    let _ = fs::remove_file(trace);
    if code == 5 {
        return Err(
            "O Windows recusou acesso à medição ETW. O log da sessão continua disponível.".into(),
        );
    }
    if code != 0 {
        return Err(format!(
            "Medição de cadência interrompida (Windows {code})."
        ));
    }
    if frames.len() < 2 {
        return Err("Nenhuma sequência de apresentações observada.".into());
    }
    fs::create_dir_all(&dir)
        .and_then(|_| write_report(&dir, &frames))
        .map_err(|e| e.to_string())?;
    let mut timeline =
        format!("rendererPid={pid} initialSidecarPid={sidecar:?} eventsLost={lost}\n");
    for line in BufReader::new(stream).lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.contains("[session-ui]")
            || line.contains("[session-sample]")
            || line.contains("[player-timing]")
        {
            timeline.push_str(&line);
            timeline.push('\n');
        }
    }
    fs::write(dir.join("sessao.log"), timeline).map_err(|e| e.to_string())?;
    Ok((dir, lost))
}

#[cfg(test)]
mod tests {
    #[test]
    fn measures_each_window_independently() {
        let dir = std::env::temp_dir().join(format!("velora-interval-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        super::write_report(
            &dir,
            &[
                (1, 7, 100),
                (2, 7, 100),
                (1, 8, 100),
                (1, 7, 40100),
                (1, 8, 160100),
                (2, 7, 200100),
            ],
        )
        .unwrap();
        let summary = std::fs::read_to_string(dir.join("resumo.csv")).unwrap();
        for expected in ["1,0x7,1,4.000", "1,0x8,1,16.000", "2,0x7,1,20.000"] {
            assert!(summary.contains(expected));
        }
        for f in ["resumo.csv", "apresentacoes.csv"] {
            std::fs::remove_file(dir.join(f)).unwrap();
        }
        std::fs::remove_dir(dir).unwrap();
    }
}
