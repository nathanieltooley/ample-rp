use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use log::{Level, LevelFilter};
use regex::Regex;
use simplelog::{Color, ColorChoice, CombinedLogger, ConfigBuilder, TermLogger, TerminalMode, WriteLogger};

const MAX_FILE_SIZE: u64 = 1_000_000 * 5; // ~5MB
const MAX_FILES: u64 = 3;

struct RollingLogger {
    log_dir: PathBuf,
    log_file_name: PathBuf,
    max_file_size: u64,
    max_files: u64,
    file_prefix: &'static str,
    log_buf: Vec<u8>,
}

impl RollingLogger {
    fn new(log_dir: PathBuf, file_name: PathBuf, max_file_size: u64, max_files: u64) -> RollingLogger {
        RollingLogger {
            log_dir,
            log_file_name: file_name,
            max_file_size,
            max_files,
            file_prefix: "ample",
            log_buf: Vec::new(),
        }
    }

    fn get_log_files(&self) -> Result<Vec<RollingLogFile>, io::Error> {
        let mut files = Vec::new();
        let dir_files = fs::read_dir(&self.log_dir)?;

        for res in dir_files {
            let entry = res?;
            // skip invalid names
            let entry_name = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(_) => continue,
            };

            let re = Regex::new(&format!(r"{}-?(\d*).log", self.file_prefix)).expect("invalid regex");
            // If the log file has an ID in its name
            if let Some(caps) = re.captures(&entry_name) {
                if let Some(m) = caps.get(1) {
                    if !m.is_empty() {
                        let index = match m.as_str().parse::<u64>() {
                            Ok(i) => i,
                            Err(_) => continue,
                        };

                        files.push(RollingLogFile { file_id: index });

                        continue;
                    }
                }
            }

            // If the log file does not have an ID in its name but still matchs "[file_prefix].log"
            if re.is_match(&entry_name) {
                files.push(RollingLogFile { file_id: 0 });
            }
        }

        Ok(files)
    }

    fn increment_logs(&self, mut log_files: Vec<RollingLogFile>) -> Result<(), io::Error> {
        log_files.sort_by(|a, b| b.file_id.cmp(&a.file_id));
        // rename all log files to temp-[prefix]-[log_id].log
        for log_file in log_files.iter_mut() {
            let log_file_name = log_file.create_log_name(self.file_prefix, &self.log_dir);
            let temp_file_name = log_file.create_log_name(&format!("temp-{}", self.file_prefix), &self.log_dir);
            fs::rename(&log_file_name, temp_file_name)?;
        }

        // increment each log id and rename the temp log files with their new id
        for log_file in log_files.iter_mut() {
            let temp_file_name = log_file.create_log_name(&format!("temp-{}", self.file_prefix), &self.log_dir);
            log_file.file_id += 1;

            fs::rename(temp_file_name, log_file.create_log_name(self.file_prefix, &self.log_dir))?;
        }

        // create the index 0 base log
        File::create(self.log_dir.join(format!("{}.log", self.file_prefix)))?;

        // remove extra log file
        if (log_files.len() + 1) as u64 > self.max_files {
            let log_to_remove = log_files.first().unwrap();
            fs::remove_file(log_to_remove.create_log_name(self.file_prefix, &self.log_dir))?;
        }

        Ok(())
    }

    fn open_log_file(&self) -> Result<File, io::Error> {
        OpenOptions::new()
            .append(true)
            .read(true)
            .create(true)
            .open(self.log_dir.join(&self.log_file_name))
    }
}

impl Write for RollingLogger {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let amount_written = self.log_buf.write(buf)?;

        // new line!
        if buf.len() == 1 && *buf.last().unwrap() == b'\n' {
            self.flush()?;
        }

        Ok(amount_written)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner_file = self.open_log_file()?;
        let expected_size = inner_file.metadata()?.len() + self.log_buf.len() as u64;
        let drain: Vec<u8> = self.log_buf.drain(..).collect();

        if expected_size > self.max_file_size {
            // Shift logs by 1
            self.increment_logs(self.get_log_files()?)?;
            // Open new and empty ample.log file
            inner_file = self.open_log_file()?;
        }

        inner_file.write(&drain)?;

        inner_file.flush()
    }
}

struct RollingLogFile {
    file_id: u64,
}

impl RollingLogFile {
    fn create_log_name(&self, prefix: &str, directory: &Path) -> PathBuf {
        if self.file_id == 0 {
            directory.join(format!("{prefix}.log"))
        } else {
            directory.join(format!("{prefix}-{}.log", self.file_id))
        }
    }
}

fn create_rolling_logger() -> io::Result<RollingLogger> {
    // Should create something like "/AppData/ample/config/logs" on windows
    // and "~/.config/ample/logs" on linux
    let log_dir = directories::ProjectDirs::from("", "", crate::APP_NAME)
        .expect("valid project dir")
        .config_dir()
        .join("logs");

    fs::create_dir_all(&log_dir)?;

    Ok(RollingLogger::new(
        log_dir,
        PathBuf::from_str("ample.log").unwrap(),
        MAX_FILE_SIZE,
        MAX_FILES,
    ))
}

pub fn init_log(log_level: LevelFilter) -> Result<(), io::Error> {
    let log_file = create_rolling_logger()?;
    // only possible error is initting twice
    let _ = CombinedLogger::init(vec![
        TermLogger::new(
            log_level,
            ConfigBuilder::new()
                .set_location_level(LevelFilter::Debug)
                .set_level_color(Level::Error, Some(Color::Red))
                .build(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(log_level, ConfigBuilder::new().set_location_level(LevelFilter::Debug).build(), log_file),
    ]);

    Ok(())
}
