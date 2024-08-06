use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::{Command, exit};
use blake3::Hasher;
use serde::{Serialize, Deserialize};
use indexmap::IndexMap;

#[derive(Serialize, Deserialize)]
struct Hashes {
    inputs: HashMap<String, String>,
    outputs: HashMap<String, String>,
}

fn hash_file(file_path: &str) -> String {
    let mut file = match fs::File::open(file_path) {
        Ok(file) => file,
        Err(_) => return "doesnotexist".to_string(),
    };
    let mut hasher = Hasher::new();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).expect(format!("Unable to read file {}", file_path).as_str());
    hasher.update(&buffer);
    hasher.finalize().to_hex().to_string()
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let (input_files, output_files, command) = parse_args(&args)?;

    // IndexMap so that insertion order is preserved. This is important since we just dump the map
    // to calculate the hash.
    let mut d = IndexMap::new();
    d.insert("inputs", &input_files);
    d.insert("outputs", &output_files);
    d.insert("command", &command);

    let hash_args = {
        let mut hasher = Hasher::new();
        hasher.update(format!("{:?}", d).as_bytes());
        hasher.finalize().to_hex().to_string()
    };

    let cache_dir = dirs::cache_dir().expect("Could not find a cache directory.").join("binboh");
    fs::create_dir_all(&cache_dir)?;
    let cache_file = cache_dir.join(format!("{}.json", hash_args));

    let previous_run: Option<Hashes> = if cache_file.exists() {
        let file = fs::File::open(&cache_file)?;
        serde_json::from_reader(file).ok()
    } else {
        None
    };

    let mut rerun = false;
    if let Some(prev) = previous_run {
        for input_file in &input_files {
            if hash_file(input_file) != prev.inputs[input_file] {
                rerun = true;
                break;
            }
        }
        if !rerun {
            for output_file in &output_files {
                if hash_file(output_file) != prev.outputs[output_file] {
                    rerun = true;
                    break;
                }
            }
        }
    } else {
        rerun = true;
    }

    if !rerun {
        println!("Result of {} is already cached.", command.join(" "));
        return Ok(());
    }

    let status = Command::new(&command[0])
        .args(&command[1..])
        .status()
        .expect("Failed to execute command");

    if !status.success() {
        exit(status.code().unwrap_or(1));
    }

    let hashes = Hashes {
        inputs: input_files.iter().map(|f| (f.clone(), hash_file(f))).collect(),
        outputs: output_files.iter().map(|f| (f.clone(), hash_file(f))).collect(),
    };

    let file = fs::File::create(cache_file)?;
    serde_json::to_writer(file, &hashes)?;

    Ok(())
}

fn parse_args(args: &[String]) -> io::Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut command = Vec::new();
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "-i" | "--inputs" => {
                i += 1;
                while i < args.len() && !args[i].starts_with('-') {
                    inputs.push(args[i].clone());
                    i += 1;
                }
            }
            "-o" | "--outputs" => {
                i += 1;
                while i < args.len() && !args[i].starts_with('-') {
                    outputs.push(args[i].clone());
                    i += 1;
                }
            }
            "--" => {
                i += 1;
                while i < args.len() {
                    command.push(args[i].clone());
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }

    Ok((inputs, outputs, command))
}
