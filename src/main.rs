use std::env;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::{Read, Seek, Write};
use std::path::Path;

fn is_yes(s: &str) -> bool {
    let s = s.to_lowercase();
    match s.trim() {
        "yes" | "y" => true,
        _ => {
            println!("Taking {:?} as a no...", s.trim());
            false
        }
    }
}
fn open_or_create(name: &str) -> Option<File> {
    match OpenOptions::new().read(true).append(true).open(name) {
        Ok(f) => Some(f),

        Err(_) => {
            print!("`{}` not found. Do you want to create it? ", name);
            io::stdout().flush().unwrap();

            let stdin = io::stdin();
            let mut buf = String::new();
            stdin.read_line(&mut buf).unwrap();

            let answer = buf.to_lowercase();
            if is_yes(&answer) {
                println!("OK");
                let path = Path::new(name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                File::create(path).unwrap();
                File::open(path).ok()
            } else {
                None
            }
        }
    }
}

fn rep_entry_write(dest: &mut impl Write, score: i64, path: &str) {
    let date_now = chrono::Local::now()
        .with_time(chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap())
        .unwrap();
    dest.write_all(date_now.to_rfc3339().as_bytes()).unwrap();
    dest.write_all(b" @ ").unwrap();
    dest.write_all(&score.to_string().as_bytes()).unwrap();
    dest.write_all(b" @ ").unwrap();
    dest.write_all(path.as_bytes()).unwrap();
    dest.write_all(b"\n").unwrap();
}

fn main() {
    let mut rep_idx_buf = String::new();
    {
        let Some(mut rep_idx) = open_or_create(".rep/rep_index") else {
            return;
        };
        // TODO: set ctx_len and so on based on settings
        //
        let Some(settings) = open_or_create(".rep/settings") else {
            return;
        };

        let args: Vec<_> = env::args().collect();
        if let Some("add") = args.get(1).map(String::as_str) {
            let Some(path) = args.get(2) else {
                println!("Usage: md-rep add (path to add to rep list)");
                return;
            };
            // TODO: check if path present in rep_index

            rep_entry_write(&mut rep_idx, 0, path);
        }

        rep_idx.seek(io::SeekFrom::Start(0)).unwrap();
        rep_idx.read_to_string(&mut rep_idx_buf).unwrap();
    }
    let today = chrono::Local::now();
    let mut new_idx = File::create(".rep/rep_index").unwrap();

    // TODO: refactor
    //
    for line in rep_idx_buf.lines() {
        let split: Vec<_> = line.splitn(3, "@").map(str::trim).collect();
        if split.len() != 3 {
            println!("Invalid line format: {:?}", line);
            new_idx.write_all(line.as_bytes()).unwrap();
            new_idx.write_all(b"\n").unwrap();
            continue;
        };
        let [date, offset, file] = split[..] else {
            unreachable!()
        };
        match chrono::DateTime::parse_from_rfc3339(date.trim()) {
            Ok(time) => {
                let offset: i64 = offset.parse().unwrap();
                if today >= time + chrono::Duration::days(offset) {
                    let score = rep(file);
                    rep_entry_write(&mut new_idx, score + offset, file);
                } else {
                    new_idx.write_all(line.as_bytes()).unwrap();
                    new_idx.write_all(b"\n").unwrap();
                }
            }

            Err(e) => {
                println!("{:?}", e);
                println!(
                    "The entry `{}` has invalid repetition date: {:?}",
                    file,
                    date.trim()
                );
                new_idx.write_all(line.as_bytes()).unwrap();
                new_idx.write_all(b"\n").unwrap();
            }
        }
    }
}

fn rep(name: &str) -> i64 {
    let Ok(mut file) = File::open(name) else {
        println!("Missing file: {}", name);
        return 0;
    };
    let mut text = String::new();
    file.read_to_string(&mut text).unwrap();

    println!("@@@ {} @@@", name);

    let stdin = io::stdin();
    let ctx_len = 2;

    let mut score = 0i64;

    // Find blanks to repeat
    //
    let blank = regex::Regex::new(r"\?\[(([^\[\]]|\\\[\\\])*)\]").unwrap();
    let lines: Vec<_> = text.lines().collect();
    for (line_no, line) in lines.iter().enumerate() {
        // TODO: handle multiple blanks on a line
        //
        if let Some(m) = blank.captures(line) {
            // Print some lines before the blank
            //
            println!("@-------");

            let ctx_start = line_no.saturating_sub(ctx_len);
            let ctx_end = (line_no + ctx_len).min(lines.len());
            println!(
                "{}",
                lines[ctx_start..line_no]
                    .iter()
                    .map(|l| String::from("  ") + l)
                    .fold(String::new(), |a, b| a + &b)
            );
            let prefix = &line[..m.get(0).unwrap().start()];
            let suffix = &line[m.get(0).unwrap().end()..];
            println!("  {}{}{}", prefix, "____", suffix);

            // Print some lines after the blank
            //
            println!(
                "{}",
                lines[line_no + 1..ctx_end]
                    .iter()
                    .map(|l| String::from("  ") + l)
                    .fold(String::new(), |a, b| a + &b)
            );
            println!("@-------");

            // Query an answer
            //
            print!("> ");
            io::stdout().flush().unwrap();
            let mut answer = String::new();
            stdin.read_line(&mut answer).unwrap();

            // Diff to expected
            //
            println!("+ {}{}{}", prefix, m.get(1).unwrap().as_str(), suffix);
            println!("- {}{}{}", prefix, answer.trim(), suffix);

            // User feedback
            //
            print!("@ Did you get it correct? ");
            io::stdout().flush().unwrap();
            answer.clear();
            stdin.read_line(&mut answer).unwrap();
            if is_yes(&answer) {
                score += 1;
            } else {
                score -= 1;
            }
        }
    }

    score
}
