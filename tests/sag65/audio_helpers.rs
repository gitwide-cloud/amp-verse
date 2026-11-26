fn fixture(name: &str) -> Vec<[f32;2]> {
    let root=std::env::var_os("SAG65_FIXTURES").map(std::path::PathBuf::from)
        .unwrap_or_else(||std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/sag65-fixtures"));
    let output=amp_core::command("ffmpeg").args(["-nostdin","-v","error","-i"])
        .arg(root.join("wav").join(name)).args(["-ac","2","-ar","44100","-f","f32le","pipe:1"]).output().unwrap();
    assert!(output.status.success(),"generate tests/sag65 fixtures first");
    let (frames, _) = output.stdout.as_chunks::<8>();
    frames
        .iter()
        .map(|frame| {
            [
                f32::from_le_bytes(frame[..4].try_into().unwrap()),
                f32::from_le_bytes(frame[4..].try_into().unwrap()),
            ]
        })
        .collect()
}
fn verify_peak(label:&str,frames:&[[f32;2]]) {
    use std::io::Write;
    let mut raw=tempfile::NamedTempFile::new().unwrap();
    let clipped=frames.iter().flatten().filter(|x|x.abs()>=1.).count();
    assert_eq!(clipped,0);
    for frame in frames {for sample in frame {raw.write_all(&sample.to_le_bytes()).unwrap();}}
    raw.flush().unwrap();
    let output=amp_core::command("ffmpeg").args(["-nostdin","-hide_banner","-f","f32le","-ar","44100","-ac","2","-i"])
        .arg(raw.path()).args(["-af","ebur128=peak=true","-f","null","-"]).output().unwrap();
    assert!(output.status.success());
    let text=String::from_utf8_lossy(&output.stderr);
    let summary=text.rsplit("Summary:").next().unwrap();
    let peak:f64=summary.lines().find_map(|l|l.trim().strip_prefix("Peak:")?.split_whitespace().next()?.parse().ok()).unwrap();
    println!("{label}: clipped={clipped}, true_peak={peak} dBTP\n{summary}");
    assert!(peak<=-1.,"{label}: {peak} dBTP exceeds -1.0");
}
