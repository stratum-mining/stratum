use binary_sv2::{Deserialize, Serialize};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{ChildStderr, ChildStdout, Command},
    time::timeout,
};
use tracing::info;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputLocation {
    StdOut,
    StdErr,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ExternalCommandCondition {
    /// String that output must contain in order to fail or pass
    pub output_string: String,
    /// Where the string should be (stderr, stdout)
    pub output_location: OutputLocation,
    /// if true and out contain string continue the test
    pub condition: bool,
    /// if true the condition is checked after initialization
    pub late_condition: bool,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum ExternalCommandConditions {
    /// Just run the command and return
    None,
    /// timer: Number of second after we panic and the test fail
    WithConditions {
        conditions: Vec<ExternalCommandCondition>,
        timer_secs: u64,
        warn_no_panic: bool,
    },
}

impl ExternalCommandConditions {
    #[allow(dead_code)]
    pub fn new_with_timer_secs(secs: u64) -> Self {
        Self::WithConditions {
            conditions: vec![],
            timer_secs: secs,
            warn_no_panic: false,
        }
    }

    #[allow(dead_code)]
    pub fn warn_no_panic(self) -> Self {
        match self {
            ExternalCommandConditions::WithConditions {
                conditions,
                timer_secs,
                ..
            } => Self::WithConditions {
                conditions,
                timer_secs,
                warn_no_panic: true,
            },
            ExternalCommandConditions::None => panic!("Expect conditions"),
        }
    }

    #[allow(dead_code)]
    pub fn continue_if_std_out_have(self, to_check: &str) -> Self {
        let condition = ExternalCommandCondition {
            output_string: to_check.to_string(),
            output_location: OutputLocation::StdOut,
            condition: true,
            late_condition: false,
        };
        match self {
            ExternalCommandConditions::WithConditions {
                mut conditions,
                timer_secs,
                warn_no_panic,
            } => {
                conditions.push(condition);
                Self::WithConditions {
                    conditions,
                    timer_secs,
                    warn_no_panic,
                }
            }
            ExternalCommandConditions::None => panic!("Expect with condition self"),
        }
    }

    #[allow(dead_code)]
    pub fn fail_if_std_out_have(self, to_check: &str) -> Self {
        let condition = ExternalCommandCondition {
            output_string: to_check.to_string(),
            output_location: OutputLocation::StdOut,
            condition: false,
            late_condition: false,
        };
        match self {
            ExternalCommandConditions::WithConditions {
                mut conditions,
                timer_secs,
                warn_no_panic,
            } => {
                conditions.push(condition);
                Self::WithConditions {
                    conditions,
                    timer_secs,
                    warn_no_panic,
                }
            }
            ExternalCommandConditions::None => panic!("Expect with condition self"),
        }
    }

    #[allow(dead_code)]
    pub fn continue_if_std_err_have(self, to_check: &str) -> Self {
        let condition = ExternalCommandCondition {
            output_string: to_check.to_string(),
            output_location: OutputLocation::StdErr,
            condition: true,
            late_condition: false,
        };
        match self {
            ExternalCommandConditions::WithConditions {
                mut conditions,
                timer_secs,
                warn_no_panic,
            } => {
                conditions.push(condition);
                Self::WithConditions {
                    conditions,
                    timer_secs,
                    warn_no_panic,
                }
            }
            ExternalCommandConditions::None => panic!("Expect with condition self"),
        }
    }

    #[allow(dead_code)]
    pub fn fail_if_std_err_have(self, to_check: &str) -> Self {
        let condition = ExternalCommandCondition {
            output_string: to_check.to_string(),
            output_location: OutputLocation::StdErr,
            condition: false,
            late_condition: false,
        };
        match self {
            ExternalCommandConditions::WithConditions {
                mut conditions,
                timer_secs,
                warn_no_panic,
            } => {
                conditions.push(condition);
                Self::WithConditions {
                    conditions,
                    timer_secs,
                    warn_no_panic,
                }
            }
            ExternalCommandConditions::None => panic!("Expect with condition self"),
        }
    }

    #[allow(dead_code)]
    pub fn fail_if_anything_on_std_err(self) -> Self {
        let condition = ExternalCommandCondition {
            output_string: "".to_string(),
            output_location: OutputLocation::StdErr,
            condition: false,
            late_condition: false,
        };
        match self {
            ExternalCommandConditions::WithConditions {
                mut conditions,
                timer_secs,
                warn_no_panic,
            } => {
                conditions.push(condition);
                Self::WithConditions {
                    conditions,
                    timer_secs,
                    warn_no_panic,
                }
            }
            ExternalCommandConditions::None => panic!("Expect with condition self"),
        }
    }

    fn check_condition(&self, output: String, location: OutputLocation, is_late: bool) -> bool {
        match self {
            ExternalCommandConditions::WithConditions {
                conditions,
                warn_no_panic,
                ..
            } => {
                for condition in conditions {
                    if output.contains(&condition.output_string)
                        && condition.output_location == location
                        && condition.late_condition == is_late
                    {
                        match condition.condition {
                            true => return true,
                            false => match warn_no_panic {
                                true => todo!(),
                                false => panic!(),
                            },
                        }
                    };
                }
                false
            }
            ExternalCommandConditions::None => {
                panic!("Try to take conditions but no conditions in self")
            }
        }
    }

    fn get_timer(&self) -> Duration {
        match self {
            ExternalCommandConditions::WithConditions { timer_secs, .. } => {
                Duration::from_secs(*timer_secs)
            }
            ExternalCommandConditions::None => {
                panic!("Try to take timer but no conditions in self")
            }
        }
    }

    async fn check_std_out_(&self, std_out: &mut ChildStdout, is_late: bool) {
        let mut reader = BufReader::new(std_out).lines();
        loop {
            let line = match reader.next_line().await.unwrap() {
                Some(line) => {
                    info!("STD OUT: {}", line);
                    line
                }
                None => return,
            };
            if self.check_condition(line, OutputLocation::StdOut, is_late) {
                return;
            }
        }
    }
    pub async fn check_std_out(&self, std_out: &mut ChildStdout, is_late: bool) -> Result<(), ()> {
        let seconds = match is_late {
            true => self.get_timer(),
            false => Duration::from_secs(u64::MAX),
        };
        timeout(seconds, self.check_std_out_(std_out, is_late))
            .await
            .map_err(|_| {
                if !self.get_warn_no_panic() {
                    panic!()
                };
            })
    }

    async fn check_std_err_(&self, std_err: &mut ChildStderr, is_late: bool) {
        let mut reader = BufReader::new(std_err).lines();
        loop {
            let line = match reader.next_line().await.unwrap() {
                Some(line) => {
                    if !line.contains("unused manifest key") {
                        info!("STD ERR: {}", line);
                    }
                    line
                }
                None => return,
            };
            if self.check_condition(line, OutputLocation::StdErr, is_late) {
                return;
            }
        }
    }
    pub async fn check_std_err(&self, std_err: &mut ChildStderr, is_late: bool) -> Result<(), ()> {
        let seconds = match is_late {
            true => self.get_timer(),
            false => Duration::from_secs(u64::MAX),
        };
        timeout(seconds, self.check_std_err_(std_err, is_late))
            .await
            .map_err(|_| {
                if !self.get_warn_no_panic() {
                    panic!()
                };
            })
    }
    fn get_warn_no_panic(&self) -> bool {
        match self {
            ExternalCommandConditions::None => panic!("Expect conditions"),
            ExternalCommandConditions::WithConditions { warn_no_panic, .. } => *warn_no_panic,
        }
    }
}

pub async fn os_command(
    command_: &str,
    args: Vec<&str>,
    conditions_: ExternalCommandConditions,
) -> Option<tokio::process::Child> {
    let mut command = Command::new(command_);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.kill_on_drop(true);
    for arg in args.clone() {
        command.arg(arg);
    }

    let mut child = if args.len() == 2 && command_ == "cargo" {
        command.spawn().unwrap()
    } else {
        command.current_dir("../../roles").spawn().unwrap()
    };
    debug_assert!(child.stdout.is_some());
    debug_assert!(child.stderr.is_some());
    match &conditions_ {
        ExternalCommandConditions::WithConditions { .. } => {
            let mut stdout = child.stdout.take().unwrap();
            let mut stderr = child.stderr.take().unwrap();

            match tokio::select! {
                  r = conditions_.check_std_out(&mut stdout,false) => r,
                  r = conditions_.check_std_err(&mut stderr,false) => r,
            } {
                Ok(_) => {
                    tokio::task::spawn(async move {
                        tokio::select! {
                              r = conditions_.check_std_out(&mut stdout,true) => r,
                              r = conditions_.check_std_err(&mut stderr,true) => r,
                        }
                    });
                    Some(child)
                }
                Err(_) => None,
            }
        }
        ExternalCommandConditions::None => Some(child),
    }
}
