//! 一个没有第三方依赖的日志库。
//!
//! 日志可以输出到 stdout、stderr 或文件（三选一），
//! 由 [`LogLevel`] 控制哪些等级的日志会被真正写出。
//!
//! 日志格式为 `时间 等级 [文件:行号] -> 消息`，例如：
//!
//! ```text
//! 2026-07-18 16:41:12.052 INFO [src/server.rs:42] -> server listening on 127.0.0.1:8080
//! ```
//!
//! # 快速开始
//!
//! 通过 [`init_console`] 或 [`init_file`] 初始化一次全局 logger，
//! 之后即可使用 [`error!`](error)、[`info!`](info) 等宏输出日志：
//!
//! ```
//! logger::init_console();
//!
//! logger::error!("something failed: {}", "boom");
//! logger::warn!("low disk space");
//! logger::info!("server listening on {}:{}", "127.0.0.1", 8080);
//! logger::debug!("debug details: {:?}", vec![1, 2, 3]);
//! logger::trace!("very noisy");
//! ```
//!
//! 也可以不经过全局 logger，直接用 [`LogBuilder`] 创建独立的 [`Logger`] 实例。

use std::fmt::Arguments;
use std::fs;
use std::fs::File;
use std::io::{Stderr, Stdout, Write};
use std::panic::Location;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{fmt, io};

/// 日志等级过滤器。
///
/// 等级按 `Off < Error < Warn < Info < Debug < Trace` 排序：
/// 设置为某个等级时，只会输出不高于该等级的日志。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}
impl LogLevel {
    pub fn name(self) -> &'static str {
        match self {
            LogLevel::Off => "OFF",
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
            LogLevel::Trace => "TRACE",
        }
    }
}
impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (&mut *f).write_str(self.name())
    }
}

/// 日志输出目标的配置项，用于 [`LogBuilder::target`]。
pub enum LogTarget {
    /// 输出到文件，以追加方式写入；父目录不存在时会自动创建。
    File(PathBuf),
    /// 输出到标准输出。
    Stdout,
    /// 输出到标准错误。
    Stderr,
}

/// 日志输出目标。
#[derive(Debug)]
enum LogOutput {
    File(File),
    Stdout(Stdout),
    Stderr(Stderr),
}

/// 日志器。线程安全，可以安全地在线程间共享引用。
///
/// 通常不直接构造，而是用 [`LogBuilder`] 创建，
/// 或通过 [`init_console`] / [`init_file`] 初始化全局实例。
#[derive(Debug)]
pub struct Logger {
    level: LogLevel,
    output: Mutex<LogOutput>,
}

impl Logger {
    /// 判断给定等级的日志是否会被输出。
    pub fn enable(&self, level: LogLevel) -> bool {
        level != LogLevel::Off && level <= self.level
    }

    /// 按 `时间 等级 [文件:行号] -> 消息` 的格式写入一行日志。
    ///
    /// 写入失败（IO 错误、锁中毒等）时静默忽略，不影响调用方。
    fn log(&self, level: LogLevel, file: &'static str, line: u32, message: Arguments) {
        if !self.enable(level) {
            return;
        }
        let level: &str = level.name();
        let time: String = Time::format();
        let mut buffer: String = String::new();

        use std::fmt::Write;
        let _ = writeln!(
            buffer,
            "{} {} [{}:{}] -> {}",
            time, level, file, line, message
        );

        if let Ok(mut output) = self.output.lock() {
            let _ = match &mut *output {
                LogOutput::File(f) => (&mut *f).write_all(buffer.as_bytes()),
                LogOutput::Stdout(o) => (&mut *o).write_all(buffer.as_bytes()),
                LogOutput::Stderr(e) => (&mut *e).write_all(buffer.as_bytes()),
            };
        }
    }

    /// 输出 `Error` 等级的日志，通常配合 `format_args!` 使用。
    ///
    /// 自动记录调用处的文件和行号。
    #[track_caller]
    pub fn error(&self, args: Arguments) {
        let location: &Location = Location::caller();
        self.log(LogLevel::Error, location.file(), location.line(), args)
    }

    /// 输出 `Warn` 等级的日志，通常配合 `format_args!` 使用。
    ///
    /// 自动记录调用处的文件和行号。
    #[track_caller]
    pub fn warn(&self, args: Arguments) {
        let location: &Location = Location::caller();
        self.log(LogLevel::Warn, location.file(), location.line(), args)
    }

    /// 输出 `Info` 等级的日志，通常配合 `format_args!` 使用。
    ///
    /// 自动记录调用处的文件和行号。
    #[track_caller]
    pub fn info(&self, args: Arguments) {
        let location: &Location = Location::caller();
        self.log(LogLevel::Info, location.file(), location.line(), args)
    }

    /// 输出 `Debug` 等级的日志，通常配合 `format_args!` 使用。
    ///
    /// 自动记录调用处的文件和行号。
    #[track_caller]
    pub fn debug(&self, args: Arguments) {
        let location: &Location = Location::caller();
        self.log(LogLevel::Debug, location.file(), location.line(), args)
    }

    /// 输出 `Trace` 等级的日志，通常配合 `format_args!` 使用。
    ///
    /// 自动记录调用处的文件和行号。
    #[track_caller]
    pub fn trace(&self, args: Arguments) {
        let location: &Location = Location::caller();
        self.log(LogLevel::Trace, location.file(), location.line(), args)
    }
}

/// =========================
/// builder
/// =========================

/// [`Logger`] 的构建器。
///
/// 默认配置：等级 [`LogLevel::Info`]，输出到 stdout。
pub struct LogBuilder {
    level: LogLevel,
    target: LogTarget,
}
impl LogBuilder {
    /// 创建默认配置的构建器。
    pub fn new() -> Self {
        Self {
            level: LogLevel::Info,
            target: LogTarget::Stdout,
        }
    }

    /// 设置日志等级，只有不高于该等级的日志会被输出。
    pub fn level(mut self, level: LogLevel) -> Self {
        self.level = level;
        self
    }

    /// 设置输出目标。
    pub fn target(mut self, target: LogTarget) -> Self {
        self.target = target;
        self
    }

    /// 按当前配置创建 [`Logger`]。
    ///
    /// # Errors
    ///
    /// 目标为 [`LogTarget::File`] 时，创建父目录或打开文件失败会返回错误。
    pub fn create(self) -> io::Result<Logger> {
        let output: LogOutput = match self.target {
            LogTarget::Stdout => LogOutput::Stdout(io::stdout()),
            LogTarget::Stderr => LogOutput::Stderr(io::stderr()),
            LogTarget::File(path) => LogOutput::File(open_file(&path)?),
        };

        Ok(Logger {
            level: self.level,
            output: Mutex::new(output),
        })
    }
}

/// 以追加模式打开（或创建）日志文件，必要时先创建父目录。
fn open_file(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    File::options().create(true).append(true).open(path)
}

/// =========================
/// public api
/// =========================

/// 全局 logger，进程内唯一，由 [`init_console`] / [`init_file`] 初始化。
static GLOBAL_LOGGER: OnceLock<Logger> = OnceLock::new();

/// 初始化全局 logger：输出到 stdout，等级为 [`LogLevel::Debug`]。
///
/// 只有首次调用生效，之后的重复调用会被忽略。
///
/// # Panics
///
/// 创建 logger 失败时 panic（stdout 目标实际上不会失败）。
pub fn init_console() {
    let logger: Logger = LogBuilder::new()
        .target(LogTarget::Stdout)
        .level(LogLevel::Debug)
        .create()
        .unwrap();
    GLOBAL_LOGGER.get_or_init(|| logger);
}

/// 初始化全局 logger：以追加模式输出到指定文件，等级为 [`LogLevel::Debug`]。
///
/// 只有首次调用生效，之后的重复调用会被忽略。文件父目录不存在时会自动创建。
///
/// # Panics
///
/// 打开或创建日志文件失败时 panic。
pub fn init_file<P: AsRef<Path>>(path: P) {
    let file_path: PathBuf = path.as_ref().to_path_buf();
    let logger: Logger = LogBuilder::new()
        .target(LogTarget::File(file_path))
        .level(LogLevel::Debug)
        .create()
        .unwrap();
    GLOBAL_LOGGER.get_or_init(|| logger);
}

// `log!` 宏的实际实现；全局 logger 未初始化时静默丢弃日志。
#[doc(hidden)]
pub fn global_logger(level: LogLevel, file: &'static str, line: u32, args: Arguments) {
    if let Some(logger) = GLOBAL_LOGGER.get() {
        let _ = logger.log(level, file, line, args);
    }
}

/// 向全局 logger 输出指定等级的日志。
///
/// 第一个参数是 [`LogLevel`]；自动记录调用处的文件和行号。
/// 一般直接使用 [`error!`](error)、[`warn!`](warn) 等宏即可。
#[macro_export]
macro_rules! log {
    ($level:expr, $($arg:tt)*) => {
        $crate::global_logger($level, file!(), line!(), format_args!($($arg)*))
    };
}

/// 向全局 logger 输出 `Error` 日志。
///
/// 自动记录调用处的文件和行号。
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => { $crate::log!($crate::LogLevel::Error, $($arg)*) };
}

/// 向全局 logger 输出 `Warn` 日志。
///
/// 自动记录调用处的文件和行号。
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => { $crate::log!($crate::LogLevel::Warn, $($arg)*) };
}

/// 向全局 logger 输出 `Info` 日志。
///
/// 自动记录调用处的文件和行号。
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => { $crate::log!($crate::LogLevel::Info, $($arg)*) };
}

/// 向全局 logger 输出 `Debug` 日志。
///
/// 自动记录调用处的文件和行号。
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => { $crate::log!($crate::LogLevel::Debug, $($arg)*) };
}

/// 向全局 logger 输出 `Trace` 日志。
///
/// 自动记录调用处的文件和行号。
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => { $crate::log!($crate::LogLevel::Trace, $($arg)*) };
}

// ----------------------------------------------------------
// ---------------------------time---------------------------
// ----------------------------------------------------------

/// 时间工具：获取并格式化本地时间。
struct Time;
impl Time {
    /// 本地时区相对 UTC 的偏移（秒）。
    /// 固定为东八区（UTC+8），不可配置。
    const TZ_OFFSET_SECOND: i64 = 8 * 3600;

    /// 当前本地时间的格式化字符串，格式为 `YYYY-MM-DD HH:MM:SS.mmm`。
    pub fn format() -> String {
        let time = Time::now();
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
            time.0, time.1, time.2, time.3, time.4, time.5, time.6
        )
    }

    /// 当前本地时间，返回 `(年, 月, 日, 时, 分, 秒, 毫秒)`。
    pub fn now() -> (u32, u8, u8, u8, u8, u8, u16) {
        let duration: Duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);
        // UTC 秒数加上时区偏移，得到本地时间
        let time_ss: u64 = (duration.as_secs() as i64 + Time::TZ_OFFSET_SECOND) as u64;
        let time_day: i64 = (time_ss / 86400) as i64;
        let (year, month, day) = Time::civil_from_day(time_day);
        let rem: u32 = (time_ss % 86400) as u32;

        let hh: u8 = (rem / 3600) as u8;
        let mm: u8 = ((rem % 3600) / 60) as u8;
        let ss: u8 = (rem % 60) as u8;
        let ms: u16 = duration.subsec_millis() as u16;

        (year, month, day, hh, mm, ss, ms)
    }

    /// 将“自 Unix epoch 起的天数”转换为日期。
    ///
    /// 算法来自 Howard Hinnant 的 `civil_from_days`。
    fn civil_from_day(z: i64) -> (u32, u8, u8) {
        let z = z + 719468;
        let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
        let doe = (z - era * 146097) as u32;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe as i64 + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        (y as u32, m as u8, d as u8)
    }
}
