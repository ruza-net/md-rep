use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::{Read, Seek, Write};
use std::path::Path;

struct Settings {
    ctx_len: usize,
}

struct Entry {
    time: chrono::DateTime<chrono::FixedOffset>,
    score: i64,
    found: bool,
}

struct FileIndex {
    name: String,
    blanks: HashMap<(usize, String), Entry>,
}
impl FileIndex {
    fn deserialize(s: &str) -> Option<Self> {
        let header = regex::Regex::new(r"^\[(.*)\]").unwrap();
        let m = header.captures(s).unwrap();
        let name = m.get(1).unwrap().as_str().to_string();
        let mut idx = Self {
            name,
            blanks: HashMap::new(),
        };

        // Parse each entry
        //
        for line in s[m.get(0).unwrap().end() + 1..].lines() {
            let split: Vec<_> = line.splitn(4, "@").map(str::trim).collect();
            if split.len() != 4 {
                println!("Invalid line format: {:?}", line);
                return None;
            };
            let [date, score, line_no, blank] = split[..] else {
                unreachable!()
            };
            match chrono::DateTime::parse_from_rfc3339(date.trim()) {
                Ok(time) => {
                    let score: i64 = score.parse().unwrap();
                    let line_no: usize = line_no.parse().unwrap();
                    idx.blanks.insert(
                        (line_no, blank.to_string()),
                        Entry {
                            time,
                            score,
                            found: false,
                        },
                    );
                }

                Err(e) => {
                    println!("{:?}", e);
                    println!(
                        "The entry `{} @ {}` has invalid repetition date: {:?}",
                        line_no,
                        blank,
                        date.trim()
                    );
                    return None;
                }
            }
        }
        Some(idx)
    }
    fn serialize(&self, dest: &mut impl Write) {
        dest.write_all(format!("[{}]\n", self.name).as_bytes())
            .unwrap();

        let date_now = chrono::Local::now();
        // .with_time(chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap())
        // .unwrap();
        for ((line_no, blank), entry) in &self.blanks {
            dest.write_all(date_now.to_rfc3339().as_bytes()).unwrap();
            dest.write_all(b" @ ").unwrap();
            dest.write_all(entry.score.to_string().as_bytes()).unwrap();
            dest.write_all(b" @ ").unwrap();
            dest.write_all(line_no.to_string().as_bytes()).unwrap();
            dest.write_all(b" @ ").unwrap();
            dest.write_all(blank.as_bytes()).unwrap();
            dest.write_all(b"\n").unwrap();
        }
    }
    fn rep(&mut self) {
        let Ok(mut file) = File::open(&self.name) else {
            println!("Missing file: {}", self.name);
            return;
        };
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();

        println!("@@@ {} @@@", self.name);

        let stdin = io::stdin();
        let ctx_len = 2;

        // Find blanks to repeat
        //
        let blank = regex::Regex::new(r"\?\[(([^\[\]]|\\\[\\\])*)\]").unwrap();
        let lines: Vec<_> = text.lines().collect();
        for (line_no, line) in lines.iter().enumerate() {
            let matches: Vec<_> = blank.captures_iter(line).collect();
            if matches.len() == 0 {
                continue;
            }
            let today = chrono::Local::now();

            // Find blanks in line and filter out those that haven't expired yet.
            //
            let mut blanks = vec![];
            let mut line_segs = vec![];
            let mut last_end = 0;
            for (i, m) in matches.iter().enumerate() {
                let start = m.get_match().start();
                let end = m.get_match().end();
                let ans = m.get(1).unwrap().as_str();

                // Append the segment from last blank
                //
                line_segs.push(line[last_end..start].to_string());
                last_end = end;

                // Check if blank expired
                //
                let key = (line_no, ans.to_string());
                if self.blanks.contains_key(&key) {
                    let Entry { time, score, found } = self.blanks.get_mut(&key).unwrap();
                    let time = *time;
                    let score = *score;
                    *found = true;
                    if today < repetition_date(time, score) {
                        line_segs.push("...".to_string());
                        continue;
                    }
                }

                // Blank expired
                //
                blanks.push(ans);
                line_segs.push(format!("____({})", i + 1));
            }
            line_segs.push(line[last_end..].to_string());

            if blanks.is_empty() {
                continue;
            }

            // Print some lines before the blank
            //
            println!("@-------");
            let ctx_start = line_no.saturating_sub(ctx_len);
            let ctx_end = (line_no + ctx_len).min(lines.len());
            print!(
                "{}",
                lines[ctx_start..line_no]
                    .iter()
                    .map(|l| String::from("  ") + &blank.replace_all(l, "...") + "\n")
                    .fold(String::new(), |a, b| a + &b)
            );

            // Print line with blanks
            //
            println!("{}", line_segs.join(""));

            // Print some lines after the blank
            //
            print!(
                "{}",
                lines[line_no + 1..ctx_end]
                    .iter()
                    .map(|l| String::from("  ") + &blank.replace_all(l, "...") + "\n")
                    .fold(String::new(), |a, b| a + &b)
            );
            println!("@-------");

            for (mut i, blank) in blanks.iter().enumerate() {
                i += 1;

                // Query an answer
                //
                print!("{} > ", i);
                io::stdout().flush().unwrap();
                let mut answer = String::new();
                stdin.read_line(&mut answer).unwrap();

                // Diff to expected
                //
                println!("{} + {}", " ".repeat(i.ilog10() as usize + 1), blank);

                // User feedback
                //
                print!("@ Did you get it correct? [Y/n] ");
                io::stdout().flush().unwrap();
                answer.clear();
                stdin.read_line(&mut answer).unwrap();
                let score = if is_yes(&answer, true) { 1 } else { -1 };

                // Update score
                //
                let key = (line_no, blank.to_string());
                let time = chrono::Local::now().fixed_offset();
                if self.blanks.contains_key(&key) {
                    let entry = self.blanks.get_mut(&key).unwrap();
                    entry.score += score;
                    entry.time = time;
                    entry.found = true;
                } else {
                    self.blanks.insert(
                        key,
                        Entry {
                            time,
                            score,
                            found: true,
                        },
                    );
                }
            }
        }
        self.blanks.retain(|_key, entry| entry.found);
    }
}

fn repetition_date(
    last_rep: chrono::DateTime<chrono::FixedOffset>,
    score: i64,
) -> chrono::DateTime<chrono::FixedOffset> {
    last_rep + chrono::Duration::minutes(2.3f32.powi(score as i32) as i64)
}

fn is_yes(s: &str, empty_is_yes: bool) -> bool {
    if empty_is_yes && s.trim().is_empty() {
        return true;
    }
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
            print!("`{}` not found. Do you want to create it? [y/n] ", name);
            io::stdout().flush().unwrap();

            let stdin = io::stdin();
            let mut buf = String::new();
            stdin.read_line(&mut buf).unwrap();

            let answer = buf.to_lowercase();
            if is_yes(&answer, false) {
                println!("OK");
                let path = Path::new(name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                File::create(path).unwrap();
                OpenOptions::new().read(true).append(true).open(name).ok()
            } else {
                None
            }
        }
    }
}

fn main() {
    // File indices
    //
    let mut files = vec![];

    let mut rep_idx_buf = String::new();
    {
        let Some(mut rep_idx) = open_or_create(".rep/index") else {
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

            files.push(FileIndex {
                name: path.to_string(),
                blanks: HashMap::new(),
            });
        }

        rep_idx.seek(io::SeekFrom::Start(0)).unwrap();
        rep_idx.read_to_string(&mut rep_idx_buf).unwrap();
    }
    let mut new_idx = File::create(".rep/index").unwrap();

    // Deserialize file indices
    //
    let file_headers: Vec<_> = regex::Regex::new(r"^\[.*\]\n")
        .unwrap()
        .find_iter(&rep_idx_buf)
        .collect();
    if file_headers.len() == 1 {
        if let Some(idx) = FileIndex::deserialize(&rep_idx_buf) {
            files.push(idx);
        } else {
            new_idx.write_all(rep_idx_buf.as_bytes()).unwrap();
            return;
        }
    }
    for w in file_headers.windows(2) {
        let file_idx_src = &rep_idx_buf[w[0].start()..w[1].start()];
        if let Some(idx) = FileIndex::deserialize(file_idx_src) {
            files.push(idx);
        } else {
            new_idx.write_all(rep_idx_buf.as_bytes()).unwrap();
            return;
        }
    }

    // Perform repetition
    //
    for idx in files.iter_mut() {
        idx.rep();
    }

    // Serialize file indices
    //
    for idx in files {
        idx.serialize(&mut new_idx);
    }
}
