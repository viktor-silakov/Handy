use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use transcribe_rs::TranscriptionEngine;
use transcribe_rs::engines::{
    gigaam::GigaAMEngine,
    whisper::WhisperEngine,
    moonshine::{MoonshineEngine, MoonshineModelParams, ModelVariant},
    parakeet::{ParakeetEngine, ParakeetModelParams},
    sense_voice::{SenseVoiceEngine, SenseVoiceModelParams},
};

enum EngineWrapper {
    GigaAM(GigaAMEngine),
    Whisper(WhisperEngine),
    Moonshine(MoonshineEngine),
    Parakeet(ParakeetEngine),
    SenseVoice(SenseVoiceEngine),
}

impl EngineWrapper {
    fn transcribe_samples(&mut self, audio: Vec<f32>) -> Result<transcribe_rs::TranscriptionResult, Box<dyn std::error::Error>> {
        match self {
            EngineWrapper::GigaAM(e) => e.transcribe_samples(audio, None).map_err(|e| e.into()),
            EngineWrapper::Whisper(e) => e.transcribe_samples(audio, None).map_err(|e| e.into()),
            EngineWrapper::Moonshine(e) => e.transcribe_samples(audio, None).map_err(|e| e.into()),
            EngineWrapper::Parakeet(e) => e.transcribe_samples(audio, None).map_err(|e| e.into()),
            EngineWrapper::SenseVoice(e) => e.transcribe_samples(audio, None).map_err(|e| e.into()),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    
    // Usage: rust-infer <engine_type> <model_path>
    if args.len() < 3 {
        eprintln!("Usage: rust-infer <engine_type> <model_path>");
        eprintln!("Engines: gigaam, whisper, moonshine, parakeet, sensevoice");
        std::process::exit(1);
    }

    let engine_type = args[1].to_lowercase();
    let model_path = &args[2];

    if !PathBuf::from(model_path).exists() {
        eprintln!("Model file not found: {}", model_path);
        std::process::exit(1);
    }

    eprintln!("Loading {} engine with model {}...", engine_type, model_path);

    let mut engine = match engine_type.as_str() {
        "gigaam" => {
            let mut e = GigaAMEngine::new();
            e.load_model(Path::new(model_path))?;
            EngineWrapper::GigaAM(e)
        }
        "whisper" => {
            let mut e = WhisperEngine::new();
            e.load_model(Path::new(model_path))?;
            EngineWrapper::Whisper(e)
        }
        "moonshine" => {
            let mut e = MoonshineEngine::new();
            // Use Base as default for remote
            e.load_model_with_params(Path::new(model_path), MoonshineModelParams::variant(ModelVariant::Base))?;
            EngineWrapper::Moonshine(e)
        }
        "parakeet" => {
            let mut e = ParakeetEngine::new();
            e.load_model_with_params(Path::new(model_path), ParakeetModelParams::int8())?;
            EngineWrapper::Parakeet(e)
        }
        "sensevoice" => {
            let mut e = SenseVoiceEngine::new();
            e.load_model_with_params(Path::new(model_path), SenseVoiceModelParams::int8())?;
            EngineWrapper::SenseVoice(e)
        }
        _ => {
            eprintln!("Unknown engine type: {}", engine_type);
            std::process::exit(1);
        }
    };

    eprintln!("Model loaded. Ready to transcribe.");
    println!("READY");
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

        match read_wav(line) {
            Ok(samples) => {
                match engine.transcribe_samples(samples) {
                    Ok(result) => {
                        let json = serde_json::json!({ "status": "success", "text": result.text });
                        println!("{}", json.to_string());
                    }
                    Err(e) => {
                        let json = serde_json::json!({ "status": "error", "error": format!("Transcription failed: {}", e) });
                        println!("{}", json.to_string());
                    }
                }
            }
            Err(e) => {
                let json = serde_json::json!({ "status": "error", "error": format!("Failed to read WAV: {}", e) });
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
                    samples.push(sample? as f32 / i16::MAX as f32);
                }
            } else {
                return Err("Only 16-bit integer WAV is supported".into());
            }
        }
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
    if spec.channels > 1 {
        let channels = spec.channels as usize;
        let mut mono = Vec::with_capacity(samples.len() / channels);
        for chunk in samples.chunks(channels) {
            mono.push(chunk.iter().sum::<f32>() / channels as f32);
        }
        samples = mono;
    }
    Ok(samples)
}
