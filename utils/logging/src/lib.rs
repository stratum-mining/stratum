// Pruned copy of crate rust log, without global logger
// https://github.com/rust-lang-nursery/log #7a60286
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

//! Log traits live here, which are called throughout the library to provide useful information for
//! debugging purposes.
//!
//! There is currently 2 ways to filter log messages. First one, by using compilation features, e.g "max_level_off".
//! The second one, client-side by implementing check against Record Level field.
//! Each module may have its own Logger or share one.

use core::{cmp, fmt};

static LOG_LEVEL_NAMES: [&str; 6] = ["GOSSIP", "TRACE", "DEBUG", "INFO", "WARN", "ERROR"];

/// An enum representing the available verbosity levels of the logger.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum Level {
    /// Designates extremely verbose information, including gossip-induced messages
    Gossip,
    /// Designates very low priority, often extremely verbose, information
    Trace,
    /// Designates lower priority information
    Debug,
    /// Designates useful information
    Info,
    /// Designates hazardous situations
    Warn,
    /// Designates very serious errors
    Error,
}

impl PartialOrd for Level {
    #[inline]
    fn partial_cmp(&self, other: &Level) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }

    #[inline]
    fn lt(&self, other: &Level) -> bool {
        (*self as usize) < *other as usize
    }

    #[inline]
    fn le(&self, other: &Level) -> bool {
        *self as usize <= *other as usize
    }

    #[inline]
    fn gt(&self, other: &Level) -> bool {
        *self as usize > *other as usize
    }

    #[inline]
    fn ge(&self, other: &Level) -> bool {
        *self as usize >= *other as usize
    }
}

impl Ord for Level {
    #[inline]
    fn cmp(&self, other: &Level) -> cmp::Ordering {
        (*self as usize).cmp(&(*other as usize))
    }
}

impl fmt::Display for Level {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.pad(LOG_LEVEL_NAMES[*self as usize])
    }
}

impl Level {
    /// Returns the most verbose logging level.
    #[inline]
    pub fn max() -> Level {
        Level::Gossip
    }
}

/// A Record, unit of logging output with Metadata to enable filtering
/// Module_path, file, line to inform on log's source
#[derive(Clone, Debug)]
pub struct Record<'a> {
    /// The verbosity level of the message.
    pub level: Level,
    /// The message body.
    pub args: fmt::Arguments<'a>,
    /// The module path of the message.
    pub module_path: &'static str,
    /// The source file containing the message.
    pub file: &'static str,
    /// The line containing the message.
    pub line: u32,
}

impl<'a> Record<'a> {
    /// Returns a new Record.
    #[inline]
    pub fn new(
        level: Level,
        args: fmt::Arguments<'a>,
        module_path: &'static str,
        file: &'static str,
        line: u32,
    ) -> Record<'a> {
        Record {
            level,
            args,
            module_path,
            file,
            line,
        }
    }
}

/// A trait encapsulating the operations required of a logger
pub trait Logger {
    /// Logs the `Record`
    fn log(&self, record: &Record);
}

/// Wrapper for logging byte slices in hex format.
#[doc(hidden)]
pub struct DebugBytes<'a>(pub &'a [u8]);
impl<'a> fmt::Display for DebugBytes<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        for i in self.0 {
            write!(f, "{:02x}", i)?;
        }
        Ok(())
    }
}

/// Logs a byte slice in hex format.
#[macro_export]
macro_rules! log_bytes {
    ($obj: expr) => {
        DebugBytes(&$obj)
    };
}

/// Create a new Record and log it. You probably don't want to use this macro directly,
/// but it needs to be exported so `log_trace` etc can use it in external crates.
#[doc(hidden)]
#[macro_export]
macro_rules! log_internal {
	($logger: expr, $lvl:expr, $($arg:tt)+) => (
		$logger.log(&Record::new($lvl, format_args!($($arg)+), module_path!(), file!(), line!()))
	);
}

/// Logs an entry at the given level.
#[doc(hidden)]
#[macro_export]
macro_rules! log_given_level {
	($logger: expr, $lvl:expr, $($arg:tt)+) => (
		match $lvl {
			#[cfg(not(any(feature = "max_level_off")))]
			Level::Error => log_internal!($logger, $lvl, $($arg)*),
			#[cfg(not(any(feature = "max_level_off", feature = "max_level_error")))]
			Level::Warn => log_internal!($logger, $lvl, $($arg)*),
			#[cfg(not(any(feature = "max_level_off", feature = "max_level_error", feature = "max_level_warn")))]
			Level::Info => log_internal!($logger, $lvl, $($arg)*),
			#[cfg(not(any(feature = "max_level_off", feature = "max_level_error", feature = "max_level_warn", feature = "max_level_info")))]
			Level::Debug => log_internal!($logger, $lvl, $($arg)*),
			#[cfg(not(any(feature = "max_level_off", feature = "max_level_error", feature = "max_level_warn", feature = "max_level_info", feature = "max_level_debug")))]
			Level::Trace => log_internal!($logger, $lvl, $($arg)*),
			#[cfg(not(any(feature = "max_level_off", feature = "max_level_error", feature = "max_level_warn", feature = "max_level_info", feature = "max_level_debug", feature = "max_level_trace")))]
			Level::Gossip => log_internal!($logger, $lvl, $($arg)*),

			#[cfg(any(feature = "max_level_off", feature = "max_level_error", feature = "max_level_warn", feature = "max_level_info", feature = "max_level_debug", feature = "max_level_trace"))]
			_ => {
				// The level is disabled at compile-time
			},
		}
	);
}

/// Log at the `ERROR` level.
#[macro_export]
macro_rules! log_error {
	($logger: expr, $($arg:tt)*) => (
		log_given_level!($logger, Level::Error, $($arg)*);
	)
}

/// Log at the `WARN` level.
#[macro_export]
macro_rules! log_warn {
	($logger: expr, $($arg:tt)*) => (
		log_given_level!($logger, Level::Warn, $($arg)*);
	)
}

/// Log at the `INFO` level.
#[macro_export]
macro_rules! log_info {
	($logger: expr, $($arg:tt)*) => (
		log_given_level!($logger, Level::Info, $($arg)*);
	)
}

/// Log at the `DEBUG` level.
#[macro_export]
macro_rules! log_debug {
	($logger: expr, $($arg:tt)*) => (
		log_given_level!($logger, Level::Debug, $($arg)*);
	)
}

/// Log at the `TRACE` level.
#[macro_export]
macro_rules! log_trace {
	($logger: expr, $($arg:tt)*) => (
		log_given_level!($logger, Level::Trace, $($arg)*)
	)
}

/// Log at the `GOSSIP` level.
#[macro_export]
macro_rules! log_gossip {
	($logger: expr, $($arg:tt)*) => (
		log_given_level!($logger, Level::Gossip, $($arg)*);
	)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    pub struct TestLogger {
        level: Level,
        pub(crate) id: String,
        pub lines: Mutex<HashMap<(String, String), usize>>,
    }

    impl TestLogger {
        pub fn new() -> TestLogger {
            Self::with_id("".to_owned())
        }
        pub fn with_id(id: String) -> TestLogger {
            TestLogger {
                level: Level::Trace,
                id,
                lines: Mutex::new(HashMap::new()),
            }
        }
        pub fn enable(&mut self, level: Level) {
            self.level = level;
        }
        pub fn assert_log(&self, module: String, line: String, count: usize) {
            let log_entries = self.lines.lock().unwrap();
            assert_eq!(log_entries.get(&(module, line)), Some(&count));
        }

        /// Search for the number of occurrence of the logged lines which
        /// 1. belongs to the specified module and
        /// 2. contains `line` in it.
        /// And asserts if the number of occurrences is the same with the given `count`
        pub fn assert_log_contains(&self, module: String, line: String, count: usize) {
            let log_entries = self.lines.lock().unwrap();
            let l: usize = log_entries
                .iter()
                .filter(|&(&(ref m, ref l), _c)| m == &module && l.contains(line.as_str()))
                .map(|(_, c)| c)
                .sum();
            assert_eq!(l, count)
        }

        /// Search for the number of occurrences of logged lines which
        /// 1. belong to the specified module and
        /// 2. match the given regex pattern.
        /// Assert that the number of occurrences equals the given `count`
        pub fn assert_log_regex(&self, module: String, pattern: regex::Regex, count: usize) {
            let log_entries = self.lines.lock().unwrap();
            let l: usize = log_entries
                .iter()
                .filter(|&(&(ref m, ref l), _c)| m == &module && pattern.is_match(&l))
                .map(|(_, c)| c)
                .sum();
            assert_eq!(l, count)
        }
    }

    impl Logger for TestLogger {
        fn log(&self, record: &Record) {
            *self
                .lines
                .lock()
                .unwrap()
                .entry((record.module_path.to_string(), format!("{}", record.args)))
                .or_insert(0) += 1;
            if record.level >= self.level {
                #[cfg(feature = "std")]
                println!(
                    "{:<5} {} [{} : {}, {}] {}",
                    record.level.to_string(),
                    self.id,
                    record.module_path,
                    record.file,
                    record.line,
                    record.args
                );
            }
        }
    }

    #[test]
    fn test_level_show() {
        assert_eq!("INFO", Level::Info.to_string());
        assert_eq!("ERROR", Level::Error.to_string());
        assert_ne!("WARN", Level::Error.to_string());
    }

    struct WrapperLog {
        logger: Arc<dyn Logger>,
    }

    impl WrapperLog {
        fn new(logger: Arc<dyn Logger>) -> WrapperLog {
            WrapperLog { logger }
        }

        fn call_macros(&self) {
            log_error!(self.logger, "This is an error");
            log_warn!(self.logger, "This is a warning");
            log_info!(self.logger, "This is an info");
            log_info!(
                self.logger,
                "bytes: {}",
                log_bytes!("This is bytes".as_bytes())
            );
            log_debug!(self.logger, "This is a debug");
            log_trace!(self.logger, "This is a trace");
            log_gossip!(self.logger, "This is a gossip");
        }
    }

    #[test]
    fn test_logging_macros() {
        let mut logger = TestLogger::new();
        logger.enable(Level::Gossip);
        let logger: Arc<dyn Logger> = Arc::new(logger);
        let wrapper = WrapperLog::new(Arc::clone(&logger));
        wrapper.call_macros();
    }

    #[test]
    fn test_log_ordering() {
        assert!(Level::Error > Level::Warn);
        assert!(Level::Error >= Level::Warn);
        assert!(Level::Error >= Level::Error);
        assert!(Level::Warn > Level::Info);
        assert!(Level::Warn >= Level::Info);
        assert!(Level::Warn >= Level::Warn);
        assert!(Level::Info > Level::Debug);
        assert!(Level::Info >= Level::Debug);
        assert!(Level::Info >= Level::Info);
        assert!(Level::Debug > Level::Trace);
        assert!(Level::Debug >= Level::Trace);
        assert!(Level::Debug >= Level::Debug);
        assert!(Level::Trace > Level::Gossip);
        assert!(Level::Trace >= Level::Gossip);
        assert!(Level::Trace >= Level::Trace);
        assert!(Level::Gossip >= Level::Gossip);

        assert!(Level::Error <= Level::Error);
        assert!(Level::Warn < Level::Error);
        assert!(Level::Warn <= Level::Error);
        assert!(Level::Warn <= Level::Warn);
        assert!(Level::Info < Level::Warn);
        assert!(Level::Info <= Level::Warn);
        assert!(Level::Info <= Level::Info);
        assert!(Level::Debug < Level::Info);
        assert!(Level::Debug <= Level::Info);
        assert!(Level::Debug <= Level::Debug);
        assert!(Level::Trace < Level::Debug);
        assert!(Level::Trace <= Level::Debug);
        assert!(Level::Trace <= Level::Trace);
        assert!(Level::Gossip < Level::Trace);
        assert!(Level::Gossip <= Level::Trace);
        assert!(Level::Gossip <= Level::Gossip);
    }
}
