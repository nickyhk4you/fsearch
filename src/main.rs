use clap::Parser;
use std::fs;
use std::path::Path;
use std::io::{self, BufRead};
use regex::Regex;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use memmap2::Mmap;

const LARGE_FILE_THRESHOLD: u64 = 10_000_000; // 10MB

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Directory to search in (defaults to current directory)
    #[arg(short, long, default_value = ".")]
    directory: String,

    /// File extension to search (if not specified, searches all files)
    #[arg(short, long)]
    extension: Option<String>,

    /// Term to search for (supports regex)
    #[arg(short, long)]
    term: String,

    /// Search recursively in subdirectories
    #[arg(short, long, default_value_t = true)]
    recursive: bool,

    /// Case sensitive search
    #[arg(short = 'c', long, default_value_t = false)]
    case_sensitive: bool,

    /// Use regex for searching
    #[arg(short = 'x', long, default_value_t = false)]
    regex: bool,

    /// Number of threads for parallel search
    #[arg(short = 't', long, default_value_t = 4)]
    threads: usize,
}

#[derive(Debug)]
struct SearchResult {
    file_path: String,
    line_number: usize,
    line: String,
    matches: Vec<(usize, usize)>, // start and end positions of matches
}

fn main() {
    let args = Args::parse();

    rayon::ThreadPoolBuilder::new()
        .num_threads(args.threads)
        .build_global()
        .unwrap();

    let pattern = if args.regex {
        Regex::new(&args.term).expect("Invalid regex pattern")
    } else {
        Regex::new(&regex::escape(&args.term)).unwrap()
    };

    match search_files(&args.directory, &args.extension, &pattern, &args) {
        Ok(results) => display_results(results),
        Err(e) => eprintln!("{}", format!("Error: {}", e).red()),
    }
}

fn search_files(
    directory: &str,
    file_extension: &Option<String>,
    pattern: &Regex,
    args: &Args,
) -> io::Result<Vec<SearchResult>> {
    let mut all_files = Vec::new();
    collect_files(Path::new(directory), file_extension, args.recursive, &mut all_files)?;

    let pb = ProgressBar::new(all_files.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
        .unwrap()
        .progress_chars("#>-"));

    let results: Vec<SearchResult> = all_files.par_iter()
        .filter_map(|path| {
            let result = search_in_file(path, pattern, args);
            pb.inc(1);
            result.ok()
        })
        .flatten()
        .collect();

    pb.finish_with_message("Search completed");
    Ok(results)
}

fn should_search_file(path: &Path, extension: &Option<String>) -> bool {
    if let Some(ext) = extension {
        if let Some(file_ext) = path.extension() {
            return file_ext.to_string_lossy().to_string() == *ext;
        }
        false
    } else {
        path.is_file()
    }
}

fn collect_files(
    dir: &Path,
    extension: &Option<String>,
    recursive: bool,
    files: &mut Vec<String>,
) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && should_search_file(&path, extension) {
                if let Some(path_str) = path.to_str() {
                    files.push(path_str.to_string());
                }
            } else if recursive && path.is_dir() {
                collect_files(&path, extension, recursive, files)?;
            }
        }
    }
    Ok(())
}

fn search_in_file(
    file_path: &str,
    pattern: &Regex,
    args: &Args,
) -> io::Result<Vec<SearchResult>> {
    let file = fs::File::open(file_path)?;
    let metadata = file.metadata()?;

    if metadata.len() > LARGE_FILE_THRESHOLD {
        search_in_large_file(file_path, pattern, args)
    } else {
        search_in_small_file(file, file_path, pattern, args)
    }
}

fn search_in_small_file(
    file: fs::File,
    file_path: &str,
    pattern: &Regex,
    args: &Args,
) -> io::Result<Vec<SearchResult>> {
    let reader = io::BufReader::new(file);
    let mut results = Vec::new();

    for (line_number, line) in reader.lines().enumerate() {
        let line = line?;
        let line_to_search = if args.case_sensitive {
            line.clone()
        } else {
            line.to_lowercase()
        };

        let matches: Vec<_> = pattern.find_iter(&line_to_search)
            .map(|m| (m.start(), m.end()))
            .collect();

        if !matches.is_empty() {
            results.push(SearchResult {
                file_path: file_path.to_string(),
                line_number: line_number + 1,
                line,
                matches,
            });
        }
    }

    Ok(results)
}

fn search_in_large_file(
    file_path: &str,
    pattern: &Regex,
    args: &Args,
) -> io::Result<Vec<SearchResult>> {
    let file = fs::File::open(file_path)?;
    let mmap = unsafe { Mmap::map(&file)? };

    let content = String::from_utf8_lossy(&mmap);
    let lines: Vec<&str> = content.lines().collect();

    let results: Vec<SearchResult> = lines.par_iter()
        .enumerate()
        .filter_map(|(line_number, &line)| {
            let line_to_search = if args.case_sensitive {
                line.to_string()
            } else {
                line.to_lowercase()
            };

            let matches: Vec<_> = pattern.find_iter(&line_to_search)
                .map(|m| (m.start(), m.end()))
                .collect();

            if matches.is_empty() {
                None
            } else {
                Some(SearchResult {
                    file_path: file_path.to_string(),
                    line_number: line_number + 1,
                    line: line.to_string(),
                    matches,
                })
            }
        })
        .collect();

    Ok(results)
}

fn display_results(results: Vec<SearchResult>) {
    if results.is_empty() {
        println!("{}", "No matches found.".yellow());
        return;
    }

    println!("\n{} matches found:\n", results.len().to_string().green());

    for result in results {
        println!("{}:{} {}",
                 result.file_path.blue(),
                 result.line_number.to_string().yellow(),
                 highlight_matches(&result.line, &result.matches)
        );
    }
}

fn highlight_matches(line: &str, matches: &[(usize, usize)]) -> String {
    let mut result = String::new();
    let mut last_end = 0;

    for &(start, end) in matches {
        result.push_str(&line[last_end..start]);
        result.push_str(&line[start..end].on_yellow().to_string());
        last_end = end;
    }
    result.push_str(&line[last_end..]);

    result
}
