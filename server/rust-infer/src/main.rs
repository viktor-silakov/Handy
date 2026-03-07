use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use transcribe_rs::{engines::gigaam::GigaAMEngine, TranscriptionEngine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // We get model and config path from args or default
    let args: Vec<String> = std::env::args().collect();
    
    let model_path = if args.len() > 1 {
        args[1].clone()
    } else {
        let mut d = std::env::current_dir()?;
        d.push("models");
        d.push("gigaam.onnx");
        d.to_string_lossy().to_string()
    };
    
    // Auto-download logic or print error if missing
    if !PathBuf::from(&model_path).exists() {
        eprintln!("Model file not found: {}. Please ensure model file exists.", model_path);
        std::process::exit(1);
    }
    
    eprintln!("Loading GigaAM model from {}...", model_path);
    
    let mut engine = GigaAMEngine::new();
    engine.load_model(std::path::Path::new(&model_path)).map_err(|e| format!("Failed to load GigaAM model: {}", e))?;
    
    eprintln!("Model loaded. Ready to transcribe.");
    println!("READY"); // Signal to Node.js that we are ready
    io::stdout().flush()?;
    
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        
        if line == "EXIT" {
            break;
        }
        
        // Line format: "file_path"
        let wav_path = line;
        
        // Read the file and convert to f32
        match read_wav(wav_path) {
            Ok(samples) => {
                match engine.transcribe_samples(samples, None) {
                    Ok(result) => {
                        let json = serde_json::json!({
                            "status": "success",
                            "text": result.text
                        });
                        println!("{}", json.to_string());
                    },
                    Err(e) => {
                        let json = serde_json::json!({
                            "status": "error",
                            "error": format!("Transcription failed: {}", e)
                        });
                        println!("{}", json.to_string());
                    }
                }
            },
            Err(e) => {
                let json = serde_json::json!({
                    "status": "error",
                    "error": format!("Failed to read WAV: {}", e)
                });
                println!("{}", json.to_string());
            }
        }
        io::stdout().flush()?;
    }
    
    Ok(())
}

fn read_wav(path: &str) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    
    let mut samples = Vec::new();
    match spec.sample_format {
        hound::SampleFormat::Int => {
            if spec.bits_per_sample == 16 {
                for sample in reader.samples::<i16>() {
                    let s = sample? as f32 / i16::MAX as f32;
                    samples.push(s);
                }
            } else {
                return Err("Only 16-bit integer WAV is supported".into());
            }
        },
        hound::SampleFormat::Float => {
            if spec.bits_per_sample == 32 {
                for sample in reader.samples::<f32>() {
                    samples.push(sample?);
                }
            } else {
                return Err("Only 32-bit float WAV is supported".into());
            }
        }
    }
    
    // Multi-channel to mono (simple average)
    if spec.channels > 1 {
        let channels = spec.channels as usize;
        let mut mono = Vec::with_capacity(samples.len() / channels);
        for chunk in samples.chunks(channels) {
            let sum: f32 = chunk.iter().sum();
            mono.push(sum / channels as f32);
        }
        samples = mono;
    }
    
    Ok(samples)
}
