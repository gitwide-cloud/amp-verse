//! Isolated fake yt-dlp for import/cancel tests. Never shipped in application packages.
use std::{env,fs,io::{self,Write},path::PathBuf,process::Command,thread,time::Duration};
fn main(){
    let args:Vec<_>=env::args().collect();
    if args.get(1).map(String::as_str)==Some("--heartbeat"){
        let path=PathBuf::from(&args[2]);
        loop{if let Ok(mut file)=fs::OpenOptions::new().create(true).append(true).open(&path){let _=file.write_all(b"beat\n");}thread::sleep(Duration::from_millis(30));}
    }
    let exe=env::current_exe().unwrap();let name=exe.file_stem().unwrap().to_string_lossy();
    assert!(args.iter().any(|a|a=="--no-playlist"));assert!(args.iter().any(|a|a=="--ignore-config"));
    if args.iter().any(|a|a=="--flat-playlist") {
        assert!(args.iter().any(|a|a=="--yes-playlist"));
        println!("{{\"entries\":[{{\"id\":\"BaW_jenozKc\"}},{{\"id\":\"abcdefghijk\"}}]}}");return;
    }
    if args.last().unwrap().ends_with("v=abcdefghijk") {eprintln!("ERROR: Private video");std::process::exit(1);}
    assert_eq!(args.last().unwrap(),"https://www.youtube.com/watch?v=BaW_jenozKc");
    if name.contains("network"){eprintln!("ERROR: network timed out");std::process::exit(1);}
    if name.contains("private"){eprintln!("ERROR: Private video");std::process::exit(1);}
    if args.iter().any(|a|a=="--dump-single-json"){
        println!("{{\"id\":\"BaW_jenozKc\",\"title\":\"Test: audio / source\",\"is_live\":false}}");return;
    }
    let index=args.iter().position(|a|a=="-o").unwrap();let template=PathBuf::from(&args[index+1]);let work=template.parent().unwrap();
    if name.contains("cancel"){
        let heartbeat=work.parent().unwrap().join("heartbeat");
        let _child=Command::new(&exe).arg("--heartbeat").arg(heartbeat).spawn().unwrap();
        fs::write(work.join("source.part"),b"partial").unwrap();
        loop{println!("AMP_PROGRESS: 5%");io::stdout().flush().unwrap();thread::sleep(Duration::from_millis(50));}
    }
    let source=work.join("source.wav");
    if name.contains("invalid"){fs::write(&source,b"not audio").unwrap();}
    else{fs::copy(exe.parent().unwrap().join("fixture.wav"),&source).unwrap();}
    println!("AMP_PROGRESS: 50%");println!("AMP_PROGRESS: 100%");
    let quoted=source.to_string_lossy().replace('\\',"\\\\").replace('"',"\\\"");
    println!("AMP_FILE:\"{quoted}\"");
}
