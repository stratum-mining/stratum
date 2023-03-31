use tokio::sync::mpsc;

#[derive(Debug)]
pub enum State {
    TemplateProviderShutdown(String),
    DownstreamShutdown(String),
    Healthy(String),
}

#[derive(Debug)]
pub struct Status {
    pub state: State,
}

#[tokio::main]
async fn main() {
    let (mut status_sender, mut status_receiver) = mpsc::channel::<Status>(32);

    while let Some(task_status) = status_receiver.recv().await {
        let Status { state } = task_status;

        match state {
            State::DownstreamShutdown(err) => {
                eprintln!(
                    "SHUTDOWN from Downstream: {}\nTry to restart the downstream listener",
                    err
                );
                let new_status = Status { state };
                let _ = status_sender.send(new_status).await;
                break;
            }
            State::TemplateProviderShutdown(err) => {
                eprintln!("SHUTDOWN from Upstream: {}\nTry to reconnecting or connecting to a new upstream", err);
                let new_status = Status { state };
                let _ = status_sender.send(new_status).await;
                break;
            }
            State::Healthy(msg) => {
                println!("HEALTHY message: {}", msg);
            }
        }
    }
}
