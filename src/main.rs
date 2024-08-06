use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Read;
use std::process::{Command, exit};
use blake3::Hasher;
use clap::Parser;
use serde::{Serialize, Deserialize};
use indexmap::IndexMap;
use anyhow::{Result, Context, bail};

#[derive(Serialize, Deserialize)]
struct Hashes {
    inputs: HashMap<String, String>,
    outputs: HashMap<String, String>,
}

/// Building INcrementally Based On Hashes
///
/// binboh is a tool to cache the results of a command based on the hashes of the input and output files.
///
/// Example: binboh -i input.txt -o output.txt -- mycommand -arg1 -arg2
///
/// In this case, binboh will run mycommand -arg1 -arg2 if input.txt or output.txt have changed since the last run.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Input files. Missing files are ignored.
    ///
    /// Specifying no input files is valid. In this case, the command will only run when outputs
    /// change.
    ///
    /// These files are assumed to not be changed by the command itself and are only hashed before
    /// the command is run.
    ///
    /// The more precisely you specify the inputs, the more powerful the caching will be.
    /// For example, if the command runs a script, and you specify the script as an input, the
    /// command will run if the script changes.
    #[clap(short, long = "input-files", value_name = "FILE", num_args=1..)]
    inputs: Vec<String>,

    /// Output files. Missing files are ignored.
    ///
    /// Specifying no output files is valid. In this case, the command will only run when inputs
    /// change.
    #[clap(short, long = "output-files", value_name = "FILE", num_args=1..)]
    outputs: Vec<String>,

    /// Print debug information.
    #[clap(long)]
    verbose: bool,

    /// Command to run.
    ///
    /// The first argument is the binary to be called, the rest are arguments to that binary.
    /// You can specify the command after a double dash to avoid parsing issues.
    #[clap(num_args=1.., required=true, last=true, value_name = "COMMAND")]
    command: Vec<String>,
}

impl Args {
    fn hash(&self) -> Result<String> {
        // IndexMap so that insertion order is preserved. This is important since we just dump the map
        // to calculate the hash.
        let mut d = IndexMap::new();
        let pwd = vec![env::current_dir()
            .with_context(|| "Failed to get current directory")?
            .to_string_lossy().to_string()];
        d.insert("pwd", &pwd);
        d.insert("inputs", &self.inputs);
        d.insert("outputs", &self.outputs);
        d.insert("command", &self.command);

        let mut hasher = Hasher::new();
        hasher.update(format!("{:?}", d).as_bytes());

        if self.verbose {
            eprintln!("Hashing working directory path: {:?}", &pwd);
            eprintln!("Hashing inputs files paths: {:?}", &self.inputs);
            eprintln!("Hashing outputs files paths: {:?}", &self.outputs);
            eprintln!("Hashing command: {:?}", &self.command);
            eprintln!("Hash: {}", hasher.clone().finalize().to_hex());
        }

        Ok(hasher.finalize().to_hex().to_string())
    }

    fn needs_to_run(&self, input_hashes: &HashMap<String, String>, previous_run: Option<&Hashes>) -> bool {
        if let Some(prev) = previous_run {
            for input_file in &self.inputs {
                if input_hashes[input_file] != prev.inputs[input_file] {
                    self.if_verbose(|| eprintln!("Input file hash differs: {}", input_file));
                    return true;
                }
                self.if_verbose(|| eprintln!("Input file hash matches: {}", input_file));
            }
            for output_file in &self.outputs {
                if self.hash_file(output_file, Some("doesnotexist")).unwrap() != prev.outputs[output_file] {
                    self.if_verbose(|| eprintln!("Output file hash differs: {}", output_file));
                    return true;
                }
                self.if_verbose(|| eprintln!("Output file hash matches: {}", output_file));
            }
        } else {
            self.if_verbose(|| eprintln!("No previous run found. Need to rerun."));
            return true;
        }
        return false;
    }

    fn if_verbose(&self, f: impl FnOnce()) {
        if self.verbose {
            f();
        }
    }

    fn hash_file(&self, file_path: &str, fallback: Option<&str>) -> Result<String> {
        self.if_verbose(|| eprintln!("Hashing file content: {}", file_path));
        let mut file = match fs::File::open(file_path) {
            Ok(file) => file,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    match fallback {
                        Some(f) => {
                            self.if_verbose(|| eprintln!("File does not exist. Using fallback for hashing: {}", file_path));
                            return Ok(f.to_string())
                        },
                        None => bail!("File does not exist: {}", file_path)
                    }
                } else {
                    bail!("Failed to open file for hashing {}: {}", file_path, e)
                }
            }
        };
        let mut hasher = Hasher::new();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .with_context(|| format!("Failed to read file {}", file_path))?;
        hasher.update(&buffer);
        let hash = hasher.finalize().to_hex().to_string();
        self.if_verbose(|| eprintln!("Hash: {}", hash));
        Ok(hash)
    }
}

fn main() -> Result<()> {
    let call = Args::parse();
    if call.command.is_empty() {
        bail!("No command specified");
    }

    let cache_dir = dirs::cache_dir()
        .with_context(|| "Could not find the user's cache directory.")?
        .join("binboh");

    let call_hash = call.hash()?;
    let cache_file = cache_dir
        .join(call_hash[0..2].to_string())
        .join(call_hash[2..4].to_string())
        .join(format!("{}.json", call_hash));

    let previous_run: Option<Hashes> = if cache_file.exists() {
        call.if_verbose(|| eprintln!("Loading previous run from: {}", cache_file.to_string_lossy()));
        let file = fs::File::open(&cache_file)
            .with_context(|| format!("Failed to open hash file {}", cache_file.to_string_lossy()))?;
        serde_json::from_reader(file).ok()
    } else {
        call.if_verbose(|| eprintln!("Previous run not found: {}", cache_file.to_string_lossy()));
        None
    };

    let input_hashes = call.inputs.iter().map(|f| call.hash_file(f, Some("doesnotexist")).map(|h| (f.clone(), h))).collect::<Result<HashMap<String,String>>>()?;
    if !call.needs_to_run(&input_hashes, previous_run.as_ref()) {
        println!("Skipped: {}", call.command.join(" "));
        return Ok(());
    }

    call.if_verbose(|| eprintln!("Running command: {}", call.command.join(" ")));
    let status = Command::new(&call.command[0])
        .args(&call.command[1..])
        .status()
        .with_context(|| format!("Failed to run command {}", call.command.join(" ")))?;

    if !status.success() {
        exit(status.code().unwrap_or(1));
    }

    let hashes = Hashes {
        inputs:  input_hashes,
        outputs: call.outputs.iter().map(|f| call.hash_file(f, Some("doesnotexist")).map(|h| (f.clone(), h))).collect::<Result<HashMap<String,String>>>()?,
    };

    call.if_verbose(|| eprintln!("Writing hashes to: {}", cache_file.to_string_lossy()));
    fs::create_dir_all(cache_file.parent().unwrap())
        .with_context(|| format!("Failed to create cache directory {}", cache_file.parent().unwrap().to_string_lossy()))?;
    let file = fs::File::create(&cache_file)
        .with_context(|| format!("Failed to create hash file {}", cache_file.to_string_lossy()))?;
    serde_json::to_writer(file, &hashes)
        .with_context(|| format!("Failed to write hashes to file {}", cache_file.to_string_lossy()))?;

    Ok(())
}

