use std::process::Stdio;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

pub async fn os_command(
    command: &str,
    args: Vec<&str>,
    condition: Option<&str>,
) -> tokio::process::Child {
    let mut command = Command::new(command);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());
    command.kill_on_drop(true);
    for arg in args {
        command.arg(arg);
    }

    let mut child = command.spawn().unwrap();
    debug_assert!(child.stdout.is_some());
    match condition {
        Some(condition) => {
            let stdout = child.stdout.take().unwrap();
            let mut stdout_reader = BufReader::new(stdout).lines();
            loop {
                let line = match dbg!(stdout_reader.next_line().await.unwrap()) {
                    Some(line) => line,
                    None => panic!("TODO this should print the stderr"),
                };
                if line.contains(condition) {
                    child.stdout = Some(stdout_reader.into_inner().into_inner());
                    break;
                }
            }
        }
        None => (),
    }
    debug_assert!(child.stdout.is_some());
    child
}
