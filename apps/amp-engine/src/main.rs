//! One request per process. The engine owns shared leases and publication.
use amp_core::engine_api::{PROTOCOL, Request, Response};
use std::io::{self, Read};
fn execute() -> Result<Response, String> {
    let mut input = Vec::new();
    io::stdin()
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut input)
        .map_err(|e| e.to_string())?;
    if input.len() > 16 * 1024 * 1024 {
        return Err("Engine request too large".into());
    }
    let request: Request = serde_json::from_slice(&input).map_err(|e| e.to_string())?;
    if request.protocol != PROTOCOL {
        return Err("Unsupported analysis protocol; update the add-on".into());
    }
    let preparation = if let Some(draft) = request.draft {
        amp_core::coordination::analyze_draft(&request.root, &request.relative, &draft)?
    } else {
        match amp_core::coordination::analyze_shared(&request.root, &request.relative, false)? {
            amp_core::coordination::AnalysisResult::Prepared(p) => *p,
            amp_core::coordination::AnalysisResult::Skipped(_) => {
                return Ok(Response::Skipped { protocol: PROTOCOL });
            }
        }
    };
    Ok(Response::Prepared {
        protocol: PROTOCOL,
        preparation: Box::new(preparation),
    })
}
fn main() {
    let response = execute().unwrap_or_else(|message| Response::Error {
        protocol: PROTOCOL,
        message,
    });
    if let Err(error) = serde_json::to_writer(io::stdout().lock(), &response) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
